use derive_more::{Deref, Display, From, FromStr, Into};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use utoipa::{
    PartialSchema, ToSchema,
    openapi::schema::{ObjectBuilder, Type},
};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    FromStr,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Deref,
    Into,
    From,
    Display,
)]
pub struct Ipv4AddrWrapper(Ipv4Addr);

impl PartialSchema for Ipv4AddrWrapper {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        ObjectBuilder::new()
            .format(Some(utoipa::openapi::SchemaFormat::Custom(
                "ipv4_address".into(),
            )))
            .schema_type(Type::String)
            .build()
            .into()
    }
}

impl ToSchema for Ipv4AddrWrapper {
    fn name() -> std::borrow::Cow<'static, str> {
        "ipv4_address".into()
    }
}
