use chrono::{DateTime, Utc};
use diesel::{Insertable, Queryable};
use std::borrow::Cow;

use crate::schema::{instance_family, instance_list, instance_pricing};

#[derive(Queryable)]
pub struct InstanceFamily<'a> {
    pub id: i32,
    pub family_name: Cow<'a, str>,
    pub family_type: Cow<'a, str>,
}

#[derive(Insertable)]
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

#[derive(Clone, Copy)]
pub enum AwsGeneration {
    HVM,
    PV,
}

#[derive(Queryable, Insertable)]
#[table_name = "instance_list"]
pub struct InstanceList<'a> {
    pub instance_type: Cow<'a, str>,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: Cow<'a, str>,
}

#[derive(Queryable)]
pub struct InstancePricing<'a> {
    pub id: i32,
    pub instance_type: Cow<'a, str>,
    pub price: f64,
    pub price_type: Cow<'a, str>,
    pub price_timestamp: DateTime<Utc>,
}

#[derive(Insertable)]
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
