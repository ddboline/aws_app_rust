use anyhow::Error;
use chrono::{DateTime, Utc};
use diesel::{
    pg::PgConnection, result::Error as DieselError, ExpressionMethods, Insertable, QueryDsl,
    Queryable, RunQueryDsl, TextExpressionMethods,
};
use stack_string::StackString;
use std::fmt;
use tokio_diesel::{AsyncConnection, AsyncRunQueryDsl};

use crate::{
    pgpool::PgPool,
    schema::{authorized_users, instance_family, instance_list, instance_pricing},
};

#[derive(Queryable, Clone, Debug)]
pub struct InstanceFamily {
    pub id: i32,
    pub family_name: StackString,
    pub family_type: StackString,
    pub data_url: Option<StackString>,
}

#[derive(Insertable, Clone, Debug)]
#[table_name = "instance_family"]
pub struct InstanceFamilyInsert {
    pub family_name: StackString,
    pub family_type: StackString,
    pub data_url: Option<StackString>,
}

impl From<InstanceFamily> for InstanceFamilyInsert {
    fn from(item: InstanceFamily) -> InstanceFamilyInsert {
        InstanceFamilyInsert {
            family_name: item.family_name,
            family_type: item.family_type,
            data_url: item.data_url,
        }
    }
}

impl InstanceFamily {
    fn _existing_entries(
        f_name: &str,
        f_type: &str,
        conn: &PgConnection,
    ) -> Result<Vec<Self>, DieselError> {
        use crate::schema::instance_family::dsl::{family_name, family_type, instance_family};
        instance_family
            .filter(family_name.eq(f_name))
            .filter(family_type.eq(f_type))
            .load(conn)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_family::dsl::{family_name, family_type, instance_family};
        instance_family
            .order((family_type, family_name))
            .load_async(pool)
            .await
            .map_err(Into::into)
    }
}

impl InstanceFamilyInsert {
    fn _insert_entry(&self, conn: &PgConnection) -> Result<(), DieselError> {
        use crate::schema::instance_family::dsl::instance_family;

        diesel::insert_into(instance_family)
            .values(self)
            .execute(conn)
            .map(|_| ())
    }

    pub async fn insert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        pool.transaction(|conn| {
            let existing_entries = InstanceFamily::_existing_entries(
                self.family_name.as_ref(),
                self.family_type.as_ref(),
                conn,
            )?;
            if existing_entries.is_empty() {
                self._insert_entry(conn)?;
                Ok((self, true))
            } else {
                Ok((self, false))
            }
        })
        .await
        .map_err(Into::into)
    }
}

impl fmt::Display for AwsGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HVM => write!(f, "hvm"),
            Self::PV => write!(f, "pv"),
        }
    }
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "instance_list"]
pub struct InstanceList {
    pub instance_type: StackString,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: StackString,
}

impl InstanceList {
    pub async fn get_all_instances(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::instance_list;
        instance_list.load_async(pool).await.map_err(Into::into)
    }

    pub async fn get_by_instance_family(
        instance_family: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::{instance_list, instance_type};
        instance_list
            .filter(instance_type.like(format!("{}%", instance_family)))
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    fn _get_by_instance_type(
        instance_type_: &str,
        conn: &PgConnection,
    ) -> Result<Vec<Self>, DieselError> {
        use crate::schema::instance_list::dsl::{instance_list, instance_type};
        instance_list
            .filter(instance_type.eq(instance_type_))
            .load(conn)
    }

    pub async fn get_by_instance_type(
        instance_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let instance_type = instance_type.to_owned();
        pool.run(move |conn| Self::_get_by_instance_type(&instance_type, conn))
            .await
            .map_err(Into::into)
    }

    fn _insert_entry(&self, conn: &PgConnection) -> Result<(), DieselError> {
        use crate::schema::instance_list::dsl::instance_list;

        diesel::insert_into(instance_list)
            .values(self)
            .execute(conn)
            .map(|_| ())
    }

    pub async fn insert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        pool.transaction(|conn| {
            let existing_entries = Self::_get_by_instance_type(self.instance_type.as_ref(), conn)?;
            if existing_entries.is_empty() {
                self._insert_entry(&conn)?;
                Ok((self, true))
            } else {
                Ok((self, false))
            }
        })
        .await
        .map_err(Into::into)
    }
}

