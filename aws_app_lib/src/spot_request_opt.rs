use clap::Parser;
use stack_string::StackString;
use std::collections::HashMap;

use crate::{config::Config, ec2_instance::SpotRequest};

#[derive(Debug, Clone, Parser)]
pub struct SpotRequestOpt {
    #[clap(short, long)]
    ami: StackString,
    #[clap(short, long)]
    instance_type: StackString,
    #[clap(long)]
    security_group: Option<StackString>,
    #[clap(short, long)]
    script: Option<StackString>,
    #[clap(long)]
    price: Option<f32>,
    #[clap(short, long, long = "tag")]
    tags: Vec<StackString>,
    #[clap(short, long)]
    key_name: Option<StackString>,
}

impl SpotRequestOpt {
    #[must_use]
    pub fn into_spot_request(self, config: &Config) -> SpotRequest {
        let security_group = self.security_group.unwrap_or_else(|| {
            config.spot_security_group.as_ref().map_or_else(
                || {
                    config
                        .default_security_group
                        .clone()
                        .expect("DEFAULT_SECURITY_GROUP NOT SET")
                },
                Clone::clone,
            )
        });
        let key_name = self.key_name.unwrap_or_else(|| {
            config
                .default_key_name
                .clone()
                .expect("NO DEFAULT_KEY_NAME")
        });
        SpotRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group,
            script: self.script.unwrap_or_else(|| "setup_aws.sh".into()),
            key_name,
            price: self.price,
            tags: get_tags(&self.tags),
        }
    }
}

pub(crate) fn get_tags(
    tags: impl IntoIterator<Item = impl AsRef<str>>,
) -> HashMap<StackString, StackString> {
    tags.into_iter()
        .map(|tag| {
            let mut key = "Name";
            let mut val = tag.as_ref();

            if let Some(idx) = tag.as_ref().find(':') {
                let (k, v) = tag.as_ref().split_at(idx);
                if val.len() > 1 {
                    key = k;
                    val = &v[1..];
                } else {
                    val = k;
                }
            }

            (key.into(), val.into())
        })
        .collect()
}
