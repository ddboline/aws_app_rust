use crate::logged_user::LOGIN_HTML;
use log::error;
use postgres_query::Error as PqError;
use rweb::{
    http::StatusCode,
    openapi::{
        ComponentDescriptor, ComponentOrInlineSchema, Entity, Response, ResponseEntity, Responses,
    },
    reject::{InvalidHeader, MissingCookie, Reject},
    Rejection, Reply,
};
use serde::Serialize;
use serde_yml::Error as YamlError;
use stack_string::StackString;
use std::{
    borrow::Cow,
    convert::Infallible,
    fmt::{Debug, Error as FmtError},
    string::FromUtf8Error,
};
use thiserror::Error;
use time_tz::system::Error as TzSystemError;

use authorized_users::errors::AuthUsersError;
use aws_app_lib::errors::AwslibError;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("YamlError {0}")]
    YamlError(#[from] YamlError),
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(StackString),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("AuthUsersError {0}")]
    AuthUsersError(#[from] AuthUsersError),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("FromUtf8Error {0}")]
    FromUtf8Error(Box<FromUtf8Error>),
    #[error("TzSystemError {0}")]
    TzSystemError(#[from] TzSystemError),
    #[error("PqError {0}")]
    PqError(Box<PqError>),
    #[error("FmtError {0}")]
    FmtError(#[from] FmtError),
    #[error("AwslibError {0}")]
    AwslibError(#[from] AwslibError),
}

impl From<PqError> for ServiceError {
    fn from(value: PqError) -> Self {
        Self::PqError(Box::new(value))
    }
}

impl From<FromUtf8Error> for ServiceError {
    fn from(value: FromUtf8Error) -> Self {
        Self::FromUtf8Error(Box::new(value))
    }
}

impl Reject for ServiceError {}

#[derive(Serialize)]
struct ErrorMessage<'a> {
    code: u16,
    message: &'a str,
}

fn login_html() -> impl Reply {
    rweb::reply::html(LOGIN_HTML)
}

/// # Errors
/// Never returns an error
pub async fn error_response(err: Rejection) -> Result<Box<dyn Reply>, Infallible> {
    let code: StatusCode;
    let message: &str;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT FOUND";
    } else if err.find::<InvalidHeader>().is_some() {
        return Ok(Box::new(login_html()));
    } else if let Some(missing_cookie) = err.find::<MissingCookie>() {
        if missing_cookie.name() == "jwt" {
            return Ok(Box::new(login_html()));
        }
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error";
    } else if let Some(service_err) = err.find::<ServiceError>() {
        match service_err {
            ServiceError::BadRequest(msg) => {
                code = StatusCode::BAD_REQUEST;
                message = msg.as_str();
            }
            ServiceError::Unauthorized => {
                return Ok(Box::new(login_html()));
            }
            _ => {
                error!("Other error: {:?}", service_err);
                code = StatusCode::INTERNAL_SERVER_ERROR;
                message = "Internal Server Error, Please try again later";
            }
        }
    } else if err.find::<rweb::reject::MethodNotAllowed>().is_some() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "METHOD NOT ALLOWED";
    } else {
        error!("Unknown error: {:?}", err);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error, Please try again later";
    };

    let reply = rweb::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message,
    });
    let reply = rweb::reply::with_status(reply, code);

    Ok(Box::new(reply))
}

impl Entity for ServiceError {
    fn type_name() -> Cow<'static, str> {
        rweb::http::Error::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        rweb::http::Error::describe(comp_d)
    }
}

impl ResponseEntity for ServiceError {
    fn describe_responses(_: &mut ComponentDescriptor) -> Responses {
        let mut map = Responses::new();

        let error_responses = [
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
            (StatusCode::BAD_REQUEST, "Bad Request"),
        ];

        for (code, msg) in &error_responses {
            map.insert(
                Cow::Owned(code.as_str().into()),
                Response {
                    description: Cow::Borrowed(*msg),
                    ..Response::default()
                },
            );
        }

        map
    }
}

#[cfg(test)]
mod test {
    use postgres_query::Error as PqError;
    use rweb::Reply;
    use serde_yml::Error as YamlError;
    use std::{fmt::Error as FmtError, string::FromUtf8Error};
    use time_tz::system::Error as TzSystemError;

    use authorized_users::errors::AuthUsersError;
    use aws_app_lib::errors::AwslibError;

    use crate::errors::{error_response, ServiceError as Error};

    #[tokio::test]
    async fn test_service_error() -> Result<(), Error> {
        let err = Error::BadRequest("TEST ERROR".into()).into();
        let resp = error_response(err).await.unwrap().into_response();
        assert_eq!(resp.status().as_u16(), 400);

        let err = Error::InternalServerError.into();
        let resp = error_response(err).await.unwrap().into_response();
        assert_eq!(resp.status().as_u16(), 500);
        Ok(())
    }

    #[test]
    fn test_error_size() {
        println!("YamlError {}", std::mem::size_of::<YamlError>());
        println!("AuthUsersError {}", std::mem::size_of::<AuthUsersError>());
        println!("std::io::Error {}", std::mem::size_of::<std::io::Error>());
        println!("FromUtf8Error {}", std::mem::size_of::<FromUtf8Error>());
        println!("TzSystemError {}", std::mem::size_of::<TzSystemError>());
        println!("PqError {}", std::mem::size_of::<PqError>());
        println!("FmtError {}", std::mem::size_of::<FmtError>());
        println!("AwslibError {}", std::mem::size_of::<AwslibError>());

        println!("Error {}", std::mem::size_of::<Error>());
        assert_eq!(std::mem::size_of::<Error>(), 32);
    }
}
