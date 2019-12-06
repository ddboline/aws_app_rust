use chrono::{DateTime, Utc};
use diesel::{Connection, ExpressionMethods, Insertable, QueryDsl, Queryable, RunQueryDsl};
use failure::{err_msg, Error};
use std::borrow::Cow;
use std::fmt;

use crate::pgpool::{PgPool, PgPoolConn};
use crate::schema::{instance_family, instance_list, instance_pricing};

#[derive(Queryable, Clone, Debug)]
pub struct InstanceFamily<'a> {
    pub id: i32,
    pub family_name: Cow<'a, str>,
    pub family_type: Cow<'a, str>,
}

#[derive(Insertable, Clone, Debug)]
#[table_name = "instance_family"]
pub struct InstanceFamilyInsert<'a> {
    pub family_name: Cow<'a, str>,
    pub family_type: Cow<'a, str>,
}

impl<'a> From<InstanceFamily<'a>> for InstanceFamilyInsert<'a> {
    fn from(item: InstanceFamily) -> InstanceFamilyInsert {
        InstanceFamilyInsert {
            family_name: item.family_name,
            family_type: item.family_type,
        }
    }
}

impl InstanceFamily<'_> {
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
            .map_err(err_msg)
    }

    pub fn existing_entries(f_name: &str, f_type: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_existing_entries(f_name, f_type, &conn)
    }
}

impl InstanceFamilyInsert<'_> {
    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_family::dsl::instance_family;

        diesel::insert_into(instance_family)
            .values(self)
            .execute(conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn insert_entry(&self, pool: &PgPool) -> Result<bool, Error> {
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
}

impl fmt::Display for AwsGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AwsGeneration::HVM => write!(f, "hvm"),
            AwsGeneration::PV => write!(f, "pv"),
        }
    }
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "instance_list"]
pub struct InstanceList<'a> {
    pub instance_type: Cow<'a, str>,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: Cow<'a, str>,
}

impl InstanceList<'_> {
    pub fn get_all_instances(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::instance_list;
        let conn = pool.get()?;
        instance_list.load(&conn).map_err(err_msg)
    }

    fn _get_by_instance_type(instance_type_: &str, conn: &PgPoolConn) -> Result<Vec<Self>, Error> {
        use crate::schema::instance_list::dsl::{instance_list, instance_type};
        instance_list
            .filter(instance_type.eq(instance_type_))
            .load(conn)
            .map_err(err_msg)
    }

    pub fn get_by_instance_type(instance_type_: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_get_by_instance_type(instance_type_, &conn)
    }

    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_list::dsl::instance_list;

        diesel::insert_into(instance_list)
            .values(self)
            .execute(conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn insert_entry(&self, pool: &PgPool) -> Result<bool, Error> {
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
}

#[derive(Queryable, Clone, Debug)]
pub struct InstancePricing<'a> {
    pub id: i32,
    pub instance_type: Cow<'a, str>,
    pub price: f64,
    pub price_type: Cow<'a, str>,
    pub price_timestamp: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[table_name = "instance_pricing"]
pub struct InstancePricingInsert<'a> {
    pub instance_type: Cow<'a, str>,
    pub price: f64,
    pub price_type: Cow<'a, str>,
    pub price_timestamp: DateTime<Utc>,
}

impl<'a> From<InstancePricing<'a>> for InstancePricingInsert<'a> {
    fn from(item: InstancePricing) -> InstancePricingInsert {
        InstancePricingInsert {
            instance_type: item.instance_type,
            price: item.price,
            price_type: item.price_type,
            price_timestamp: item.price_timestamp,
        }
    }
}

impl InstancePricing<'_> {
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
            .map_err(err_msg)
    }

    pub fn existing_entries(i_type: &str, p_type: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let conn = pool.get()?;
        Self::_existing_entries(i_type, p_type, &conn)
    }
}

impl InstancePricingInsert<'_> {
    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::instance_pricing::dsl::instance_pricing;

        diesel::insert_into(instance_pricing)
            .values(self)
            .execute(conn)
            .map_err(err_msg)
            .map(|_| ())
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
        .map_err(err_msg)
        .map(|_| ())
    }

    pub fn upsert_entry(&self, pool: &PgPool) -> Result<bool, Error> {
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