use failure::{err_msg, format_err, Error};
use rusoto_core::{HttpClient, Region};
use rusoto_credential::{AutoRefreshingProvider, StaticProvider};
use rusoto_ec2::Ec2Client;
use rusoto_ecr::EcrClient;
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use std::collections::HashMap;
use std::env::var;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

macro_rules! get_client_sts {
    ($T:ty, $region:expr) => {
        StsInstance::new(None).and_then(|sts| {
            let client = match sts.get_provider() {
                Some(provider) => <$T>::new_with(HttpClient::new()?, provider, $region),
                None => <$T>::new($region),
            };
            Ok(client)
        })
    };
}

#[derive(Clone)]
pub struct StsInstance {
    sts_client: StsClient,
    region: Region,
    aws_access_key_id: String,
    aws_secret_access_key: String,
    role_arn: Option<String>,
}

impl Default for StsInstance {
    fn default() -> Self {
        Self {
            sts_client: StsClient::new(Region::UsEast1),
            region: Region::UsEast1,
            aws_access_key_id: "".to_string(),
            aws_secret_access_key: "".to_string(),
            role_arn: None,
        }
    }
}

impl StsInstance {
    pub fn new(profile_name: Option<&str>) -> Result<Self, Error> {
        let profiles = AwsProfileInfo::fill_profile_map()?;
        let profile_name = match profile_name {
            Some(n) => n.to_string(),
            None => var("AWS_PROFILE")
                .ok()
                .unwrap_or_else(|| "default".to_string()),
        };
        let current_profile = profiles
            .get(&profile_name)
            .ok_or_else(|| format_err!("No such profile: {}", profile_name))?;

        let region = current_profile
            .region
            .as_ref()
            .and_then(|reg| reg.parse().ok())
            .unwrap_or(Region::UsEast1);
        let (key, secret) = match current_profile.source_profile.as_ref() {
            Some(prof) => {
                let source_profile = profiles
                    .get(prof)
                    .ok_or_else(|| format_err!("Source profile {} doesn't exist", prof))?;
                (
                    source_profile
                        .aws_access_key_id
                        .as_ref()
                        .ok_or_else(|| err_msg("No aws_access_key"))?,
                    source_profile
                        .aws_secret_access_key
                        .as_ref()
                        .ok_or_else(|| err_msg("No aws_secret_key"))?,
                )
            }
            None => (
                current_profile
                    .aws_access_key_id
                    .as_ref()
                    .ok_or_else(|| err_msg("No aws_access_key"))?,
                current_profile
                    .aws_secret_access_key
                    .as_ref()
                    .ok_or_else(|| err_msg("No aws_secret_key"))?,
            ),
        };
        let provider = StaticProvider::new_minimal(key.to_string(), secret.to_string());

        Ok(Self {
            sts_client: StsClient::new_with(HttpClient::new()?, provider, region.clone()),
            region,
            aws_access_key_id: key.to_string(),
            aws_secret_access_key: secret.to_string(),
            role_arn: current_profile.role_arn.clone(),
        })
    }

    pub fn get_provider(
        &self,
    ) -> Option<AutoRefreshingProvider<StsAssumeRoleSessionCredentialsProvider>> {
        self.role_arn.as_ref().and_then(|role_arn| {
            AutoRefreshingProvider::new(StsAssumeRoleSessionCredentialsProvider::new(
                self.sts_client.clone(),
                role_arn.to_string(),
                "default".to_string(),
                None,
                None,
                None,
                None,
            ))
            .ok()
        })
    }

    pub fn get_ec2_client(&self, region: Region) -> Result<Ec2Client, Error> {
        get_client_sts!(Ec2Client, region)
    }

    pub fn get_ecr_client(&self, region: Region) -> Result<EcrClient, Error> {
        get_client_sts!(EcrClient, region)
    }
}

#[derive(Default, Clone, Debug)]
pub struct AwsProfileInfo {
    pub region: Option<String>,
    pub role_arn: Option<String>,
    pub source_profile: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
}

impl AwsProfileInfo {
    pub fn add(&mut self, key: &str, value: &str) -> Option<String> {
        match key {
            "region" => self.region.replace(value.to_string()),
            "role_arn" => self.role_arn.replace(value.to_string()),
            "source_profile" => self.source_profile.replace(value.to_string()),
            "aws_access_key_id" => self.aws_access_key_id.replace(value.to_string()),
            "aws_secret_access_key" => self.aws_secret_access_key.replace(value.to_string()),
            _ => None,
        }
    }

    pub fn fill_profile_map() -> Result<HashMap<String, AwsProfileInfo>, Error> {
        let home_dir = var("HOME").map_err(|e| format_err!("No HOME directory {}", e))?;
        let config_file = format!("{}/.aws/config", home_dir);
        let credential_file = format!("{}/.aws/credentials", home_dir);
        let mut profile_map = HashMap::new();
        let mut current_profile: Option<String> = None;
        let mut current_info: Option<AwsProfileInfo> = Some(AwsProfileInfo::default());
        for fname in &[config_file, credential_file] {
            if !Path::new(fname).exists() {
                continue;
            }
            let results: Result<(), Error> = BufReader::new(File::open(fname)?)
                .lines()
                .map(|l| {
                    let line = l?.trim().to_string();
                    if line.starts_with('[') && line.ends_with(']') {
                        let new_name = line
                            .replace("[", "")
                            .replace("]", "")
                            .replace("profile ", "")
                            .trim()
                            .to_string();
                        let new_info = profile_map
                            .remove(&new_name)
                            .unwrap_or_else(|| AwsProfileInfo::default());
                        let old_name = current_profile.replace(new_name);
                        let old_info = current_info.replace(new_info);
                        if let Some(name) = old_name {
                            if let Some(info) = old_info {
                                profile_map.insert(name, info);
                            }
                        }
                    } else {
                        let entries: Vec<_> = line.split('=').map(|x| x.trim()).collect();
                        if entries.len() >= 2 {
                            current_info.as_mut().map(|c| c.add(entries[0], entries[1]));
                        }
                    }
                    Ok(())
                })
                .collect();
            results?;
            if let Some(name) = current_profile.take() {
                if let Some(info) = current_info.take() {
                    profile_map.insert(name, info);
                }
            }
        }
        Ok(profile_map)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::ec2_instance::Ec2Instance;
    use crate::sts_instance::AwsProfileInfo;

    #[test]
    #[ignore]
    fn test_fill_profile_map() {
        let prof_map = AwsProfileInfo::fill_profile_map().unwrap();
        for (k, v) in &prof_map {
            println!("{} {:?}", k, v);
        }
        assert!(prof_map.len() > 0);
        assert!(prof_map.contains_key("default"));
    }

    #[test]
    #[ignore]
    fn test_use_sts_profile() {
        let config = Config::new();
        let ec2 = Ec2Instance::new(config);
        let inst = ec2.get_all_regions().unwrap();
        println!("{}", inst.len());
        assert!(inst.len() >= 16);
    }
}
