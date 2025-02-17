use aws_config::SdkConfig;
pub use aws_sdk_iam::types::AccessKeyMetadata;
use aws_sdk_iam::{
    types::{AccessKey, Group, User},
    Client as IamClient,
};
use aws_types::region::Region;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use time::OffsetDateTime;

use crate::{date_time_wrapper::DateTimeWrapper, errors::AwslibError as Error};

#[derive(Clone)]
pub struct IamInstance {
    iam_client: IamClient,
}

impl IamInstance {
    #[must_use]
    pub fn new(sdk_config: &SdkConfig) -> Self {
        Self {
            iam_client: IamClient::from_conf(sdk_config.into()),
        }
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn set_region(&mut self, region: impl AsRef<str>) -> Result<(), Error> {
        let region: String = region.as_ref().into();
        let region = Region::new(region);
        let sdk_config = aws_config::from_env().region(region).load().await;
        self.iam_client = IamClient::from_conf((&sdk_config).into());
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn list_users(&self) -> Result<impl Iterator<Item = IamUser>, Error> {
        let users = self
            .iam_client
            .list_users()
            .send()
            .await?
            .users
            .into_iter()
            .filter_map(IamUser::from_user);
        Ok(users)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_user(
        &self,
        user_name: Option<impl Into<String>>,
    ) -> Result<Option<IamUser>, Error> {
        self.iam_client
            .get_user()
            .set_user_name(user_name.map(Into::into))
            .send()
            .await
            .map(|x| x.user.and_then(IamUser::from_user))
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn list_groups(&self) -> Result<impl Iterator<Item = IamGroup>, Error> {
        let groups = self
            .iam_client
            .list_groups()
            .send()
            .await?
            .groups
            .into_iter()
            .filter_map(IamGroup::from_group);
        Ok(groups)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn list_groups_for_user(
        &self,
        user_name: impl Into<String>,
    ) -> Result<impl Iterator<Item = IamGroup>, Error> {
        let groups = self
            .iam_client
            .list_groups_for_user()
            .user_name(user_name)
            .send()
            .await?
            .groups
            .into_iter()
            .filter_map(IamGroup::from_group);
        Ok(groups)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn create_user(
        &self,
        user_name: impl Into<String>,
    ) -> Result<Option<IamUser>, Error> {
        self.iam_client
            .create_user()
            .user_name(user_name)
            .send()
            .await
            .map(|r| r.user.and_then(IamUser::from_user))
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn delete_user(&self, user_name: impl Into<String>) -> Result<(), Error> {
        self.iam_client
            .delete_user()
            .user_name(user_name)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn add_user_to_group(
        &self,
        user_name: impl Into<String>,
        group_name: impl Into<String>,
    ) -> Result<(), Error> {
        self.iam_client
            .add_user_to_group()
            .user_name(user_name)
            .group_name(group_name)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn remove_user_from_group(
        &self,
        user_name: impl Into<String>,
        group_name: impl Into<String>,
    ) -> Result<(), Error> {
        self.iam_client
            .remove_user_from_group()
            .user_name(user_name)
            .group_name(group_name)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn list_access_keys(
        &self,
        user_name: impl Into<String>,
    ) -> Result<Vec<AccessKeyMetadata>, Error> {
        self.iam_client
            .list_access_keys()
            .user_name(user_name)
            .send()
            .await
            .map(|r| r.access_key_metadata)
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn create_access_key(
        &self,
        user_name: impl Into<String>,
    ) -> Result<Option<IamAccessKey>, Error> {
        self.iam_client
            .create_access_key()
            .user_name(user_name)
            .send()
            .await
            .map(|x| x.access_key.and_then(IamAccessKey::from_access_key))
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn delete_access_key(
        &self,
        user_name: impl Into<String>,
        access_key_id: impl Into<String>,
    ) -> Result<(), Error> {
        self.iam_client
            .delete_access_key()
            .access_key_id(access_key_id)
            .user_name(user_name)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct IamUser {
    pub arn: StackString,
    pub create_date: DateTimeWrapper,
    pub user_id: StackString,
    pub user_name: StackString,
    pub tags: HashMap<String, StackString>,
}

impl IamUser {
    fn from_user(user: User) -> Option<Self> {
        let create_date =
            OffsetDateTime::from_unix_timestamp(user.create_date.as_secs_f64() as i64)
                .ok()?
                .into();
        let tags = user
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.key, t.value.into()))
            .collect();
        Some(IamUser {
            arn: user.arn.into(),
            create_date,
            user_id: user.user_id.into(),
            user_name: user.user_name.into(),
            tags,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IamGroup {
    pub arn: StackString,
    pub create_date: OffsetDateTime,
    pub group_id: StackString,
    pub group_name: StackString,
}

impl IamGroup {
    fn from_group(group: Group) -> Option<Self> {
        let create_date =
            OffsetDateTime::from_unix_timestamp(group.create_date.as_secs_f64() as i64).ok()?;
        Some(Self {
            arn: group.arn.into(),
            create_date,
            group_id: group.group_id.into(),
            group_name: group.group_name.into(),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct IamAccessKey {
    pub access_key_id: StackString,
    pub create_date: DateTimeWrapper,
    pub access_key_secret: StackString,
    pub status: StackString,
    pub user_name: StackString,
}

impl IamAccessKey {
    fn from_access_key(key: AccessKey) -> Option<Self> {
        let create_date =
            OffsetDateTime::from_unix_timestamp(key.create_date?.as_secs_f64() as i64)
                .map_or_else(|_| DateTimeWrapper::now(), Into::into);
        Some(Self {
            access_key_id: key.access_key_id.into(),
            create_date,
            access_key_secret: key.secret_access_key.into(),
            status: key.status.as_str().into(),
            user_name: key.user_name.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use aws_sdk_sts::Client as StsClient;
    use std::collections::HashMap;

    use crate::{errors::AwslibError as Error, iam_instance::IamInstance};

    #[tokio::test]
    async fn test_list_users() -> Result<(), Error> {
        let sdk_config = aws_config::load_from_env().await;
        let sts = StsClient::from_conf((&sdk_config).into());
        let current_user_id = sts.get_caller_identity().send().await?.user_id.unwrap();
        println!("{current_user_id}");

        let iam = IamInstance::new((&sdk_config).into());
        let users_map: HashMap<_, _> = iam
            .list_users()
            .await?
            .map(|user| (user.user_id.clone(), user))
            .collect();
        println!("{:?}", users_map);
        assert!(users_map.contains_key(current_user_id.as_str()));

        let user_name: Option<&str> = None;
        let user = iam.get_user(user_name).await?.unwrap();
        assert_eq!(user.user_id, current_user_id);

        let groups: Vec<_> = iam.list_groups().await?.collect();
        println!("{:?}", groups);
        assert!(groups.len() > 0);
        Ok(())
    }
}
