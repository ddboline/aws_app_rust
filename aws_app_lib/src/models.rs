use anyhow::Error;
use futures::Stream;
use postgres_query::{client::GenericClient, query, query_dyn, Error as PqError, FromSqlRow};
use stack_string::{format_sstr, StackString};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::pgpool::{PgPool, PgTransaction};

#[derive(FromSqlRow, Clone, Debug, PartialEq, Eq)]
pub struct InstanceFamily {
    pub family_name: StackString,
    pub family_type: StackString,
    pub data_url: Option<StackString>,
    pub use_for_spot: bool,
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

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_all(
        pool: &PgPool,
        for_spot_instance: Option<bool>,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let constraint = if let Some(for_spot_instance) = for_spot_instance {
            if for_spot_instance {
                "WHERE use_for_spot IS true"
            } else {
                "WHERE use_for_spot IS false"
            }
        } else {
            ""
        };
        let query = format_sstr!(
            r"
                SELECT * FROM instance_family {constraint}
                ORDER BY family_type, family_name
            "
        );
        let query = query_dyn!(&query)?;
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO instance_family (
                    family_name, family_type, data_url
                ) VALUES (
                    $family_name, $family_type, $data_url
                )
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
                SET family_type=$family_type,
                    data_url=$data_url
                WHERE family_name=$family_name
            "#,
            family_name = self.family_name,
            family_type = self.family_type,
            data_url = self.data_url,
        );
        query.execute(conn).await?;
        Ok(())
    }

    /// # Errors
    /// Returns error if db query fails
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

#[derive(FromSqlRow, Clone, Debug, PartialEq)]
pub struct InstanceList {
    pub instance_type: StackString,
    pub family_name: StackString,
    pub n_cpu: i32,
    pub memory_gib: f64,
    pub generation: StackString,
}

impl InstanceList {
    /// # Errors
    /// Returns error if db query fails
    pub async fn get_all_instances(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let query = query!("SELECT * FROM instance_list");
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_by_instance_family(
        instance_family: &str,
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let query = query!(
            "SELECT * FROM instance_list WHERE family_name = $family_name",
            family_name = instance_family,
        );
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
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

    /// # Errors
    /// Returns error if db query fails
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

    /// # Errors
    /// Returns error if db query fails
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
    pub id: Uuid,
    pub instance_type: StackString,
    pub price: f64,
    pub price_type: StackString,
    pub price_timestamp: OffsetDateTime,
}

impl InstancePricing {
    #[must_use]
    pub fn new(
        instance_type: &str,
        price: f64,
        price_type: &str,
        price_timestamp: OffsetDateTime,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instance_type: instance_type.into(),
            price,
            price_type: price_type.into(),
            price_timestamp,
        }
    }

    async fn _existing_entry<C>(
        instance_type: &str,
        price_type: &str,
        conn: &C,
    ) -> Result<Option<Self>, Error>
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
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn existing_entry(
        instance_type: &str,
        price_type: &str,
        pool: &PgPool,
    ) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::_existing_entry(instance_type, price_type, &conn)
            .await
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_all(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let query = query!("SELECT * FROM instance_pricing");
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
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

    /// # Errors
    /// Returns error if db query fails
    pub async fn upsert_entry(&self, pool: &PgPool) -> Result<Option<Self>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;

        let existing_entry =
            Self::_existing_entry(&self.instance_type, &self.price_type, conn).await?;

        if existing_entry.is_none() {
            self._insert_entry(conn).await?;
        } else {
            self._update_entry(conn).await?;
        }
        tran.commit().await?;
        Ok(existing_entry)
    }
}

#[derive(Clone, Copy)]
pub enum AwsGeneration {
    HVM,
    PV,
}

impl AwsGeneration {
    #[must_use]
    pub fn to_str(self) -> &'static str {
        match self {
            Self::HVM => "hvm",
            Self::PV => "pv",
        }
    }
}

impl fmt::Display for AwsGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
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
    #[must_use]
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
        f.write_str(self.to_str())
    }
}

