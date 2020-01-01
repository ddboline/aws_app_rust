use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Query};
use actix_web::HttpResponse;
use aws_app_lib::resource_type::ResourceType;
use failure::{err_msg, Error};
use serde::{Deserialize, Serialize};

use super::app::AppState;
use super::logged_user::LoggedUser;
use super::requests::{
    CleanupEcrImagesRequest, DeleteEcrImageRequest, DeleteImageRequest, DeleteSnapshotRequest,
    DeleteVolumeRequest, HandleRequest, TerminateRequest,
};

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

// fn to_json<T>(js: &T) -> Result<HttpResponse, Error>
// where
//     T: Serialize,
// {
//     Ok(HttpResponse::Ok().json2(js))
// }

pub async fn sync_frontpage(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    let results = block(move || data.aws.handle(ResourceType::Instances))
        .await
        .map_err(err_msg)?;
    let body =
        include_str!("../../templates/index.html").replace("DISPLAY_TEXT", &results.join("<br>"));
    form_http_response(body)
}

#[derive(Serialize, Deserialize)]
pub struct ResourceRequest {
    resource: String,
}

pub async fn list(
    query: Query<ResourceRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query: ResourceType = query
        .into_inner()
        .resource
        .parse()
        .unwrap_or(ResourceType::Instances);
    let results = block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response(results.join("<br>"))
}

pub async fn terminate(
    query: Query<TerminateRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}

pub async fn delete_image(
    query: Query<DeleteImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}

pub async fn delete_volume(
    query: Query<DeleteVolumeRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}

pub async fn delete_snapshot(
    query: Query<DeleteSnapshotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}

pub async fn delete_ecr_image(
    query: Query<DeleteEcrImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}

pub async fn cleanup_ecr_images(
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    block(move || data.aws.handle(CleanupEcrImagesRequest {}))
        .await
        .map_err(err_msg)?;
    form_http_response("finished".to_string())
}
