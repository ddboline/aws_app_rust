use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Query};
use actix_web::HttpResponse;
use failure::{err_msg, Error};

use super::app::AppState;
use super::logged_user::LoggedUser;

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

pub async fn sync_frontpage(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    form_http_response(include_str!("../../templates/index.html").to_string())
}
