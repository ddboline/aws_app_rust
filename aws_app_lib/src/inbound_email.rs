use anyhow::{format_err, Error};
use mail_parser::{Message, MessageParser, MessagePart};
use stack_string::StackString;
use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto}, io::Read,
};
use time::OffsetDateTime;
use uuid::Uuid;
use tempfile::NamedTempFile;
use flate2::read::GzDecoder;

use crate::{config::Config, models::{DmarcRecords, InboundEmailDB}, pgpool::PgPool, s3_instance::S3Instance};

#[derive(Debug)]
pub struct InboundEmail {
    pub from_address: StackString,
    pub to_address: StackString,
    pub subject: StackString,
    pub date: OffsetDateTime,
    pub text_content: StackString,
    pub html_content: StackString,
    pub raw_email: StackString,
}

impl From<InboundEmailDB> for InboundEmail {
    fn from(value: InboundEmailDB) -> Self {
        Self {
            from_address: value.from_address,
            to_address: value.to_address,
            subject: value.subject,
            date: value.date,
            text_content: value.text_content,
            html_content: value.html_content,
            raw_email: value.raw_email,
        }
    }
}

impl TryFrom<Message<'_>> for InboundEmail {
    type Error = Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let from_address: StackString = message
            .from()
            .and_then(|a| a.first())
            .and_then(|a| a.address())
            .map(Into::into)
            .ok_or_else(|| format_err!("No From Address"))?;
        let to_address: StackString = message
            .to()
            .and_then(|a| a.first())
            .and_then(|a| a.address())
            .map(Into::into)
            .ok_or_else(|| format_err!("No To Address"))?;
        let subject: StackString = message
            .subject()
            .map(Into::into)
            .ok_or_else(|| format_err!("No Subject Found"))?;
        let date = message
            .date()
            .and_then(|d| OffsetDateTime::from_unix_timestamp(d.to_timestamp()).ok())
            .unwrap_or_else(OffsetDateTime::now_utc);
        let text_content = message
            .text_bodies()
            .filter_map(MessagePart::text_contents)
            .fold(StackString::new(), |mut s, t| {
                s.push_str(t);
                s.push_str("\n");
                s
            });
        let html_content = message
            .html_bodies()
            .filter_map(MessagePart::text_contents)
            .fold(StackString::new(), |mut s, h| {
                s.push_str(h);
                s.push_str("\r\n");
                s
            });
        let raw_email = StackString::from_utf8(message.raw_message())?;
        Ok(Self {
            from_address,
            to_address,
            subject,
            date,
            text_content,
            html_content,
            raw_email,
        })
    }
}

