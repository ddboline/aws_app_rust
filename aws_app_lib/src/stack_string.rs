use anyhow::Error;
use derive_more::{From, Into};
use diesel::{
    backend::Backend,
    deserialize::{FromSql, Result as DeResult},
    serialize::{Output, Result as SerResult, ToSql},
    sql_types::Text,
};
use serde::{Deserialize, Serialize};
use smartstring::alias::String as SmartString;
use std::fmt;
use std::string::FromUtf8Error;
use std::{
    borrow::{Borrow, Cow},
    cmp::PartialEq,
    io::Write,
    ops::{Deref, DerefMut},
    path::Path,
    str::FromStr,
};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Into,
    From,
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
#[serde(into = "String", from = "String")]
pub struct StackString(SmartString);

impl StackString {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn from_utf8(vec: Vec<u8>) -> Result<Self, FromUtf8Error> {
        String::from_utf8(vec).map(Into::into)
    }
}

impl fmt::Display for StackString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<StackString> for String {
    fn from(item: StackString) -> Self {
        item.0.into()
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

impl Deref for StackString {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.0.as_ref()
    }
}

impl DerefMut for StackString {
    fn deref_mut(&mut self) -> &mut str {
        self.0.as_mut()
    }
}

impl<DB> ToSql<Text, DB> for StackString
where
    DB: Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> SerResult {
        self.as_str().to_sql(out)
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
        self.as_str()
    }
}

impl AsRef<Path> for StackString {
    fn as_ref(&self) -> &Path {
        self.as_str().as_ref()
    }
}

impl FromStr for StackString {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}

impl<'a> PartialEq<Cow<'a, str>> for StackString {
    #[inline]
    fn eq(&self, other: &Cow<'a, str>) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
}

impl<'a> PartialEq<String> for StackString {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
}

impl<'a> PartialEq<str> for StackString {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
}

impl<'a> PartialEq<&'a str> for StackString {
    #[inline]
    fn eq(&self, other: &&'a str) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
}
