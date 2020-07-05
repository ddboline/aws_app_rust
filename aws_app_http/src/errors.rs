use crate::logged_user::TRIGGER_DB_UPDATE;
use actix_web::{error::ResponseError, HttpResponse};
use anyhow::Error as AnyhowError;
use log::error;
use rust_auth_server::static_files;
use std::fmt::Debug;
use thiserror::Error;

use aws_app_lib::stack_string::StackString;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(StackString),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Anyhow error {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
}

// impl ResponseError trait allows to convert our errors into http responses
// with appropriate data
impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            Self::BadRequest(ref message) => HttpResponse::BadRequest().json(message),
            Self::Unauthorized => {
                TRIGGER_DB_UPDATE.set();
                static_files::login_html()
            }
            _ => {
                error!("Internal server error {:?}", self);
                HttpResponse::InternalServerError().json("Internal Server Error, Please try later")
            }
        }
    }
}
