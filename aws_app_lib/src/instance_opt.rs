use clap::Parser;
use stack_string::StackString;

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
    script: Option<StackString>,
    #[clap(short, long, long = "tag")]
    tags: Vec<StackString>,
    #[clap(short, long)]
    key_name: Option<StackString>,
}

impl InstanceOpt {
    #[must_use]
    pub fn into_instance_request(self, config: &Config) -> InstanceRequest {
        let security_group = self.security_group.unwrap_or_else(|| {
            config
                .default_security_group
                .clone()
                .expect("NO DEFAULT_SECURITY_GROUP")
        });
        let key_name = self.key_name.unwrap_or_else(|| {
            config
                .default_key_name
                .clone()
                .expect("NO DEFAULT_KEY_NAME")
        });
        InstanceRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group,
            script: self.script.unwrap_or_else(|| "setup_aws.sh".into()),
            key_name,
            tags: get_tags(&self.tags),
        }
    }
}
