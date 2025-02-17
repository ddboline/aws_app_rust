use aws_config::SdkConfig;
use aws_sdk_route53::{
    types::{Change, ChangeAction, ChangeBatch, HostedZone, ResourceRecordSet, RrType},
    Client as Route53Client,
};
use aws_types::region::Region;
use futures::{stream::FuturesUnordered, TryStreamExt};
use stack_string::format_sstr;
use std::{fmt, net::Ipv4Addr};

use crate::errors::AwslibError as Error;

#[derive(Clone)]
pub struct Route53Instance {
    route53_client: Route53Client,
}

impl fmt::Debug for Route53Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Route53Instance")
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct DnsRecord {
    pub dnsname: String,
    pub ip: String,
}

impl Route53Instance {
    #[must_use]
    pub fn new(config: &SdkConfig) -> Self {
        Self {
            route53_client: Route53Client::from_conf(config.into()),
        }
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn set_region(&mut self, region: impl AsRef<str>) -> Result<(), Error> {
        let region: String = region.as_ref().into();
        let region = Region::new(region);
        let sdk_config = aws_config::from_env().region(region).load().await;
        self.route53_client = Route53Client::from_conf((&sdk_config).into());
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn get_hosted_zones(&self) -> Result<Vec<HostedZone>, Error> {
        self.route53_client
            .list_hosted_zones()
            .send()
            .await
            .map(|r| r.hosted_zones)
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn list_record_sets(
        &self,
        id: impl Into<String>,
    ) -> Result<Vec<ResourceRecordSet>, Error> {
        self.route53_client
            .list_resource_record_sets()
            .hosted_zone_id(id)
            .send()
            .await
            .map_err(Into::into)
            .map(|r| r.resource_record_sets)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn list_dns_records(&self, id: impl Into<String>) -> Result<Vec<DnsRecord>, Error> {
        self.list_record_sets(id).await.map(|result| {
            result
                .into_iter()
                .filter_map(|record| {
                    if record.r#type == RrType::A {
                        let dnsname = record.name.trim_end_matches('.').into();
                        let ip = record.resource_records?.pop()?.value().into();
                        Some(DnsRecord { dnsname, ip })
                    } else {
                        None
                    }
                })
                .collect()
        })
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn list_all_dns_records(&self) -> Result<Vec<(String, DnsRecord)>, Error> {
        let hosted_zones = self.get_hosted_zones().await?;
        let futures: FuturesUnordered<_> = hosted_zones
            .into_iter()
            .map(|zone| async move {
                self.list_dns_records(&zone.id).await.map(|v| {
                    v.into_iter()
                        .map(|record| (zone.id.clone(), record))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let results: Vec<_> = futures.try_collect().await?;
        let mut dns_records: Vec<_> = results.into_iter().flatten().collect();
        dns_records.sort();
        Ok(dns_records)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn update_dns_record(
        &self,
        zone_id: &str,
        name: &str,
        old_ip: Ipv4Addr,
        new_ip: Ipv4Addr,
    ) -> Result<(), Error> {
        if old_ip == new_ip {
            return Ok(());
        }
        let old_ip = old_ip.to_string();
        let new_ip = new_ip.to_string();
        let mut record = self
            .list_record_sets(zone_id)
            .await?
            .into_iter()
            .find(|r| r.r#type == RrType::A && r.name == name)
            .ok_or_else(|| Error::StaticCustomError("No record found"))?;

        let value = record
            .resource_records
            .as_mut()
            .ok_or_else(|| Error::StaticCustomError("No resource records"))?;
        if let Some(r) = value.get_mut(0) {
            if r.value != old_ip {
                return Err(Error::CustomError(format_sstr!(
                    "old_ip {old_ip} does not match current ip {:?}",
                    r.value
                )));
            }
            r.value.clone_from(&new_ip);
        } else {
            return Err(Error::StaticCustomError("No resource records"));
        }

        let change_batch = ChangeBatch::builder()
            .comment(format!("change ip of {name} from {old_ip} to {new_ip}"))
            .changes(
                Change::builder()
                    .action(ChangeAction::Upsert)
                    .resource_record_set(record)
                    .build()?,
            )
            .build()?;
        self.route53_client
            .change_resource_record_sets()
            .hosted_zone_id(zone_id)
            .change_batch(change_batch)
            .send()
            .await?;
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn get_ip_address(&self) -> Result<Ipv4Addr, Error> {
        let ip = reqwest::get("https://ipinfo.io/ip")
            .await?
            .error_for_status()?
            .text()
            .await?
            .parse()?;
        Ok(ip)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        config::Config,
        errors::AwslibError as Error,
        route53_instance::{DnsRecord, Route53Instance},
    };

    #[tokio::test]
    #[ignore]
    async fn test_route53_instance() -> Result<(), Error> {
        let config = aws_config::load_from_env().await;
        let r53 = Route53Instance::new(&config);
        for zone in r53.get_hosted_zones().await? {
            for record_set in r53.list_record_sets(&zone.id).await? {
                if let Some(records) = record_set.resource_records {
                    println!(
                        "{:?} {:?} {}",
                        record_set.name,
                        record_set.r#type,
                        records.len()
                    );
                }
            }
            let result = r53.list_dns_records(&zone.id).await?;
            println!("{:?}", result);
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_list_all_dns_records() -> Result<(), Error> {
        let config = aws_config::load_from_env().await;
        let r53 = Route53Instance::new(&config);
        let result = r53.list_all_dns_records().await?;
        assert!(result.len() > 0);
        println!("{:?}", result);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_ip_address() -> Result<(), Error> {
        let config = aws_config::load_from_env().await;
        let r53 = Route53Instance::new(&config);
        let ip = r53.get_ip_address().await?;
        let name_map: HashMap<_, _> = r53
            .list_all_dns_records()
            .await?
            .into_iter()
            .map(|(_, DnsRecord { dnsname, ip })| (dnsname, ip))
            .collect();
        let config = Config::init_config()?;
        if config.domain == "www.ddboline.net" || config.domain == "cloud.ddboline.net" {
            if let Some(home_ip) = name_map.get(config.domain.as_str()) {
                assert_eq!(&ip.to_string(), home_ip);
            }
        }
        Ok(())
    }
}
