use failure::{format_err, Error};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ResourceType {
    Reserved,
    Spot,
    Ami,
    Volume,
    Snapshot,
    Ecr,
    Key,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ResourceType::Reserved => "reserved",
                ResourceType::Spot => "spot",
                ResourceType::Ami => "ami",
                ResourceType::Volume => "volume",
                ResourceType::Snapshot => "snapshot",
                ResourceType::Ecr => "ecr",
                ResourceType::Key => "key",
            }
        )
    }
}

impl FromStr for ResourceType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "reserved" => Ok(ResourceType::Reserved),
            "spot" => Ok(ResourceType::Spot),
            "ami" => Ok(ResourceType::Ami),
            "volume" => Ok(ResourceType::Volume),
            "snapshot" => Ok(ResourceType::Snapshot),
            "ecr" => Ok(ResourceType::Ecr),
            "key" => Ok(ResourceType::Key),
            _ => Err(format_err!("{} is not a ResourceType", s)),
        }
    }
}
