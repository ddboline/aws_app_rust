use anyhow::{format_err, Error};
use mail_parser::{Message, MessageParser};
use mail_parser::MessagePart;
use stack_string::StackString;
use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{config::Config, models::InboundEmailDB, pgpool::PgPool, s3_instance::S3Instance};

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
    ) -> Result<Vec<StackString>, Error> {
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
            .get_list_of_keys(bucket, None)
            .await?
            .into_iter()
            .filter_map(|object| object.key.map(Into::into))
            .collect();

        let mut new_keys = Vec::new();
        for (key, entry) in &key_dict {
            if !remote_keys.contains(key.as_str()) {
                InboundEmailDB::delete_entry_by_id(entry.id, pool).await?;
            }
        }
        for key in &remote_keys {
            let key = key.as_str();
            if !key_dict.contains_key(key) {
                let raw_email = s3.download_to_string(bucket, key).await?;
                if let Some(message) = parser.parse(raw_email.as_bytes()) {
                    let email: InboundEmail = message.try_into()?;
                    email.into_db(bucket, key).upsert_entry(pool).await?;
                    new_keys.push(key.into());
                }
            }
        }

        Ok(new_keys)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use mail_parser::MessageParser;
    use std::convert::TryInto;

    use crate::{
        config::Config, inbound_email::InboundEmail, models::InboundEmailDB, pgpool::PgPool,
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
            Some(key.s3_key.clone())
        } else {
            None
        };

        let new_keys = InboundEmail::sync_db(&config, &s3, &pool).await?;
        assert!(new_keys.len() > 0);
        if let Some(existing) = existing {
            assert!(new_keys.contains(&existing));
        }

        Ok(())
    }
}
