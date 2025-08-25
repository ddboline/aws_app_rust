#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unused_async)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_for_each)]
#![recursion_limit = "256"]

pub mod app;
pub mod elements;
pub mod errors;
pub mod ipv4addr_wrapper;
pub mod logged_user;
pub mod requests;
pub mod routes;

use derive_more::{From, Into};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use time::OffsetDateTime;
use utoipa::ToSchema;
use utoipa_helper::derive_utoipa_schema;

use aws_app_lib::{
    iam_instance::{IamAccessKey, IamUser},
    resource_type::ResourceType,
};

#[derive(Debug, Serialize, Deserialize, Into, From)]
pub struct IamUserWrapper(IamUser);

derive_utoipa_schema!(IamUserWrapper, _IamUserWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// IamUser
struct _IamUserWrapper {
    // Iam Arn
    #[schema(inline)]
    arn: StackString,
    // Created DateTime
    create_date: OffsetDateTime,
    // User ID
    #[schema(inline)]
    user_id: StackString,
    // User Name
    #[schema(inline)]
    user_name: StackString,
    // Tags
    #[schema(inline)]
    tags: HashMap<String, StackString>,
}

#[derive(Serialize, Deserialize, Into, From)]
pub struct IamAccessKeyWrapper(IamAccessKey);

derive_utoipa_schema!(IamAccessKeyWrapper, _IamAccessKeyWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// IamAccessKey
#[schema(as = IamAccessKey)]
struct _IamAccessKeyWrapper {
    // Access Key ID
    #[schema(inline)]
    access_key_id: StackString,
    // Created DateTime
    #[schema(inline)]
    create_date: OffsetDateTime,
    // Access Secret Key
    #[schema(inline)]
    access_key_secret: StackString,
    // Status
    #[schema(inline)]
    status: StackString,
    // User Name
    #[schema(inline)]
    user_name: StackString,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Into, From)]
pub struct ResourceTypeWrapper(ResourceType);

derive_utoipa_schema!(ResourceTypeWrapper, _ResourceTypeWrapper);

#[allow(dead_code)]
#[derive(ToSchema, Serialize)]
// ResourceType
#[schema(as = ResourceType)]
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
        _IamAccessKeyWrapper, _IamUserWrapper, _ResourceTypeWrapper, IamAccessKeyWrapper,
        IamUserWrapper, ResourceTypeWrapper,
    };
    use utoipa_helper::derive_utoipa_test;

    #[test]
    fn test_types() {
        derive_utoipa_test!(IamUserWrapper, _IamUserWrapper);
        derive_utoipa_test!(IamAccessKeyWrapper, _IamAccessKeyWrapper);
        derive_utoipa_test!(ResourceTypeWrapper, _ResourceTypeWrapper);
    }
}
