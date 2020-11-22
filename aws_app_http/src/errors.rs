use crate::logged_user::TRIGGER_DB_UPDATE;
use actix_web::{error::ResponseError, HttpResponse};
use anyhow::Error as AnyhowError;
use log::error;
use stack_string::StackString;
use std::fmt::Debug;
use thiserror::Error;

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
                login_html()
            }
            _ => {
                error!("Internal server error {:?}", self);
                HttpResponse::InternalServerError().json("Internal Server Error, Please try later")
            }
        }
    }
}

fn login_html() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            "
            <script>
                !function() {
                    let final_url = location.href;
                    location.replace('/auth/login.html?final_url=' + final_url);
                }()
            </script>
        ",
        )
}
