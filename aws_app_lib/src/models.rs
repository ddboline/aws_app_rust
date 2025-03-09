use futures::Stream;
use mail_parser::{MessageParser, MimeHeaders, PartType};
use postgres_query::{client::GenericClient, query, query_dyn, Error as PqError, FromSqlRow};
use roxmltree::{Document, NodeType};
use stack_string::{format_sstr, StackString};
use std::{collections::HashSet, fmt};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::fs;
use uuid::Uuid;

use crate::{
    config::Config,
    errors::AwslibError as Error,
    pgpool::{PgPool, PgTransaction},
    s3_instance::S3Instance,
};

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

    async fn insert_entry_impl<C>(&self, conn: &C) -> Result<(), Error>
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

    async fn update_entry<C>(&self, conn: &C) -> Result<(), Error>
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
            self.update_entry(conn).await?;
        } else {
            self.insert_entry_impl(conn).await?;
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
        Self::_get_by_instance_type(instance_type, &conn).await
    }

    async fn insert_entry_impl<C>(&self, conn: &C) -> Result<(), Error>
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

    async fn update_entry<C>(&self, conn: &C) -> Result<(), Error>
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
            self.update_entry(conn).await?;
        } else {
            self.insert_entry_impl(conn).await?;
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
        Self::_existing_entry(instance_type, price_type, &conn).await
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

    async fn insert_entry_impl<C>(&self, conn: &C) -> Result<(), Error>
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

    async fn update_entry<C>(&self, conn: &C) -> Result<(), Error>
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
            self.insert_entry_impl(conn).await?;
        } else {
            self.update_entry(conn).await?;
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
    pub created_at: OffsetDateTime,
}

