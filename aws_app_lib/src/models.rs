use actix_threadpool::run as block;
use anyhow::{format_err, Error};
use chrono::{DateTime, Utc};
use diesel::{
    Connection, ExpressionMethods, Insertable, QueryDsl, Queryable, RunQueryDsl,
    TextExpressionMethods,
};
use std::fmt;

use crate::pgpool::{PgPool, PgPoolConn};
use crate::schema::{authorized_users, instance_family, instance_list, instance_pricing};

#[derive(Queryable, Clone, Debug)]
pub struct InstanceFamily {
    pub id: i32,
    pub family_name: String,
    pub family_type: String,
}

#[derive(Insertable, Clone, Debug)]
#[table_name = "instance_family"]
pub struct InstanceFamilyInsert {
    pub family_name: String,
    pub family_type: String,
}

impl From<InstanceFamily> for InstanceFamilyInsert {
    fn from(item: InstanceFamily) -> InstanceFamilyInsert {
        InstanceFamilyInsert {
            family_name: item.family_name,
            family_type: item.family_type,
        }
    }
}

impl InstanceFamily {
    fn _existing_entries(
        f_name: &str,
        f_type: &str,
        conn: &PgPoolConn,
    ) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_family::dsl::{family_name, family_type, instance_family};
        instance_family
            .filter(family_name.eq(f_name))
            .filter(family_type.eq(f_type))
            .load(conn)
            .map_err(Into::into)
    }

    pub fn existing_entries_sync(
        f_name: &str,
        f_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_existing_entries(f_name, f_type, &conn)
    }

    pub async fn existing_entries(
        f_name: &str,
        f_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let f_name = f_name.to_owned();
        let f_type = f_type.to_owned();
        let pool = pool.clone();
        block(move || Self::existing_entries_sync(&f_name, &f_type, &pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }

    pub fn get_all_sync(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_family::dsl::instance_family;
        let conn = pool.get()?;
        instance_family.load(&conn).map_err(Into::into)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let pool = pool.clone();
        block(move || Self::get_all_sync(&pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }
}

impl InstanceFamilyInsert {
    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_family::dsl::instance_family;

        diesel::insert_into(instance_family)
            .values(self)
            .execute(conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn insert_entry_sync(&self, pool: &PgPool) -> Result<bool, Error> {
        let conn = pool.get()?;

        conn.transaction(|| {
            let existing_entries =
                InstanceFamily::_existing_entries(&self.family_name, &self.family_type, &conn)?;
            if existing_entries.is_empty() {
                self._insert_entry(&conn)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    pub async fn insert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        let pool = pool.clone();
        block(move || self.insert_entry_sync(&pool).map(|r| (self, r)))
            .await
            .map_err(|e| format_err!("{:?}", e))
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
    pub instance_type: String,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: String,
}

impl InstanceList {
    pub fn get_all_instances_sync(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::instance_list;
        let conn = pool.get()?;
        instance_list.load(&conn).map_err(Into::into)
    }

    pub async fn get_all_instances(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let pool = pool.clone();
        block(move || Self::get_all_instances_sync(&pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }

    pub fn get_by_instance_family_sync(
        instance_family: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::{instance_list, instance_type};
        let conn = pool.get()?;
        instance_list
            .filter(instance_type.like(format!("{}%", instance_family)))
            .load(&conn)
            .map_err(Into::into)
    }

    pub async fn get_by_instance_family(
        instance_family: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let instance_family = instance_family.to_owned();
        let pool = pool.clone();
        block(move || Self::get_by_instance_family_sync(&instance_family, &pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }

    fn _get_by_instance_type(instance_type_: &str, conn: &PgPoolConn) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::{instance_list, instance_type};
        instance_list
            .filter(instance_type.eq(instance_type_))
            .load(conn)
            .map_err(Into::into)
    }

    pub fn get_by_instance_type_sync(
        instance_type_: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_get_by_instance_type(instance_type_, &conn)
    }

    pub async fn get_by_instance_type(
        instance_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let instance_type = instance_type.to_owned();
        let pool = pool.clone();
        block(move || Self::get_by_instance_type_sync(&instance_type, &pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }

    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_list::dsl::instance_list;

        diesel::insert_into(instance_list)
            .values(self)
            .execute(conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn insert_entry_sync(&self, pool: &PgPool) -> Result<bool, Error> {
        let conn = pool.get()?;

        conn.transaction(|| {
            let existing_entries = Self::_get_by_instance_type(&self.instance_type, &conn)?;
            if existing_entries.is_empty() {
                self._insert_entry(&conn)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    pub async fn insert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        let pool = pool.clone();
        block(move || self.insert_entry_sync(&pool).map(|r| (self, r)))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }
}

#[derive(Queryable, Clone, Debug)]
pub struct InstancePricing {
    pub id: i32,
    pub instance_type: String,
    pub price: f64,
    pub price_type: String,
    pub price_timestamp: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[table_name = "instance_pricing"]
pub struct InstancePricingInsert {
    pub instance_type: String,
    pub price: f64,
    pub price_type: String,
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
        conn: &PgPoolConn,
    ) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_pricing::dsl::{instance_pricing, instance_type, price_type};
        instance_pricing
            .filter(instance_type.eq(i_type))
            .filter(price_type.eq(p_type))
            .load(conn)
            .map_err(Into::into)
    }

    pub fn existing_entries_sync(
        i_type: &str,
        p_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_existing_entries(i_type, p_type, &conn)
    }

    pub async fn existing_entries(
        i_type: &str,
        p_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let i_type = i_type.to_owned();
        let p_type = p_type.to_owned();
        let pool = pool.clone();
        block(move || Self::existing_entries_sync(&i_type, &p_type, &pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }

    pub fn get_all_sync(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_pricing::dsl::instance_pricing;
        let conn = pool.get()?;
        instance_pricing.load(&conn).map_err(Into::into)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let pool = pool.clone();
        block(move || Self::get_all_sync(&pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }
}

impl InstancePricingInsert {
    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_pricing::dsl::instance_pricing;

        diesel::insert_into(instance_pricing)
            .values(self)
            .execute(conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn _update_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
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
        .map_err(Into::into)
    }

    pub fn upsert_entry_sync(&self, pool: &PgPool) -> Result<bool, Error> {
        let conn = pool.get()?;

        conn.transaction(|| {
            let existing_entries =
                InstancePricing::_existing_entries(&self.instance_type, &self.price_type, &conn)?;
            if existing_entries.is_empty() {
                self._insert_entry(&conn)?;
                Ok(true)
            } else {
                self._update_entry(&conn)?;
                Ok(false)
            }
        })
    }

    pub async fn upsert_entry(self, pool: &PgPool) -> Result<(Self, bool), Error> {
        let pool = pool.clone();
        block(move || self.upsert_entry_sync(&pool).map(|r| (self, r)))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }
}

#[derive(Clone, Copy)]
pub enum AwsGeneration {
    HVM,
    PV,
}

#[derive(Clone, Copy)]
pub enum PricingType {
    Reserved,
    OnDemand,
    Spot,
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: String,
    pub telegram_userid: Option<i64>,
}

impl AuthorizedUsers {
    pub fn get_authorized_users_sync(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::authorized_users::dsl::authorized_users;
        let conn = pool.get()?;
        authorized_users.load(&conn).map_err(Into::into)
    }

    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let pool = pool.clone();
        block(move || Self::get_authorized_users_sync(&pool))
            .await
            .map_err(|e| format_err!("{:?}", e))
    }
}
