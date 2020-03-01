use anyhow::{format_err, Error};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ResourceType {
    Instances,
    Reserved,
    Spot,
    Ami,
    Volume,
    Snapshot,
    Ecr,
    Key,
    Script,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
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
            }
        )
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
            _ => Err(format_err!("{} is not a ResourceType", s)),
        }
    }
}
