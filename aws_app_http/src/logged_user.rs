use authorized_users::AuthInfo;
pub use authorized_users::{
    AUTHORIZED_USERS, AuthorizedUser, AuthorizedUser as ExternalUser, JWT_SECRET, KEY_LENGTH,
    LOGIN_HTML, SECRET_KEY, get_random_key, get_secrets, token::Token,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use futures::TryStreamExt;
use log::debug;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    env::var,
    str::FromStr,
};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use aws_app_lib::{models::AuthorizedUsers as AuthorizedUsersDB, pgpool::PgPool};

use crate::errors::ServiceError as Error;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, ToSchema)]
// LoggedUser
pub struct LoggedUser {
    #[schema(example = r#""user@example.com""#, inline)]
    // Email Address
    pub email: StackString,
    // Session Id
    #[schema(inline)]
    pub session: Uuid,
    // User Created At
    #[schema(inline)]
    pub created_at: OffsetDateTime,
}

impl LoggedUser {
    /// # Errors
    /// Returns `Error::Unauthorized` if `session_id` does not match
    /// `self.session`
    pub fn verify_session_id(self, session_id: Uuid) -> Result<Self, Error> {
        if self.session == session_id {
            Ok(self)
        } else {
            Err(Error::Unauthorized)
        }
    }

    fn extract_user_from_cookies(cookie_jar: &CookieJar) -> Option<LoggedUser> {
        let session_id: Uuid = StackString::from_display(cookie_jar.get("session-id")?.encoded())
            .strip_prefix("session-id=")?
            .parse()
            .ok()?;
        debug!("session_id {session_id:?}");
        let user: LoggedUser = StackString::from_display(cookie_jar.get("jwt")?.encoded())
            .strip_prefix("jwt=")?
            .parse()
            .ok()?;
        debug!("user {user:?}");
        user.verify_session_id(session_id).ok()
    }
}

impl From<AuthorizedUser> for LoggedUser {
    fn from(user: AuthorizedUser) -> Self {
        Self {
            email: user.get_email().into(),
            session: user.get_session(),
            created_at: user.get_created_at(),
        }
    }
}

impl TryFrom<Token> for LoggedUser {
    type Error = Error;
    fn try_from(token: Token) -> Result<Self, Self::Error> {
        if let Ok(user) = token.try_into() {
            if AUTHORIZED_USERS.is_authorized(&user) {
                return Ok(user.into());
            }
            debug!("NOT AUTHORIZED {user:?}",);
        }
        Err(Error::Unauthorized)
    }
}

impl FromStr for LoggedUser {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut buf = StackString::new();
        buf.push_str(s);
        let token: Token = buf.into();
        token.try_into()
    }
}

impl<S> FromRequestParts<S> for LoggedUser
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let cookie_jar = CookieJar::from_request_parts(parts, state)
            .await
            .expect("extract failed");
        debug!("cookie_jar {cookie_jar:?}");
        let user = LoggedUser::extract_user_from_cookies(&cookie_jar)
            .ok_or_else(|| Error::Unauthorized)?;
        Ok(user)
    }
}

/// # Errors
/// Returns error if `get_authorized_users` fails
pub async fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    if let Ok("true") = var("TESTENV").as_ref().map(String::as_str) {
        AUTHORIZED_USERS.update_users(hashmap! {
            "user@test".into() => ExternalUser::new(
                "user@test",
                Uuid::new_v4(),
                "",
            )
        });
        return Ok(());
    }
    let most_recent_user_db = AuthorizedUsersDB::get_most_recent(pool).await?;
    let existing_users = AUTHORIZED_USERS.get_users();
    let most_recent_user = existing_users.values().map(AuthInfo::get_created_at).max();
    if most_recent_user_db.is_some()
        && most_recent_user.is_some()
        && most_recent_user_db <= most_recent_user
    {
        return Ok(());
    }
    debug!("most_recent_user_db {most_recent_user_db:?} most_recent_user {most_recent_user:?}");

    let result: Result<HashMap<StackString, _>, _> = AuthorizedUsersDB::get_authorized_users(pool)
        .await?
        .map_ok(|u| {
            (
                u.email.clone(),
                ExternalUser::new(&u.email, Uuid::new_v4(), ""),
            )
        })
        .try_collect()
        .await;
    let users = result?;
    AUTHORIZED_USERS.update_users(users);
    debug!("AUTHORIZED_USERS {:?}", *AUTHORIZED_USERS);
    Ok(())
}

#[cfg(test)]
mod tests {
    use authorized_users::AuthorizedUser;
    use uuid::Uuid;

    use crate::logged_user::LoggedUser;

    #[test]
    fn test_authorized_user_to_logged_user() {
        let email = "test@localhost";
        let user = AuthorizedUser::new(email, Uuid::new_v4(), "");
        let user: LoggedUser = user.into();

        assert_eq!(user.email, email);
    }
}
