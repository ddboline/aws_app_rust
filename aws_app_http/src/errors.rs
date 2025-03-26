use crate::logged_user::LOGIN_HTML;
use axum::{
    extract::Json,
    http::{StatusCode, header::InvalidHeaderName},
    response::{IntoResponse, Response},
};
use log::error;
use postgres_query::Error as PqError;
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use serde_yml::Error as YamlError;
use stack_string::{StackString, format_sstr};
use std::{
    fmt::{Debug, Error as FmtError},
    net::AddrParseError,
    string::FromUtf8Error,
};
use thiserror::Error;
use time_tz::system::Error as TzSystemError;
use utoipa::{
    IntoResponses, PartialSchema, ToSchema,
    openapi::{ContentBuilder, ResponseBuilder, ResponsesBuilder},
};

use authorized_users::errors::AuthUsersError;
use aws_app_lib::errors::AwslibError;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("AddrParseError {0}")]
    AddrParseError(#[from] AddrParseError),
    #[error("InvalidHeaderName {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
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

#[derive(Serialize, ToSchema)]
struct ErrorMessage {
    message: StackString,
}

impl axum::response::IntoResponse for ErrorMessage {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => (StatusCode::OK, LOGIN_HTML).into_response(),
            Self::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, ErrorMessage { message }).into_response()
            }
            e => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorMessage {
                    message: format_sstr!("Internal Server Error: {e}"),
                },
            )
                .into_response(),
        }
    }
}

impl IntoResponses for ServiceError {
    fn responses() -> std::collections::BTreeMap<
        String,
        utoipa::openapi::RefOr<utoipa::openapi::response::Response>,
    > {
        let error_message_content = ContentBuilder::new()
            .schema(Some(ErrorMessage::schema()))
            .build();
        ResponsesBuilder::new()
            .response(
                StatusCode::UNAUTHORIZED.as_str(),
                ResponseBuilder::new()
                    .description("Not Authorized")
                    .content(
                        "text/html",
                        ContentBuilder::new().schema(Some(String::schema())).build(),
                    ),
            )
            .response(
                StatusCode::BAD_REQUEST.as_str(),
                ResponseBuilder::new()
                    .description("Bad Request")
                    .content("application/json", error_message_content.clone()),
            )
            .response(
                StatusCode::INTERNAL_SERVER_ERROR.as_str(),
                ResponseBuilder::new()
                    .description("Internal Server Error")
                    .content("application/json", error_message_content.clone()),
            )
            .build()
            .into()
    }
}

#[cfg(test)]
mod test {
    use postgres_query::Error as PqError;
    use serde_yml::Error as YamlError;
    use std::{fmt::Error as FmtError, string::FromUtf8Error};
    use time_tz::system::Error as TzSystemError;

    use authorized_users::errors::AuthUsersError;
    use aws_app_lib::errors::AwslibError;

    use crate::errors::ServiceError as Error;

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
