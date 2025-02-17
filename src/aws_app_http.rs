#![allow(clippy::semicolon_if_nothing_returned)]
use aws_app_http::{app::start_app, errors::ServiceError as Error};
use aws_app_lib::errors::AwslibError;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    tokio::spawn(async move { start_app().await })
        .await
        .map_err(Into::<AwslibError>::into)?
}
