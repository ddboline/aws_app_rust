use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Json, Query};
use actix_web::HttpResponse;
use aws_app_lib::resource_type::ResourceType;
use failure::{err_msg, Error};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::fs::{remove_file, File};
use std::io::{Read, Write};
use std::path::Path;

use aws_app_lib::config::Config;
use aws_app_lib::ec2_instance::SpotRequest;

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

#[derive(Serialize, Deserialize)]
pub struct EditData {
    pub filename: String,
}

pub async fn edit_script(
    query: Query<EditData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let filename = format!("{}/{}", data.aws.config.script_directory, query.filename);
    let mut text = String::new();
    if Path::new(&filename).exists() {
        File::open(&filename)?.read_to_string(&mut text)?;
    }
    let body = format!(
        r#"
        <textarea name="message" id="script_editor_form" rows=50 cols=100
        form="script_edit_form">{text}</textarea><br>
        <form id="script_edit_form">
        <input type="button" name="update" value="Update" onclick="submitFormData('{fname}')">
        <input type="button" name="cancel" value="Cancel" onclick="listResource('script')">
        </form>"#,
        text = text,
        fname = &query.filename,
    );
    form_http_response(body)
}

#[derive(Serialize, Deserialize)]
pub struct ReplaceData {
    pub filename: String,
    pub text: String,
}

pub async fn replace_script(
    req: Json<ReplaceData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let req = req.into_inner();
    let filename = format!("{}/{}", data.aws.config.script_directory, req.filename);
    let mut f = File::create(&filename)?;
    write!(f, "{}", req.text)?;
    form_http_response("done".to_string())
}

pub async fn delete_script(
    query: Query<EditData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let filename = format!("{}/{}", data.aws.config.script_directory, query.filename);
    let p = Path::new(&filename);
    if p.exists() {
        remove_file(p)?;
    }
    form_http_response("done".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct SpotBuilder {
    pub ami: String,
}

pub async fn build_spot_request(
    query: Query<SpotBuilder>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();

    let d = data.clone();
    let mut amis = block(move || d.aws.get_all_ami_tags())
        .await
        .map_err(err_msg)?;
    let mut ami_opt: Vec<_> = amis
        .iter()
        .filter(|ami| ami.id == query.ami)
        .cloned()
        .collect();
    amis.retain(|ami| ami.id != query.ami);
    ami_opt.extend_from_slice(&amis);
    let amis: Vec<_> = ami_opt
        .into_iter()
        .map(|ami| format!(r#"<option value="{}">{}</option>"#, ami.id, ami.name,))
        .collect();

    let d = data.clone();
    let files: Vec<_> = block(move || d.aws.get_all_scripts())
        .await
        .map_err(err_msg)?
        .into_iter()
        .map(|f| format!(r#"<option value="{f}">{f}</option>"#, f = f))
        .collect();

    let d = data.clone();
    let keys: Vec<_> = block(move || d.aws.ec2.get_all_key_pairs())
        .await
        .map_err(err_msg)?
        .into_iter()
        .map(|k| format!(r#"<option value="{k}">{k}</option>"#, k = k.0))
        .collect();

    let body = format!(
        r#"
            <form action="javascript:createScript()">
            Ami: <select id="ami">{}</select><br>
            Instance type: <input type="text" name="instance_type" id="instance_type" value="t3.nano"/><br>
            Security group: <input type="text" name="security_group" id="security_group" value="{}"/><br>
            Script: <select id="script">{}</select><br>
            Key: <select id="key">{}</select><br>
            Price: <input type="text" name="price" id="price" value="{}"/><br>
            Name: <input type="text" name="name" id="name"/><br>
            <input type="button" name="create_request" value="Request" onclick="requestSpotInstance();"/><br>
            </form>
        "#,
        amis.join("\n"),
        data.aws.config.spot_security_group,
        files.join("\n"),
        keys.join("\n"),
        data.aws.config.max_spot_price,
    );

    form_http_response(body)
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotRequestData {
    pub ami: String,
    pub instance_type: String,
    pub security_group: String,
    pub script: String,
    pub key_name: String,
    pub price: String,
    pub name: String,
}

impl SpotRequestData {
    pub fn into_spot_request(self, config: &Config) -> SpotRequest {
        SpotRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group: self.security_group,
            script: self.script,
            key_name: self.key_name,
            price: self.price.parse().unwrap_or(config.max_spot_price),
            tags: hashmap! { "Name".to_string() => self.name },
        }
    }
}

pub async fn request_spot(
    req: Json<SpotRequestData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let req = req.into_inner().into_spot_request(&data.aws.config);
    block(move || data.aws.ec2.request_spot_instance(&req))
        .await
        .map_err(err_msg)?;
    form_http_response("done".to_string())
}
