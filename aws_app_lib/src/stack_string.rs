use anyhow::Error;
use derive_more::{Display, From, Into};
use diesel::backend::Backend;
use diesel::deserialize::{FromSql, Result as DeResult};
use diesel::serialize::{Output, Result as SerResult, ToSql};
use diesel::sql_types::Text;
use inlinable_string::InlinableString;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::io::Write;
use std::str::FromStr;

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Into,
    From,
    Display,
    PartialEq,
    Eq,
    Hash,
    FromSqlRow,
    AsExpression,
    Default,
    PartialOrd,
    Ord,
)]
#[sql_type = "Text"]
#[serde(into = "String", from = "&str")]
pub struct StackString(InlinableString);

impl Into<String> for StackString {
    fn into(self) -> String {
        self.0.to_string()
    }
}

impl From<String> for StackString {
    fn from(item: String) -> Self {
        Self(item.into())
    }
}

impl From<&String> for StackString {
    fn from(item: &String) -> Self {
        Self(item.as_str().into())
    }
}

impl From<&str> for StackString {
    fn from(item: &str) -> Self {
        Self(item.into())
    }
}

impl Borrow<str> for StackString {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

impl<DB> ToSql<Text, DB> for StackString
where
    DB: Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> SerResult {
        (self.0.as_ref() as &str).to_sql(out)
    }
}

impl<ST, DB> FromSql<ST, DB> for StackString
where
    DB: Backend,
    *const str: FromSql<ST, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> DeResult<Self> {
        let str_ptr = <*const str as FromSql<ST, DB>>::from_sql(bytes)?;
        let string = unsafe { &*str_ptr };
        Ok(string.into())
    }
}

impl AsRef<str> for StackString {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl FromStr for StackString {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}
