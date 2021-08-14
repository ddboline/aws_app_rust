#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::default_trait_access)]

pub mod app;
pub mod errors;
pub mod ipv4addr_wrapper;
pub mod logged_user;
pub mod requests;
pub mod routes;

use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

use aws_app_lib::{
    iam_instance::{IamAccessKey, IamUser},
    resource_type::ResourceType,
};

#[derive(Debug, Serialize, Deserialize, Schema)]
pub struct IamUserWrapper {
    #[schema(description="Iam Arn")]
    pub arn: StackString,
    #[schema(description="Created DateTime")]
    pub create_date: DateTime<Utc>,
    #[schema(description="User ID")]
    pub user_id: StackString,
    #[schema(description="User Name")]
    pub user_name: StackString,
    #[schema(description="Tags")]
    pub tags: HashMap<String, StackString>,
}

impl From<IamUser> for IamUserWrapper {
    fn from(item: IamUser) -> Self {
        Self {
            arn: item.arn,
            create_date: item.create_date.into(),
            user_id: item.user_id,
            user_name: item.user_name,
            tags: item.tags,
        }
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct IamAccessKeyWrapper {
    #[schema(description="Access Key ID")]
    pub access_key_id: StackString,
    #[schema(description="Created DateTime")]
    pub create_date: DateTime<Utc>,
    #[schema(description="Access Secret Key")]
    pub access_key_secret: StackString,
    #[schema(description="Status")]
    pub status: StackString,
    #[schema(description="User Name")]
    pub user_name: StackString,
}

impl From<IamAccessKey> for IamAccessKeyWrapper {
    fn from(item: IamAccessKey) -> Self {
        Self {
            access_key_id: item.access_key_id,
            create_date: item.create_date.into(),
            access_key_secret: item.access_key_secret,
            status: item.status,
            user_name: item.user_name,
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Schema)]
pub enum ResourceTypeWrapper {
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
}

impl From<ResourceType> for ResourceTypeWrapper {
    fn from(item: ResourceType) -> Self {
        match item {
            ResourceType::Instances => Self::Instances,
            ResourceType::Reserved => Self::Reserved,
            ResourceType::Spot => Self::Spot,
            ResourceType::Ami => Self::Ami,
            ResourceType::Volume => Self::Volume,
            ResourceType::Snapshot => Self::Snapshot,
            ResourceType::Ecr => Self::Ecr,
            ResourceType::Key => Self::Key,
            ResourceType::Script => Self::Script,
            ResourceType::User => Self::User,
            ResourceType::Group => Self::Group,
            ResourceType::AccessKey => Self::AccessKey,
            ResourceType::Route53 => Self::Route53,
            ResourceType::SystemD => Self::SystemD,
        }
    }
}

impl From<ResourceTypeWrapper> for ResourceType {
    fn from(item: ResourceTypeWrapper) -> Self {
        match item {
            ResourceTypeWrapper::Instances => Self::Instances,
            ResourceTypeWrapper::Reserved => Self::Reserved,
            ResourceTypeWrapper::Spot => Self::Spot,
            ResourceTypeWrapper::Ami => Self::Ami,
            ResourceTypeWrapper::Volume => Self::Volume,
            ResourceTypeWrapper::Snapshot => Self::Snapshot,
            ResourceTypeWrapper::Ecr => Self::Ecr,
            ResourceTypeWrapper::Key => Self::Key,
            ResourceTypeWrapper::Script => Self::Script,
            ResourceTypeWrapper::User => Self::User,
            ResourceTypeWrapper::Group => Self::Group,
            ResourceTypeWrapper::AccessKey => Self::AccessKey,
            ResourceTypeWrapper::Route53 => Self::Route53,
            ResourceTypeWrapper::SystemD => Self::SystemD,
        }
    }
}
