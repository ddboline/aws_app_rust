#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unused_async)]
#![allow(clippy::ignored_unit_patterns)]
#![recursion_limit = "256"]

pub mod app;
pub mod elements;
pub mod errors;
pub mod ipv4addr_wrapper;
pub mod logged_user;
pub mod requests;
pub mod routes;

use derive_more::{From, Into};
use rweb::Schema;
use rweb_helper::{derive_rweb_schema, DateTimeType};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;

use aws_app_lib::{
    iam_instance::{IamAccessKey, IamUser},
    resource_type::ResourceType,
};

#[derive(Debug, Serialize, Deserialize, Into, From)]
pub struct IamUserWrapper(IamUser);

derive_rweb_schema!(IamUserWrapper, _IamUserWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "IamUser")]
struct _IamUserWrapper {
    #[schema(description = "Iam Arn")]
    arn: StackString,
    #[schema(description = "Created DateTime")]
    create_date: DateTimeType,
    #[schema(description = "User ID")]
    user_id: StackString,
    #[schema(description = "User Name")]
    user_name: StackString,
    #[schema(description = "Tags")]
    tags: HashMap<String, StackString>,
}

#[derive(Serialize, Deserialize, Into, From)]
pub struct IamAccessKeyWrapper(IamAccessKey);

derive_rweb_schema!(IamAccessKeyWrapper, _IamAccessKeyWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "IamAccessKey")]
struct _IamAccessKeyWrapper {
    #[schema(description = "Access Key ID")]
    access_key_id: StackString,
    #[schema(description = "Created DateTime")]
    create_date: DateTimeType,
    #[schema(description = "Access Secret Key")]
    access_key_secret: StackString,
    #[schema(description = "Status")]
    status: StackString,
    #[schema(description = "User Name")]
    user_name: StackString,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Into, From)]
pub struct ResourceTypeWrapper(ResourceType);

derive_rweb_schema!(ResourceTypeWrapper, _ResourceTypeWrapper);

#[allow(dead_code)]
#[derive(Schema, Serialize)]
#[schema(component = "ResourceType")]
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
    #[serde(rename = "inbound-email")]
    InboundEmail,
}

#[cfg(test)]
mod test {
    use crate::{
        IamAccessKeyWrapper, IamUserWrapper, ResourceTypeWrapper, _IamAccessKeyWrapper,
        _IamUserWrapper, _ResourceTypeWrapper,
    };
    use rweb_helper::derive_rweb_test;

    #[test]
    fn test_types() {
        derive_rweb_test!(IamUserWrapper, _IamUserWrapper);
        derive_rweb_test!(IamAccessKeyWrapper, _IamAccessKeyWrapper);
        derive_rweb_test!(ResourceTypeWrapper, _ResourceTypeWrapper);
    }
}