impl AuthorizedUsers {
    /// # Errors
    /// Returns error if db query fails
    pub async fn get_authorized_users(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let query = query!("SELECT * FROM authorized_users WHERE deleted_at IS NULL");
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_most_recent(pool: &PgPool) -> Result<Option<OffsetDateTime>, Error> {
        #[derive(FromSqlRow)]
        struct CreatedDeleted {
            created_at: Option<OffsetDateTime>,
            deleted_at: Option<OffsetDateTime>,
        }

        let query = query!(
            "SELECT max(created_at) as created_at, max(deleted_at) as deleted_at FROM \
             authorized_users"
        );
        let conn = pool.get().await?;
        let result: Option<CreatedDeleted> = query.fetch_opt(&conn).await?;
        match result {
            Some(result) => Ok(result.created_at.max(result.deleted_at)),
            None => Ok(None),
        }
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
    pub async fn get_keys(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<InboundEmailBucketKey, PqError>>, Error> {
        let query = query!(
            r"
                SELECT id, s3_bucket, s3_key
                FROM inbound_email
            "
        );
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
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

    async fn insert_entry_impl<C>(&self, conn: &C) -> Result<(), Error>
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

    async fn update_entry<C>(&self, conn: &C) -> Result<(), Error>
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
            self.update_entry(conn).await?;
        } else {
            self.insert_entry_impl(conn).await?;
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

    /// # Errors
    /// Returns error if db query fails
    pub async fn extract_attachments(
        &self,
        config: &Config,
        s3: &S3Instance,
    ) -> Result<Vec<StackString>, Error> {
        let mut extracted_attachments = Vec::new();
        let parser = MessageParser::default();
        let bucket = config
            .inbound_email_bucket
            .as_ref()
            .ok_or_else(|| Error::StaticCustomError("No Inbound Email Bucket"))?;

        let attachments: HashSet<StackString> = s3
            .get_list_of_keys(bucket, Some("attachments/"))
            .await?
            .into_iter()
            .filter_map(|object| object.key.map(Into::into))
            .collect();

        let tdir = TempDir::new()?;
        if let Some(message) = parser.parse(self.raw_email.as_bytes()) {
            for attachment in message.attachments() {
                if let PartType::Binary(body) = &attachment.body {
                    if let Some(filename) = attachment
                        .content_disposition()
                        .and_then(|c| c.attribute("filename").or_else(|| c.attribute("name")))
                    {
                        let s3key = format_sstr!("attachments/{filename}");
                        if attachments.contains(&s3key) {
                            continue;
                        }
                        let filepath = tdir.path().join(filename);
                        fs::write(&filepath, &body).await?;
                        s3.upload(&filepath, bucket, &s3key).await?;
                        extracted_attachments.push(s3key);
                        if let Some(content_type) = attachment.content_disposition() {
                            for a in content_type.attributes().unwrap_or(&[]) {
                                println!("{a:?}");
                            }
                        }
                    }
                }
            }
        }
        Ok(extracted_attachments)
    }
}

#[derive(FromSqlRow, Clone, Debug, PartialEq)]
pub struct DmarcRecords {
    pub id: Uuid,
    pub s3_key: Option<StackString>,
    pub org_name: Option<StackString>,
    pub email: Option<StackString>,
    pub report_id: Option<StackString>,
    pub date_range_begin: Option<i32>,
    pub date_range_end: Option<i32>,
    pub policy_domain: Option<StackString>,
    pub source_ip: Option<StackString>,
    pub count: Option<i32>,
    pub auth_result_type: Option<StackString>,
    pub auth_result_domain: Option<StackString>,
    pub auth_result_result: Option<StackString>,
    pub created_at: OffsetDateTime,
}

impl Default for DmarcRecords {
    fn default() -> Self {
        Self::new()
    }
}

impl DmarcRecords {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            s3_key: None,
            org_name: None,
            email: None,
            report_id: None,
            date_range_begin: None,
            date_range_end: None,
            policy_domain: None,
            source_ip: None,
            count: None,
            auth_result_type: None,
            auth_result_domain: None,
            auth_result_result: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn get_parsed_s3_keys(pool: &PgPool) -> Result<HashSet<StackString>, Error> {
        #[derive(FromSqlRow)]
        struct RecordKeys {
            s3_key: StackString,
        }

        let query = query!("SELECT distinct s3_key FROM dmarc_records WHERE s3_key IS NOT NULL");
        let conn = pool.get().await?;
        let result: Vec<RecordKeys> = query.fetch(&conn).await?;
        Ok(result.into_iter().map(|r| r.s3_key).collect())
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn delete_by_s3_key(s3_key: &str, pool: &PgPool) -> Result<u64, Error> {
        let query = query!(
            "DELETE FROM dmarc_records WHERE s3_key = $s3_key",
            s3_key = s3_key
        );
        let conn = pool.get().await?;
        query.execute(&conn).await.map_err(Into::into)
    }

    async fn insert_entry_impl<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r"
                INSERT INTO dmarc_records (
                    id, s3_key, org_name, email, report_id, date_range_begin,
                    date_range_end, policy_domain, source_ip, count,
                    auth_result_type, auth_result_domain, auth_result_result,
                    created_at
                ) VALUES (
                    $id, $s3_key, $org_name, $email, $report_id, $date_range_begin,
                    $date_range_end, $policy_domain, $source_ip, $count,
                    $auth_result_type, $auth_result_domain, $auth_result_result,
                    $created_at
                )
            ",
            id = self.id,
            s3_key = self.s3_key,
            org_name = self.org_name,
            email = self.email,
            report_id = self.report_id,
            date_range_begin = self.date_range_begin,
            date_range_end = self.date_range_end,
            policy_domain = self.policy_domain,
            source_ip = self.source_ip,
            count = self.count,
            auth_result_type = self.auth_result_type,
            auth_result_domain = self.auth_result_domain,
            auth_result_result = self.auth_result_result,
            created_at = self.created_at,
        );
        query.execute(conn).await?;
        Ok(())
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        self.insert_entry_impl(&conn).await?;
        Ok(())
    }

    /// # Errors
    /// Returns error if XML parsing fails
    pub fn parse_xml(body: &str, s3_key: Option<&str>) -> Result<Vec<Self>, Error> {
        let mut records = Vec::new();
        let mut dmarc_record = DmarcRecords::new();
        if let Some(s3_key) = s3_key {
            dmarc_record.s3_key = Some(s3_key.into());
        }
        let doc = Document::parse(body)?;
        for d in doc.root().descendants() {
            if d.node_type() == NodeType::Element && d.tag_name().name() == "org_name" {
                dmarc_record.org_name = d.text().map(Into::into);
            }
            if d.node_type() == NodeType::Element && d.tag_name().name() == "email" {
                dmarc_record.email = d.text().map(Into::into);
            }
            if d.node_type() == NodeType::Element && d.tag_name().name() == "report_id" {
                dmarc_record.report_id = d.text().map(Into::into);
            }
            if d.node_type() == NodeType::Element && d.tag_name().name() == "date_range" {
                for d0 in d.descendants() {
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "begin" {
                        dmarc_record.date_range_begin = d0.text().and_then(|t| t.parse().ok());
                    }
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "end" {
                        dmarc_record.date_range_end = d0.text().and_then(|t| t.parse().ok());
                    }
                }
            }
            if d.node_type() == NodeType::Element && d.tag_name().name() == "policy_published" {
                for d0 in d.descendants() {
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "domain" {
                        dmarc_record.policy_domain = d0.text().map(Into::into);
                    }
                }
            }
            if d.node_type() == NodeType::Element && d.tag_name().name() == "record" {
                for d0 in d.descendants() {
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "source_ip" {
                        dmarc_record.source_ip = d0.text().map(Into::into);
                    }
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "count" {
                        dmarc_record.count = d0.text().and_then(|t| t.parse().ok());
                    }
                    if d0.node_type() == NodeType::Element && d0.tag_name().name() == "auth_results"
                    {
                        for d1 in d0.descendants() {
                            for t in ["dkim", "spf"] {
                                if d1.node_type() == NodeType::Element && d1.tag_name().name() == t
                                {
                                    let mut dmarc_record1 = dmarc_record.clone();
                                    dmarc_record1.id = Uuid::new_v4();
                                    dmarc_record1.auth_result_type = Some(t.into());
                                    for d2 in d1.descendants() {
                                        if d2.node_type() == NodeType::Element
                                            && d2.tag_name().name() == "domain"
                                        {
                                            dmarc_record1.auth_result_domain =
                                                d2.text().map(Into::into);
                                        }
                                        if d2.node_type() == NodeType::Element
                                            && d2.tag_name().name() == "result"
                                        {
                                            dmarc_record1.auth_result_result =
                                                d2.text().map(Into::into);
                                        }
                                    }
                                    records.push(dmarc_record1);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use flate2::read::GzDecoder;
    use std::{fs, io::Read};
    use tempfile::TempDir;

    use crate::{errors::AwslibError as Error, models::DmarcRecords};

    #[tokio::test]
    async fn test_parse_xml() -> Result<(), Error> {
        let td = TempDir::new()?;
        let data = include_bytes!("../../tests/data/temp.xml.gz");
        let p = td.path().join("temp.xml.gz");
        fs::write(&p, data)?;
        let mut body = String::new();
        GzDecoder::new(fs::File::open(&p)?).read_to_string(&mut body)?;
        let records = DmarcRecords::parse_xml(&body, Some("test_key"))?;
        assert_eq!(records.len(), 21);
        Ok(())
    }
}
