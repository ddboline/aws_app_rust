use anyhow::Error;
use chrono::{DateTime, Utc};
use postgres_query::{client::GenericClient, query, FromSqlRow};
use stack_string::StackString;
use std::fmt;

use crate::pgpool::{PgPool, PgTransaction};

#[derive(FromSqlRow, Clone, Debug)]
pub struct InstanceFamily {
    pub family_name: StackString,
    pub family_type: StackString,
    pub data_url: Option<StackString>,
}

impl InstanceFamily {
    async fn get_by_family_name<C>(family_name: &str, conn: &C) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                SELECT * FROM instance_family WHERE family_name = $family_name
            "#,
            family_name = family_name,
        );
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!(
            r#"
                SELECT * FROM instance_family
                ORDER BY family_type, family_name
            "#
        );
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO instance_family (family_name, family_type, data_url)
                VALUES ($family_name, $family_type, $data_url)
            "#,
            family_name = self.family_name,
            family_type = self.family_type,
            data_url = self.data_url,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                UPDATE instance_family
                SET family_type=$family_type,data_url=$data_url
                WHERE family_name=$family_name
            "#,
            family_name = self.family_name,
            family_type = self.family_type,
            data_url = self.data_url,
        );
        query.execute(conn).await?;
        Ok(())
    }

    pub async fn upsert_entry(&self, pool: &PgPool) -> Result<Option<Self>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;

        let existing = Self::get_by_family_name(&self.family_name, conn).await?;

        if existing.is_some() {
            self._update_entry(conn).await?;
        } else {
            self._insert_entry(conn).await?;
        }
        tran.commit().await?;
        Ok(existing)
    }
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct InstanceList {
    pub instance_type: StackString,
    pub family_name: StackString,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: StackString,
}

impl InstanceList {
    pub async fn get_all_instances(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM instance_list");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    pub async fn get_by_instance_family(
        instance_family: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let query = query!(
            "SELECT * FROM instance_list WHERE family_name = $family_name",
            family_name = instance_family,
        );
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    async fn _get_by_instance_type<C>(instance_type: &str, conn: &C) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                SELECT * FROM instance_list WHERE instance_type = $instance_type
            "#,
            instance_type = instance_type,
        );
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    pub async fn get_by_instance_type(
        instance_type: &str,
        pool: &PgPool,
    ) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::_get_by_instance_type(instance_type, &conn)
            .await
            .map_err(Into::into)
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO instance_list (
                    instance_type, family_name, n_cpu, memory_gib, generation
                ) VALUES (
                    $instance_type, $family_name, $n_cpu, $memory_gib, $generation
                )
            "#,
            instance_type = self.instance_type,
            family_name = self.family_name,
            n_cpu = self.n_cpu,
            memory_gib = self.memory_gib,
            generation = self.generation,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                UPDATE instance_list
                SET family_name=$family_name, n_cpu=$n_cpu, memory_gib=$memory_gib, generation=$generation
                WHERE instance_type = $instance_type
            "#,
            instance_type = self.instance_type,
            family_name = self.family_name,
            n_cpu = self.n_cpu,
            memory_gib = self.memory_gib,
            generation = self.generation,
        );
        query.execute(conn).await?;
        Ok(())
    }

    pub async fn upsert_entry(&self, pool: &PgPool) -> Result<Option<Self>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;

        let result = Self::_get_by_instance_type(&self.instance_type, conn).await?;

        if result.is_some() {
            self._update_entry(conn).await?;
        } else {
            self._insert_entry(conn).await?;
        }
        tran.commit().await?;
        Ok(result)
    }
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct InstancePricing {
    pub id: i32,
    pub instance_type: StackString,
    pub price: f64,
    pub price_type: StackString,
    pub price_timestamp: DateTime<Utc>,
}

impl InstancePricing {
    pub fn new(
        instance_type: &str,
        price: f64,
        price_type: &str,
        price_timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: -1,
            instance_type: instance_type.into(),
            price,
            price_type: price_type.into(),
            price_timestamp,
        }
    }

    async fn _existing_entries<C>(
        instance_type: &str,
        price_type: &str,
        conn: &C,
    ) -> Result<Vec<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                SELECT * FROM instance_pricing
                WHERE instance_type = $instance_type
                  AND price_type = $price_type
            "#,
            instance_type = instance_type,
            price_type = price_type,
        );
        query.fetch(conn).await.map_err(Into::into)
    }

    pub async fn existing_entries(
        instance_type: &str,
        price_type: &str,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let conn = pool.get().await?;
        Self::_existing_entries(instance_type, price_type, &conn)
            .await
            .map_err(Into::into)
    }

    pub async fn get_all(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM instance_pricing");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO instance_pricing (
                    instance_type, price, price_type, price_timestamp
                ) values (
                    $instance_type, $price, $price_type, $price_timestamp
                )
            "#,
            instance_type = self.instance_type,
            price = self.price,
            price_type = self.price_type,
            price_timestamp = self.price_timestamp,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                UPDATE instance_pricing
                SET price = $price, price_timestamp = $price_timestamp
                WHERE instance_type = $instance_type
                  AND price_type = $price_type
            "#,
            instance_type = self.instance_type,
            price = self.price,
            price_type = self.price_type,
            price_timestamp = self.price_timestamp,
        );
        query.execute(conn).await?;
        Ok(())
    }

    pub async fn upsert_entry(&self, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;

        let existing_entries =
            Self::_existing_entries(&self.instance_type, &self.price_type, conn).await?;

        if existing_entries.is_empty() {
            self._insert_entry(conn).await?;
        } else {
            self._update_entry(conn).await?;
        }
        tran.commit().await?;
        Ok(existing_entries)
    }
}

#[derive(Clone, Copy)]
pub enum AwsGeneration {
    HVM,
    PV,
}

impl AwsGeneration {
    pub fn to_str(self) -> &'static str {
        match self {
            Self::HVM => "hvm",
            Self::PV => "pv",
        }
    }
}

impl fmt::Display for AwsGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl From<AwsGeneration> for StackString {
    fn from(item: AwsGeneration) -> StackString {
        item.to_str().into()
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub enum PricingType {
    Reserved,
    OnDemand,
    Spot,
}

impl PricingType {
    pub fn to_str(self) -> &'static str {
        match self {
            Self::OnDemand => "ondemand",
            Self::Reserved => "reserved",
            Self::Spot => "spot",
        }
    }
}

impl fmt::Display for PricingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl From<PricingType> for StackString {
    fn from(p: PricingType) -> Self {
        StackString::from_display(p).unwrap()
    }
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

impl AuthorizedUsers {
    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM authorized_users");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }
}