#[derive(Queryable, Clone, Debug)]
pub struct InstancePricing {
    pub id: i32,
    pub instance_type: StackString,
    pub price: f64,
    pub price_type: StackString,
    pub price_timestamp: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[table_name = "instance_pricing"]
pub struct InstancePricingInsert {
    pub instance_type: StackString,
    pub price: f64,
    pub price_type: StackString,
    pub price_timestamp: DateTime<Utc>,
}

impl From<InstancePricing> for InstancePricingInsert {
    fn from(item: InstancePricing) -> InstancePricingInsert {
        InstancePricingInsert {
            instance_type: item.instance_type,
            price: item.price,
            price_type: item.price_type,
            price_timestamp: item.price_timestamp,
        }
    }
}

impl InstancePricing {
    fn _existing_entries(
        i_type: &str,
        p_type: &str,
        conn: &PgConnection,
    ) -> Result<Vec<Self>, DieselError> {
        use crate::schema::instance_pricing::dsl::{instance_pricing, instance_type, price_type};
        instance_pricing
            .filter(instance_type.eq(i_type))
            .filter(price_type.eq(p_type))
            .load(conn)
    }

    pub async fn existing_entries(
        i_type: &str,
        p_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let i_type = i_type.to_owned();
        let p_type = p_type.to_owned();
        pool.run(move |conn| Self::_existing_entries(&i_type, &p_type, conn))
            .await
            .map_err(Into::into)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_pricing::dsl::instance_pricing;
        instance_pricing.load_async(pool).await.map_err(Into::into)
    }
}

impl InstancePricingInsert {
    fn _insert_entry(&self, conn: &PgConnection) -> Result<(), DieselError> {
        use crate::schema::instance_pricing::dsl::instance_pricing;

        diesel::insert_into(instance_pricing)
            .values(self)
            .execute(conn)
            .map(|_| ())
    }

    fn _update_entry(&self, conn: &PgConnection) -> Result<(), DieselError> {
        use crate::schema::instance_pricing::dsl::{
            instance_pricing, instance_type, price, price_timestamp, price_type,
        };
        diesel::update(
            instance_pricing
                .filter(instance_type.eq(&self.instance_type))
                .filter(price_type.eq(&self.price_type)),
        )
        .set((
            price.eq(self.price),
            price_timestamp.eq(self.price_timestamp),
        ))
        .execute(conn)
        .map(|_| ())
    }

    pub async fn upsert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        pool.transaction(|conn| {
            let existing_entries = InstancePricing::_existing_entries(
                self.instance_type.as_ref(),
                self.price_type.as_ref(),
                &conn,
            )?;
            if existing_entries.is_empty() {
                self._insert_entry(&conn)?;
                Ok((self, true))
            } else {
                self._update_entry(&conn)?;
                Ok((self, false))
            }
        })
        .await
        .map_err(Into::into)
    }
}

#[derive(Clone, Copy)]
pub enum AwsGeneration {
    HVM,
    PV,
}

impl From<AwsGeneration> for StackString {
    fn from(item: AwsGeneration) -> StackString {
        match item {
            AwsGeneration::HVM => "hvm".into(),
            AwsGeneration::PV => "pv".into(),
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub enum PricingType {
    Reserved,
    OnDemand,
    Spot,
}

impl fmt::Display for PricingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OnDemand => write!(f, "ondemand"),
            Self::Reserved => write!(f, "reserved"),
            Self::Spot => write!(f, "spot"),
        }
    }
}

impl From<PricingType> for StackString {
    fn from(p: PricingType) -> Self {
        p.to_string().into()
    }
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

impl AuthorizedUsers {
    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::authorized_users::dsl::authorized_users;
        authorized_users.load_async(pool).await.map_err(Into::into)
    }
}
