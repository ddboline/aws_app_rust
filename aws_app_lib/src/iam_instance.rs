use anyhow::Error;
use chrono::{DateTime, Utc};
use rusoto_core::Region;
use rusoto_iam::{
    AccessKey, AccessKeyMetadata, AddUserToGroupRequest, CreateAccessKeyRequest, CreateUserRequest,
    DeleteAccessKeyRequest, DeleteUserRequest, GetUserRequest, Group, Iam as _, IamClient,
    ListAccessKeysRequest, ListGroupsForUserRequest, ListGroupsRequest, ListUsersRequest,
    RemoveUserFromGroupRequest, User,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use sts_profile_auth::get_client_sts;

use crate::config::Config;

#[derive(Clone)]
pub struct IamInstance {
    iam_client: IamClient,
}

impl Default for IamInstance {
    fn default() -> Self {
        Self {
            iam_client: get_client_sts!(IamClient, Region::UsEast1).expect("StsProfile failed"),
        }
    }
}

impl IamInstance {
    pub fn new(config: &Config) -> Self {
        let config = config.clone();
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            iam_client: get_client_sts!(IamClient, region).expect("StsProfile failed"),
        }
    }

    pub async fn list_users(&self) -> Result<impl Iterator<Item = IamUser>, Error> {
        let users = self
            .iam_client
            .list_users(ListUsersRequest::default())
            .await?
            .users
            .into_iter()
            .map(Into::into);
        Ok(users)
    }

    pub async fn get_user(&self, user_name: Option<impl AsRef<str>>) -> Result<IamUser, Error> {
        self.iam_client
            .get_user(GetUserRequest {
                user_name: user_name.map(|s| s.as_ref().into()),
            })
            .await
            .map(|x| x.user.into())
            .map_err(Into::into)
    }

    pub async fn list_groups(&self) -> Result<impl Iterator<Item = IamGroup>, Error> {
        let groups = self
            .iam_client
            .list_groups(ListGroupsRequest::default())
            .await?
            .groups
            .into_iter()
            .map(Into::into);
        Ok(groups)
    }

    pub async fn list_groups_for_user(
        &self,
        user_name: &str,
    ) -> Result<impl Iterator<Item = IamGroup>, Error> {
        let groups = self
            .iam_client
            .list_groups_for_user(ListGroupsForUserRequest {
                user_name: user_name.into(),
                ..ListGroupsForUserRequest::default()
            })
            .await?
            .groups
            .into_iter()
            .map(Into::into);
        Ok(groups)
    }

    pub async fn create_user(&self, user_name: &str) -> Result<Option<IamUser>, Error> {
        self.iam_client
            .create_user(CreateUserRequest {
                user_name: user_name.into(),
                ..CreateUserRequest::default()
            })
            .await
            .map(|r| r.user.map(Into::into))
            .map_err(Into::into)
    }

    pub async fn delete_user(&self, user_name: &str) -> Result<(), Error> {
        self.iam_client
            .delete_user(DeleteUserRequest {
                user_name: user_name.into(),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn add_user_to_group(&self, user_name: &str, group_name: &str) -> Result<(), Error> {
        self.iam_client
            .add_user_to_group(AddUserToGroupRequest {
                user_name: user_name.into(),
                group_name: group_name.into(),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn remove_user_from_group(
        &self,
        user_name: &str,
        group_name: &str,
    ) -> Result<(), Error> {
        self.iam_client
            .remove_user_from_group(RemoveUserFromGroupRequest {
                user_name: user_name.into(),
                group_name: group_name.into(),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn list_access_keys(&self, user_name: &str) -> Result<Vec<AccessKeyMetadata>, Error> {
        self.iam_client
            .list_access_keys(ListAccessKeysRequest {
                user_name: Some(user_name.into()),
                ..ListAccessKeysRequest::default()
            })
            .await
            .map(|x| x.access_key_metadata)
            .map_err(Into::into)
    }

    pub async fn create_access_key(&self, user_name: &str) -> Result<IamAccessKey, Error> {
        self.iam_client
            .create_access_key(CreateAccessKeyRequest {
                user_name: Some(user_name.into()),
            })
            .await
            .map(|x| x.access_key.into())
            .map_err(Into::into)
    }

    pub async fn delete_access_key(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), Error> {
        self.iam_client
            .delete_access_key(DeleteAccessKeyRequest {
                access_key_id: access_key_id.into(),
                user_name: Some(user_name.into()),
            })
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IamUser {
    pub arn: StackString,
    pub create_date: DateTime<Utc>,
    pub user_id: StackString,
    pub user_name: StackString,
    pub tags: HashMap<String, StackString>,
}

impl From<User> for IamUser {
    fn from(user: User) -> Self {
        let create_date: DateTime<Utc> = user.create_date.parse().unwrap_or_else(|_| Utc::now());
        let tags = user
            .tags
            .unwrap_or_else(Vec::new)
            .into_iter()
            .map(|t| (t.key, t.value.into()))
            .collect();
        IamUser {
            arn: user.arn.into(),
            create_date: create_date.into(),
            user_id: user.user_id.into(),
            user_name: user.user_name.into(),
            tags,
        }
    }
}

#[derive(Debug)]
pub struct IamGroup {
    pub arn: StackString,
    pub create_date: DateTime<Utc>,
    pub group_id: StackString,
    pub group_name: StackString,
}

impl From<Group> for IamGroup {
    fn from(group: Group) -> Self {
        let create_date = group.create_date.parse().unwrap_or_else(|_| Utc::now());
        Self {
            arn: group.arn.into(),
            create_date,
            group_id: group.group_id.into(),
            group_name: group.group_name.into(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct IamAccessKey {
    pub access_key_id: StackString,
    pub create_date: DateTime<Utc>,
    pub access_key_secret: StackString,
    pub status: StackString,
    pub user_name: StackString,
}

impl From<AccessKey> for IamAccessKey {
    fn from(key: AccessKey) -> Self {
        let create_date = key
            .create_date
            .and_then(|dt| dt.parse().ok())
            .unwrap_or_else(Utc::now);
        Self {
            access_key_id: key.access_key_id.into(),
            create_date: create_date.into(),
            access_key_secret: key.secret_access_key.into(),
            status: key.status.into(),
            user_name: key.user_name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use std::collections::HashMap;
    use sts_profile_auth::StsInstance;

    use crate::{config::Config, iam_instance::IamInstance};

    #[tokio::test]
    async fn test_list_users() -> Result<(), Error> {
        let sts = StsInstance::new(None)?;
        let current_user = sts.get_user_id().await?;
        println!("{:?}", current_user);
        let current_user_id = current_user.user_id.expect("No User Id?");

        let config = Config::init_config()?;
        let iam = IamInstance::new(&config);
        let users_map: HashMap<_, _> = iam
            .list_users()
            .await?
            .map(|user| (user.user_id.clone(), user))
            .collect();
        println!("{:?}", users_map);
        assert!(users_map.contains_key(current_user_id.as_str()));

        let user_name: Option<&str> = None;
        let user = iam.get_user(user_name).await?;
        assert_eq!(user.user_id, current_user_id);

        let groups: Vec<_> = iam.list_groups().await?.collect();
        println!("{:?}", groups);
        assert!(groups.len() > 0);
        Ok(())
    }
}
