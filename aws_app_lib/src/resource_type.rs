use serde::{Deserialize, Serialize};
use stack_string::{StackString, format_sstr};
use std::{convert::TryFrom, fmt, str::FromStr};

use crate::errors::AwslibError as Error;

pub static ALL_RESOURCES: [ResourceType; 15] = [
    ResourceType::Instances,
    ResourceType::Reserved,
    ResourceType::Spot,
    ResourceType::Ami,
    ResourceType::Volume,
    ResourceType::Snapshot,
    ResourceType::Ecr,
    ResourceType::Key,
    ResourceType::Script,
    ResourceType::User,
    ResourceType::Group,
    ResourceType::AccessKey,
    ResourceType::Route53,
    ResourceType::SystemD,
    ResourceType::InboundEmail,
];

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ResourceType {
    #[serde(rename = "instances")]
    Instances,
    #[serde(rename = "reserved")]
    Reserved,
    #[serde(rename = "spot")]
    Spot,
    #[serde(rename = "ami")]
    Ami,
    #[serde(rename = "volume")]
    Volume,
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "ecr")]
    Ecr,
    #[serde(rename = "key")]
    Key,
    #[serde(rename = "script")]
    Script,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "group")]
    Group,
    #[serde(rename = "access-key")]
    AccessKey,
    #[serde(rename = "route53")]
    Route53,
    #[serde(rename = "systemd")]
    SystemD,
    #[serde(rename = "inbound-email")]
    InboundEmail,
    #[serde(rename = "security-group")]
    SecurityGroup,
    #[serde(rename = "all")]
    All,
}

impl ResourceType {
    #[must_use]
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Instances => "instances",
            Self::Reserved => "reserved",
            Self::Spot => "spot",
            Self::Ami => "ami",
            Self::Volume => "volume",
            Self::Snapshot => "snapshot",
            Self::Ecr => "ecr",
            Self::Key => "key",
            Self::Script => "script",
            Self::User => "user",
            Self::Group => "group",
            Self::AccessKey => "access-key",
            Self::Route53 => "route53",
            Self::SystemD => "systemd",
            Self::InboundEmail => "inbound-email",
            Self::SecurityGroup => "security-group",
            Self::All => "all",
        }
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl FromStr for ResourceType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "instances" => Ok(Self::Instances),
            "reserved" => Ok(Self::Reserved),
            "spot" => Ok(Self::Spot),
            "ami" => Ok(Self::Ami),
            "volume" => Ok(Self::Volume),
            "snapshot" => Ok(Self::Snapshot),
            "ecr" => Ok(Self::Ecr),
            "key" => Ok(Self::Key),
            "script" => Ok(Self::Script),
            "user" => Ok(Self::User),
            "group" => Ok(Self::Group),
            "access-key" | "access_key" => Ok(Self::AccessKey),
            "route53" | "dns" => Ok(Self::Route53),
            "systemd" => Ok(Self::SystemD),
            "inbound-email" => Ok(Self::InboundEmail),
            "security-group" => Ok(Self::SecurityGroup),
            "all" => Ok(Self::All),
            _ => Err(Error::CustomError(format_sstr!(
                "{} is not a ResourceType",
                s
            ))),
        }
    }
}

impl From<ResourceType> for String {
    fn from(item: ResourceType) -> Self {
        item.to_string()
    }
}

impl From<ResourceType> for StackString {
    fn from(item: ResourceType) -> Self {
        item.to_str().into()
    }
}

impl TryFrom<&str> for ResourceType {
    type Error = Error;
    fn try_from(item: &str) -> Result<Self, Self::Error> {
        item.parse()
    }
}

impl TryFrom<String> for ResourceType {
    type Error = Error;
    fn try_from(item: String) -> Result<Self, Self::Error> {
        item.parse()
    }
}
