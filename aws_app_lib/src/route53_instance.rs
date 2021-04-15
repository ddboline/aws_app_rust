use anyhow::{format_err, Error};
use rusoto_core::Region;
use rusoto_route53::{
    Change, ChangeBatch, ChangeResourceRecordSetsRequest, HostedZone, ListHostedZonesRequest,
    ListResourceRecordSetsRequest, ResourceRecordSet, Route53, Route53Client,
};
use std::{fmt, net::Ipv4Addr};
use sts_profile_auth::get_client_sts;

use crate::config::Config;

pub struct Route53Instance {
    route53_client: Route53Client,
    region: Region,
}

impl fmt::Debug for Route53Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Route53Instance")
    }
}

impl Default for Route53Instance {
    fn default() -> Self {
        Self {
            route53_client: get_client_sts!(Route53Client, Region::UsEast1)
                .expect("StsProfile failed"),
            region: Region::UsEast1,
        }
    }
}

impl Route53Instance {
    pub fn new(config: &Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            route53_client: get_client_sts!(Route53Client, region.clone())
                .expect("StsProfile failed"),
            region,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.route53_client = get_client_sts!(Route53Client, self.region.clone())?;
        Ok(())
    }

    pub async fn get_hosted_zones(&self) -> Result<Vec<HostedZone>, Error> {
        self.route53_client
            .list_hosted_zones(ListHostedZonesRequest::default())
            .await
            .map_err(Into::into)
            .map(|r| r.hosted_zones)
    }

    pub async fn list_record_sets(&self, id: &str) -> Result<Vec<ResourceRecordSet>, Error> {
        let request = ListResourceRecordSetsRequest {
            hosted_zone_id: id.into(),
            ..ListResourceRecordSetsRequest::default()
        };
        self.route53_client
            .list_resource_record_sets(request)
            .await
            .map_err(Into::into)
            .map(|r| r.resource_record_sets)
    }

    pub async fn list_dns_records(&self, id: &str) -> Result<Vec<(String, String)>, Error> {
        self.list_record_sets(id).await.map(|result| {
            result
                .into_iter()
                .filter_map(|record| {
                    if record.type_ == "A" {
                        let value = record.resource_records?.pop()?.value;
                        Some((record.name, value))
                    } else {
                        None
                    }
                })
                .collect()
        })
    }

    pub async fn update_dns_record(
        &self,
        zone_id: &str,
        name: &str,
        old_ip: Ipv4Addr,
        new_ip: Ipv4Addr,
    ) -> Result<(), Error> {
        let old_ip = old_ip.to_string();
        let new_ip = new_ip.to_string();
        let mut record = self
            .list_record_sets(zone_id)
            .await?
            .into_iter()
            .filter(|r| r.type_ == "A" && r.name == name)
            .next()
            .ok_or_else(|| format_err!("No record found"))?;

        let value = record
            .resource_records
            .as_mut()
            .ok_or_else(|| format_err!("No resource records"))?;
        if let Some(r) = value.get_mut(0) {
            if r.value != old_ip {
                return Err(format_err!(
                    "old_ip {} does not match current ip {}",
                    old_ip,
                    r.value
                ));
            }
            r.value = new_ip.clone();
        } else {
            return Err(format_err!("No resource records"));
        }

        let request = ChangeResourceRecordSetsRequest {
            hosted_zone_id: zone_id.into(),
            change_batch: ChangeBatch {
                comment: Some(format!(
                    "change ip of {} from {} to {}",
                    name, old_ip, new_ip
                )),
                changes: vec![Change {
                    action: "UPSERT".into(),
                    resource_record_set: record,
                }],
            },
        };
        self.route53_client
            .change_resource_record_sets(request)
            .await?;
        Ok(())
    }

    pub async fn get_ip_address() -> Result<Ipv4Addr, Error> {
        let response = reqwest::get("https://ipinfo.io/ip").await?.error_for_status()?.text().await?;
        let ip: Ipv4Addr = response.parse()?;
        println!("{}", ip);
        Ok(ip)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use std::net::Ipv4Addr;

    use crate::{config::Config, route53_instance::Route53Instance};

    #[tokio::test]
    async fn test_route53_instance() -> Result<(), Error> {
        let config = Config::init_config()?;
        let r53 = Route53Instance::new(&config);
        for zone in r53.get_hosted_zones().await? {
            for record_set in r53.list_record_sets(&zone.id).await? {
                if let Some(records) = record_set.resource_records {
                    println!("{} {} {}", record_set.name, record_set.type_, records.len());
                }
            }
            let result = r53.list_dns_records(&zone.id).await?;
            println!("{:?}", result);
        }
        assert!(false);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_ip_address() -> Result<(), Error> {
        let ip = Route53Instance::get_ip_address().await?;
        assert_eq!(ip, Ipv4Addr::new(68, 174, 151, 250));
        Ok(())
    }
}
