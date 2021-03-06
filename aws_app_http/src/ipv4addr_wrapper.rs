use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{Entity, Schema, Type};
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
    fn describe() -> Schema {
        Schema {
            schema_type: Some(Type::String),
            format: "ipv4_address".into(),
            ..Schema::default()
        }
    }
}
