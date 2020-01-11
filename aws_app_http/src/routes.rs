use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Json, Query};
use actix_web::HttpResponse;
use aws_app_lib::resource_type::ResourceType;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::fs::{remove_file, File};
use std::io::{Read, Write};
use std::path::Path;

use aws_app_lib::config::Config;
use aws_app_lib::ec2_instance::SpotRequest;
use aws_app_lib::models::InstanceFamily;

use super::app::AppState;
use super::errors::ServiceError as Error;
use super::logged_user::LoggedUser;
use super::requests::{
    CleanupEcrImagesRequest, DeleteEcrImageRequest, DeleteImageRequest, DeleteSnapshotRequest,
    DeleteVolumeRequest, HandleRequest, StatusRequest, TerminateRequest,
};

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

pub async fn sync_frontpage(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    let results = block(move || data.aws.handle(ResourceType::Instances)).await?;
    let body =
        include_str!("../../templates/index.html").replace("DISPLAY_TEXT", &results.join("\n"));
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
    let results = block(move || data.aws.handle(query)).await?;
    form_http_response(results.join("\n"))
}

pub async fn terminate(
    query: Query<TerminateRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query)).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_image(
    query: Query<DeleteImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query)).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_volume(
    query: Query<DeleteVolumeRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query)).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_snapshot(
    query: Query<DeleteSnapshotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query)).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_ecr_image(
    query: Query<DeleteEcrImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    block(move || data.aws.handle(query)).await?;
    form_http_response("finished".to_string())
}

pub async fn cleanup_ecr_images(
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    block(move || data.aws.handle(CleanupEcrImagesRequest {})).await?;
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
    let rows = text.split('\n').count() + 5;
    let body = format!(
        r#"
        <textarea name="message" id="script_editor_form" rows={rows} cols=100
        form="script_edit_form">{text}</textarea><br>
        <form id="script_edit_form">
        <input type="button" name="update" value="Update" onclick="submitFormData('{fname}')">
        <input type="button" name="cancel" value="Cancel" onclick="listResource('script')">
        </form>"#,
        text = text,
        fname = &query.filename,
        rows = rows,
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
    let mut amis = block(move || d.aws.get_all_ami_tags()).await?;
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
        .await?
        .into_iter()
        .map(|f| format!(r#"<option value="{f}">{f}</option>"#, f = f))
        .collect();

    let d = data.clone();
    let keys: Vec<_> = block(move || d.aws.ec2.get_all_key_pairs())
        .await?
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
    block(move || data.aws.ec2.request_spot_instance(&req)).await?;
    form_http_response("done".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct PriceRequest {
    pub search: Option<String>,
}

pub async fn get_prices(
    query: Query<PriceRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let d = data.clone();
    let inst_fam: Vec<_> = block(move || InstanceFamily::get_all(&d.aws.pool))
        .await?
        .into_iter()
        .map(|fam| {
            format!(
                r#"<option value="{n}.">{n} : {t}</option>"#,
                n = fam.family_name,
                t = fam.family_type,
            )
        })
        .collect();

    let prices = if let Some(search) = query.search {
        let d = data.clone();
        block(move || d.aws.get_ec2_prices(&[search]))
            .await
            ?
            .into_iter()
            .map(|price| {
                format!(
                    r#"
                    <tr style="text-align: center;">
                        <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                    </tr>
                    "#,
                    price.instance_type,
                    match price.ondemand_price {
                        Some(p) => format!("${:0.4}/hr", p),
                        None => "".to_string(),
                    },
                    match price.spot_price {
                        Some(p) => format!("${:0.4}/hr", p),
                        None => "".to_string(),
                    },
                    match price.reserved_price {
                        Some(p) => format!("${:0.4}/hr", p),
                        None => "".to_string(),
                    },
                    price.ncpu,
                    price.memory,
                    price.instance_family,
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    let body = if prices.is_empty() {
        format!(
            r#"
                <form action="javascript:listPrices()">
                <select id="inst_fam">{}</select><br>
                <input type="button" name="create_request" value="Request" onclick="listPrices();"/><br>
                </form><br>
            "#,
            inst_fam.join("\n"),
        )
    } else {
        format!(
            r#"<table border="1" class="dataframe"><thead>{}</thead><tbody>{}</tbody></table>"#,
            r#"
                <tr>
                <th>Instance Type</th>
                <th>Ondemand Price</th>
                <th>Spot Price</th>
                <th>Reserved Price</th>
                <th>N CPU</th>
                <th>Memory GiB</th>
                <th>Instance Family</th>
                </tr>
            "#,
            prices.join("\n")
        )
    };

    form_http_response(body)
}

pub async fn update(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    let entries = block(move || data.aws.update()).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows={} cols=100>{}</textarea>"#,
        entries.len() + 5,
        entries.join("\n"),
    );
    form_http_response(body)
}

pub async fn status(
    query: Query<StatusRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let entries = block(move || data.aws.handle(query)).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows={} cols=100>{}</textarea>"#,
        entries.len() + 5,
        entries.join("\n")
    );
    form_http_response(body)
}
