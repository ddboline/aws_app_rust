use actix_threadpool::BlockingError;
use actix_web::{error::ResponseError, HttpResponse};
use anyhow::Error as AnyhowError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Anyhow error {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("blocking error {0}")]
    BlockingError(String),
}

// impl ResponseError trait allows to convert our errors into http responses with appropriate data
impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            Self::BadRequest(ref message) => HttpResponse::BadRequest().json(message),
            Self::Unauthorized => HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(
                    include_str!("../../templates/login.html")
                        .replace("main.css", "/auth/main.css")
                        .replace("main.js", "/auth/main.js"),
                ),
            _ => {
                HttpResponse::InternalServerError().json("Internal Server Error, Please try later")
            }
        }
    }
}

impl From<BlockingError<AnyhowError>> for ServiceError {
    fn from(item: BlockingError<AnyhowError>) -> Self {
        Self::BlockingError(item.to_string())
    }
}
