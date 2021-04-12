use anyhow::Error;
use std::fmt;
use rusoto_route53::{
    Route53, Route53Client, ListHostedZonesRequest,
    HostedZone, ListResourceRecordSetsRequest,
    ResourceRecordSet,
};
use sts_profile_auth::get_client_sts;
use rusoto_core::Region;

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
            route53_client: get_client_sts!(Route53Client, Region::UsEast1).expect("StsProfile failed"),
            region: Region::UsEast1,
        }
    }
}

impl Route53Instance {
    pub fn new(config: &Config) -> Self {
        let region: Region = config.aws_region_name.parse().ok().unwrap_or(Region::UsEast1);
        Self {
            route53_client: get_client_sts!(Route53Client, region.clone()).expect("StsProfile failed"),
            region,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.route53_client = get_client_sts!(Route53Client, self.region.clone())?;
        Ok(())
    }

    pub async fn get_hosted_zones(&self) -> Result<Vec<HostedZone>, Error> {
        self.route53_client.list_hosted_zones(ListHostedZonesRequest::default()).await.map_err(Into::into).map(|r| r.hosted_zones)
    }

    pub async fn list_record_sets(&self, id: &str) -> Result<Vec<ResourceRecordSet>, Error> {
        let request = ListResourceRecordSetsRequest {
            hosted_zone_id: id.into(),
            ..ListResourceRecordSetsRequest::default()
        };
        self.route53_client.list_resource_record_sets(request).await.map_err(Into::into).map(|r| r.resource_record_sets)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::config::Config;
    use crate::route53_instance::Route53Instance;

    #[tokio::test]
    async fn test_route53_instance() -> Result<(), Error> {
        let config = Config::init_config()?;
        let r53=  Route53Instance::new(&config);
        for zone in r53.get_hosted_zones().await? {
            for record in r53.list_record_sets(&zone.id).await? {
                println!("{:?}", record);
            }
        }
        assert!(false);
        Ok(())
    }
}