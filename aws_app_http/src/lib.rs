#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::unused_async)]

pub mod app;
pub mod errors;
pub mod ipv4addr_wrapper;
pub mod logged_user;
pub mod requests;
pub mod routes;

use chrono::{DateTime, Utc};
use derive_more::{From, Into};
use rweb::{
    openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity},
    Schema,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{borrow::Cow, collections::HashMap};

use aws_app_lib::{
    iam_instance::{IamAccessKey, IamUser},
    resource_type::ResourceType,
};

#[derive(Debug, Serialize, Deserialize, Into, From)]
pub struct IamUserWrapper(IamUser);

impl Entity for IamUserWrapper {
    fn type_name() -> Cow<'static, str> {
        _IamUserWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _IamUserWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _IamUserWrapper {
    #[schema(description = "Iam Arn")]
    arn: StackString,
    #[schema(description = "Created DateTime")]
    create_date: DateTime<Utc>,
    #[schema(description = "User ID")]
    user_id: StackString,
    #[schema(description = "User Name")]
    user_name: StackString,
    #[schema(description = "Tags")]
    tags: HashMap<String, StackString>,
}

#[derive(Serialize, Deserialize, Into, From)]
pub struct IamAccessKeyWrapper(IamAccessKey);

impl Entity for IamAccessKeyWrapper {
    fn type_name() -> Cow<'static, str> {
        _IamAccessKeyWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _IamAccessKeyWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _IamAccessKeyWrapper {
    #[schema(description = "Access Key ID")]
    access_key_id: StackString,
    #[schema(description = "Created DateTime")]
    create_date: DateTime<Utc>,
    #[schema(description = "Access Secret Key")]
    access_key_secret: StackString,
    #[schema(description = "Status")]
    status: StackString,
    #[schema(description = "User Name")]
    user_name: StackString,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Into, From)]
pub struct ResourceTypeWrapper(ResourceType);

impl Entity for ResourceTypeWrapper {
    fn type_name() -> Cow<'static, str> {
        _ResourceTypeWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _ResourceTypeWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema, Serialize)]
enum _ResourceTypeWrapper {
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
