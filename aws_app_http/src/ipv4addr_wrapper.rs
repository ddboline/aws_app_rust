use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity, Schema, Type};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

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

impl Entity for Ipv4AddrWrapper {
    #[inline]
    fn type_name() -> std::borrow::Cow<'static, str> {
        "ipv4_address".into()
    }
    #[inline]
    fn describe(_: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        ComponentOrInlineSchema::Inline(Schema {
            schema_type: Some(Type::String),
            format: "ipv4_address".into(),
            ..Schema::default()
        })
    }
}