impl From<PricingType> for StackString {
    fn from(p: PricingType) -> Self {
        p.to_str().into()
    }
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

impl AuthorizedUsers {
    /// # Errors
    /// Returns error if db query fails
    pub async fn get_authorized_users(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let query = query!("SELECT * FROM authorized_users");
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }
}

#[derive(FromSqlRow, Clone, Debug, PartialEq)]
pub struct InboundEmailDB {
    pub id: Uuid,
    pub s3_bucket: StackString,
    pub s3_key: StackString,
    pub from_address: StackString,
    pub to_address: StackString,
    pub subject: StackString,
    pub date: OffsetDateTime,
    pub text_content: StackString,
    pub html_content: StackString,
    pub raw_email: StackString,
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct InboundEmailBucketKey {
    pub id: Uuid,
    pub s3_bucket: StackString,
    pub s3_key: StackString,
}

impl InboundEmailDB {
    /// # Errors
    /// Returns error if db query fails
    pub async fn get_keys(pool: &PgPool) -> Result<Vec<InboundEmailBucketKey>, Error> {
        let query = query!(
            r"
                SELECT id, s3_bucket, s3_key
                FROM inbound_email
            "
        );
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_all(
        pool: &PgPool,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let mut query = format_sstr!("SELECT * FROM inbound_email ORDER BY date");
        if let Some(offset) = offset {
            query.push_str(&format_sstr!(" OFFSET {offset}"));
        }
        if let Some(limit) = limit {
            query.push_str(&format_sstr!(" LMIIT {limit}"));
        }
        let query = query_dyn!(&query)?;
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    async fn _get_by_id<C>(id: Uuid, conn: &C) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!("SELECT * FROM inbound_email WHERE id = $id", id = id,);
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::_get_by_id(id, &conn).await
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_by_bucket_key(
        pool: &PgPool,
        bucket: &str,
        key: &str,
    ) -> Result<Option<Self>, Error> {
        let query = query!(
            r"
                SELECT * FROM inbound_email
                WHERE bucket = $bucket
                  AND key = $key
            ",
            bucket = bucket,
            key = key,
        );
        let conn = pool.get().await?;
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r"
                INSERT INTO inbound_email (
                    id, s3_bucket, s3_key, from_address, to_address,
                    subject, date, text_content, html_content, raw_email
                ) VALUES (
                    $id, $s3_bucket, $s3_key, $from_address, $to_address,
                    $subject, $date, $text_content, $html_content, $raw_email
                )
            ",
            id = self.id,
            s3_bucket = self.s3_bucket,
            s3_key = self.s3_key,
            from_address = self.from_address,
            to_address = self.to_address,
            subject = self.subject,
            date = self.date,
            text_content = self.text_content,
            html_content = self.html_content,
            raw_email = self.raw_email,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r"
                UPDATE inbound_email
                SET s3_bucket=$s3_bucket,
                    s3_key=$s3_key,
                    from_address=$from_address,
                    to_address=$to_address,
                    subject=$subject,
                    date=$date,
                    text_content=$text_content,
                    html_content=$html_content,
                    raw_email=$raw_email
                WHERE id = $id
            ",
            id = self.id,
            s3_bucket = self.s3_bucket,
            s3_key = self.s3_key,
            from_address = self.from_address,
            to_address = self.to_address,
            subject = self.subject,
            date = self.date,
            text_content = self.text_content,
            html_content = self.html_content,
            raw_email = self.raw_email,
        );
        query.execute(conn).await?;
        Ok(())
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn upsert_entry(&self, pool: &PgPool) -> Result<Option<Self>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;

        let existing = Self::_get_by_id(self.id, conn).await?;

        if existing.is_some() {
            self._update_entry(conn).await?;
        } else {
            self._insert_entry(conn).await?;
        }
        tran.commit().await?;
        Ok(existing)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn delete_entry_by_id(id: Uuid, pool: &PgPool) -> Result<(), Error> {
        let query = query!("DELETE FROM inbound_email WHERE id = $id", id = id);
        let conn = pool.get().await?;
        query.execute(&conn).await?;
        Ok(())
    }
}