impl InboundEmail {
    #[must_use]
    pub fn into_db(self, s3_bucket: &str, s3_key: &str) -> InboundEmailDB {
        InboundEmailDB {
            id: Uuid::new_v4(),
            s3_bucket: s3_bucket.into(),
            s3_key: s3_key.into(),
            from_address: self.from_address,
            to_address: self.to_address,
            subject: self.subject,
            date: self.date,
            text_content: self.text_content,
            html_content: self.html_content,
            raw_email: self.raw_email,
        }
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn sync_db(
        config: &Config,
        s3: &S3Instance,
        pool: &PgPool,
    ) -> Result<(Vec<StackString>, Vec<StackString>), Error> {
        let parser = MessageParser::default();
        let bucket = config
            .inbound_email_bucket
            .as_ref()
            .ok_or_else(|| format_err!("No Inbound Email Bucket"))?;
        let key_dict: HashMap<StackString, _> = InboundEmailDB::get_keys(pool)
            .await?
            .into_iter()
            .map(|ibk| (ibk.s3_key.clone(), ibk))
            .collect();
        let remote_keys: HashSet<StackString> = s3
            .get_list_of_keys(bucket, Some("inbound-email/"))
            .await?
            .into_iter()
            .filter_map(|object| object.key.map(Into::into))
            .collect();

        let mut new_keys = Vec::new();
        let mut new_attachments = Vec::new();
        for (key, entry) in &key_dict {
            if !remote_keys.contains(key.as_str()) {
                InboundEmailDB::delete_entry_by_id(entry.id, pool).await?;
            } else if let Some(email) = InboundEmailDB::get_by_id(pool, entry.id).await? {
                new_attachments.extend(email.extract_attachments(config, s3).await?);
            }
        }
        for key in &remote_keys {
            let key = key.as_str();
            if !key_dict.contains_key(key) {
                let raw_email = s3.download_to_string(bucket, key).await?;
                if let Some(message) = parser.parse(raw_email.as_bytes()) {
                    let email: InboundEmail = message.try_into()?;
                    let email = email.into_db(bucket, key);
                    email.upsert_entry(pool).await?;
                    email.extract_attachments(config, s3).await?;
                    new_keys.push(key.into());
                }
            }
        }

        Ok((new_keys, new_attachments))
    }

    /// # Errors
    /// Returns error if db query fails
    pub async fn parse_dmarc_records(config: &Config, s3: &S3Instance, pool: &PgPool) -> Result<Vec<DmarcRecords>, Error> {
        let mut new_records = Vec::new();
        let bucket = config
            .inbound_email_bucket
            .as_ref()
            .ok_or_else(|| format_err!("No Inbound Email Bucket"))?;

        let parsed_attachments: HashSet<StackString> = DmarcRecords::get_parsed_s3_keys(pool).await?.into_iter().collect();

        for attachment in s3
            .get_list_of_keys(bucket, Some("attachments/"))
            .await? {
                if let Some(key) = &attachment.key {
                    if !parsed_attachments.contains(key.as_str()) {
                        let f = NamedTempFile::new()?;
                        s3.download(bucket, key, f.path()).await?;
                        if let Some(t) = infer::get_from_path(f.path())? {
                            let mut buffer = String::new();
                            if t.mime_type() == "text/xml" {
                                buffer = tokio::fs::read_to_string(f.path()).await?;
                            } else if t.mime_type() == "application/gzip" {
                                GzDecoder::new(std::fs::File::open(f.path())?).read_to_string(&mut buffer)?;
                            }
                            if !buffer.is_empty() {
                                for record in DmarcRecords::parse_xml(&buffer, Some(key.as_str()))? {
                                    record.insert_entry(pool).await?;
                                    new_records.push(record);
                                }
                            }
                        }
                    }
                }
            }

        Ok(new_records)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use mail_parser::MessageParser;
    use stack_string::{format_sstr, StackString};
    use std::{convert::TryInto, fmt::Write};

    use crate::{
        config::Config, inbound_email::InboundEmail, models::{DmarcRecords, InboundEmailDB}, pgpool::PgPool,
        s3_instance::S3Instance,
    };

    #[test]
    fn test_email_format() -> Result<(), Error> {
        let data = include_str!("../../tests/data/AMAZON_SES_SETUP_NOTIFICATION");

        let parser = MessageParser::default();
        let message = parser.parse(data.as_bytes()).unwrap();

        assert_eq!(message.text_body_count(), 1);

        let email: InboundEmail = message.try_into()?;

        assert_eq!(email.subject, "Amazon SES Setup Notification");
        assert_eq!(email.from_address, "no-reply-aws@amazon.com");
        assert_eq!(email.to_address, "recipient@example.com");

        assert!(email
            .text_content
            .contains("Thank you for using Amazon SES!"));

        let data = include_str!("../../tests/data/example_html_email");
        let message = parser.parse(data.as_bytes()).unwrap();

        assert_eq!(message.text_body_count(), 1);

        for header in message.headers() {
            let header_name = header.name();
            let mut address_list = Vec::new();
            let mut groups_list = Vec::new();
            if let Some(address) = header.value().as_address() {
                if let Some(list) = address.as_list() {
                    for l in list {
                        address_list.push(l.address().unwrap_or(""));
                    }
                }
                if let Some(groups) = address.as_group() {
                    for g in groups {
                        let name = g.name.as_ref().map_or("", |s| s.as_ref());
                        let mut group_addresses = Vec::new();
                        for a in &g.addresses {
                            group_addresses.push(a.address().unwrap_or(""));
                        }
                        groups_list.push(format_sstr!("{name} {}", group_addresses.join(",")));
                    }
                }
            }
            let text_list = header.value().as_text_list().unwrap_or_default().join(" ");
            if let Some(content_type) = header.value().as_content_type() {
                println!("ctype {}", content_type.ctype());
            }
            let mut received_host = StackString::new();
            if let Some(received) = header.value().as_received() {
                if let Some(host) = received.from() {
                    write!(&mut received_host, " from: {host}").unwrap();
                }
                if let Some(host) = received.by() {
                    write!(&mut received_host, " by: {host}").unwrap();
                }
                if let Some(f) = received.for_() {
                    write!(&mut received_host, " for: {f}").unwrap();
                }
            }
            println!(
                "name {header_name} {} {} {text_list} {received_host}",
                address_list.join(","),
                groups_list.join(" ")
            );
        }

        let email: InboundEmail = message.try_into()?;

        assert_eq!(email.subject, "Test");
        assert_eq!(email.from_address, "daniel.boline@agilischemicals.com");
        assert_eq!(email.to_address, "ddboline@ddboline.net");
        assert!(email
            .html_content
            .contains("Digital Commerce Platform Purpose Built For Chemical Industry"));

        Ok(())
    }

    #[tokio::test]
    async fn test_sync_inbound_email() -> Result<(), Error> {
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let sdk_config = aws_config::load_from_env().await;
        let s3 = S3Instance::new(&sdk_config);

        let existing = if let Some(key) = InboundEmailDB::get_keys(&pool).await?.first() {
            InboundEmailDB::delete_entry_by_id(key.id, &pool).await?;
            println!("found key {}", key.s3_key);
            Some(key.s3_key.clone())
        } else {
            None
        };

        let (new_keys, _) = InboundEmail::sync_db(&config, &s3, &pool).await?;
        if let Some(existing) = &existing {
            assert!(new_keys.len() > 0);
            assert!(new_keys.contains(existing));
        }

        if let Some(existing) = DmarcRecords::get_parsed_s3_keys(&pool).await?.pop() {
            DmarcRecords::delete_by_s3_key(&existing, &pool).await?;
        }

        let new_records = InboundEmail::parse_dmarc_records(&config, &s3, &pool).await?;
        if existing.is_some() {
            assert!(new_records.len() > 0);
        }

        Ok(())
    }
}
