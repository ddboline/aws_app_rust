use anyhow::Error;
use rusoto_iam::{IamClient, Iam as _};
use rusoto_core::Region;
use sts_profile_auth::get_client_sts;

use crate::config::Config;

pub struct IamInstance {
    iam_client: IamClient,
    region: Region,
}

impl Default for IamInstance {
    fn default() -> Self {
        let config = Config::new();
        Self {
            iam_client: get_client_sts!(IamClient, Region::UsEast1).expect("StsProfile failed"),
            region: Region::UsEast1,
        }
    }
}

impl IamInstance {
    pub fn new(config: &Config) -> Self {
        let config = config.clone();
        Self {

        }
    }
}