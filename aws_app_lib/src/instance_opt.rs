use anyhow::{format_err, Error};
use clap::Parser;
use stack_string::StackString;
use std::path::PathBuf;

use crate::{config::Config, ec2_instance::InstanceRequest, spot_request_opt::get_tags};

#[derive(Parser, Debug, Clone)]
pub struct InstanceOpt {
    #[clap(short, long)]
    ami: StackString,
    #[clap(short, long)]
    instance_type: StackString,
    #[clap(long)]
    security_group: Option<StackString>,
    #[clap(short, long)]
    script: Option<PathBuf>,
    #[clap(short, long, long = "tag")]
    tags: Vec<StackString>,
    #[clap(short, long)]
    key_name: Option<StackString>,
}

impl InstanceOpt {
    /// # Errors
    /// Returns error if configs are missing
    pub fn into_instance_request(self, config: &Config) -> Result<InstanceRequest, Error> {
        let security_group = self
            .security_group
            .or_else(|| config.default_security_group.clone())
            .ok_or_else(|| format_err!("NO DEFAULT_SECURITY_GROUP"))?;
        let key_name = self
            .key_name
            .or_else(|| config.default_key_name.clone())
            .ok_or_else(|| format_err!("NO DEFAULT_KEY_NAME"))?;
        Ok(InstanceRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group,
            script: self.script.unwrap_or_else(|| "setup_aws.sh".into()),
            key_name,
            tags: get_tags(&self.tags),
        })
    }
}
