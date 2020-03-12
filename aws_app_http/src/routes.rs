use actix_web::{
    http::StatusCode,
    web::{Data, Json, Query},
    HttpResponse,
};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::{
    fs::{remove_file, File},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use aws_app_lib::{
    config::Config,
    ec2_instance::SpotRequest,
    models::{InstanceFamily, InstanceList},
    resource_type::ResourceType,
};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{
        get_websock_pids, CleanupEcrImagesRequest, CommandRequest, DeleteEcrImageRequest,
        DeleteImageRequest, DeleteSnapshotRequest, DeleteVolumeRequest, HandleRequest,
        NoVncStartRequest, NoVncStatusRequest, NoVncStopRequest, StatusRequest, TerminateRequest,
    },
};

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

pub async fn sync_frontpage(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    let results = data.aws.handle(ResourceType::Instances).await?;
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
    let results = data.aws.handle(query).await?;
    form_http_response(results.join("\n"))
}

pub async fn terminate(
    query: Query<TerminateRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_image(
    query: Query<DeleteImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_volume(
    query: Query<DeleteVolumeRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_snapshot(
    query: Query<DeleteSnapshotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_ecr_image(
    query: Query<DeleteEcrImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn cleanup_ecr_images(
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    data.aws.handle(CleanupEcrImagesRequest {}).await?;
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
        <input type="button" name="Request" value="Request" onclick="updateScriptAndBuildSpotRequest('{fname}')">
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

#[derive(Serialize, Deserialize, Debug)]
pub struct SpotBuilder {
    pub ami: Option<String>,
    pub inst: Option<String>,
    pub script: Option<String>,
}

pub async fn build_spot_request(
    query: Query<SpotBuilder>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();

    let mut amis: Vec<_> = data
        .aws
        .get_all_ami_tags()
        .await?
        .into_iter()
        .map(Arc::new)
        .collect();

    let ami_opt = if let Some(ami_) = &query.ami {
        let mut ami_opt: Vec<_> = amis.iter().filter(|ami| &ami.id == ami_).cloned().collect();
        amis.retain(|ami| &ami.id != ami_);
        ami_opt.extend_from_slice(&amis);
        ami_opt
    } else {
        amis
    };

    let amis: Vec<_> = ami_opt
        .into_iter()
        .map(|ami| format!(r#"<option value="{}">{}</option>"#, ami.id, ami.name,))
        .collect();

    let mut inst_fam: Vec<_> = InstanceFamily::get_all(&data.aws.pool)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect();

    let inst_opt = if let Some(inst) = &query.ami {
        let mut inst_opt: Vec<_> = inst_fam
            .iter()
            .filter(|fam| &fam.family_name == inst)
            .cloned()
            .collect();
        inst_fam.retain(|fam| &fam.family_name != inst);
        inst_opt.extend_from_slice(&inst_fam);
        inst_opt
    } else {
        inst_fam
    };

    let inst_fam: Vec<_> = inst_opt
        .into_iter()
        .map(|fam| format!(r#"<option value="{n}">{n}</option>"#, n = fam.family_name,))
        .collect();

    let inst = query.inst.unwrap_or_else(|| "t3".to_string());
    let instances: Vec<_> = InstanceList::get_by_instance_family(&inst, &data.aws.pool)
        .await?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .collect();

    let mut files = data.aws.get_all_scripts()?;

    let file_opts = if let Some(script) = &query.script {
        let mut file_opt: Vec<_> = files.iter().filter(|f| f == &script).cloned().collect();
        files.retain(|f| f != script);
        file_opt.extend_from_slice(&files);
        file_opt
    } else {
        files
    };
    let files: Vec<_> = file_opts
        .into_iter()
        .map(|f| format!(r#"<option value="{f}">{f}</option>"#, f = f))
        .collect();

    let keys: Vec<_> = data
        .aws
        .ec2
        .get_all_key_pairs()
        .await?
        .into_iter()
        .map(|k| format!(r#"<option value="{k}">{k}</option>"#, k = k.0))
        .collect();

    let body = format!(
        r#"
            <form action="javascript:createScript()">
            Ami: <select id="ami">{ami}</select><br>
            Instance family: <select id="inst_fam" onchange="instanceOptions()">{inst_fam}</select><br>
            Instance type: <select id="instance_type">{inst}</select><br>
            Security group: <input type="text" name="security_group" id="security_group" 
                value="{sec}"/><br>
            Script: <select id="script">{script}</select><br>
            Key: <select id="key">{key}</select><br>
            Price: <input type="text" name="price" id="price" value="{price}"/><br>
            Name: <input type="text" name="name" id="name"/><br>
            <input type="button" name="create_request" value="Request"
                onclick="requestSpotInstance();"/><br>
            </form>
        "#,
        ami = amis.join("\n"),
        inst_fam = inst_fam.join("\n"),
        inst = instances.join("\n"),
        sec = data.aws.config.spot_security_group,
        script = files.join("\n"),
        key = keys.join("\n"),
        price = data.aws.config.max_spot_price,
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
    data.aws.ec2.request_spot_instance(&req).await?;
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
    let inst_fam: Vec<_> = InstanceFamily::get_all(&data.aws.pool)
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
        data.aws.get_ec2_prices(&[search])
            .await
            ?
            .into_iter()
            .map(|price| {
                format!(
                    r#"
                    <tr style="text-align: center;">
                        <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                        <td>{}</td>
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
                    format!(
                        r#"<input type="button" name="Request" value="Request" onclick="buildSpotRequest(null, '{}', null)">"#,
                        price.instance_type,
                    ),
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
                <select id="inst_fam" onchange="listPrices();">{}</select><br>
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
    let entries = data.aws.update().await?;
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
    let entries = data.aws.get_status(&query.instance).await?;
    let body = format!(
        r#"{}<br><textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows={} cols=100>{}</textarea>"#,
        format!(
            r#"
            <form action="javascript:runCommand('{host}')">
            <input type="text" name="command_text" id="command_text"/>
            <input type="button" name="run_command" value="Run" onclick="runCommand('{host}');"/>
            </form>
        "#,
            host = query.instance
        ),
        entries.len() + 5,
        entries.join("\n")
    );
    form_http_response(body)
}

pub async fn command(
    payload: Json<CommandRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let payload = payload.into_inner();
    let entries = data
        .aws
        .run_command(&payload.instance, &payload.command)
        .await?;
    let body = format!(
        r#"{}<br><textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows={} cols=100>{}</textarea>"#,
        format!(
            r#"
                <form action="javascript:runCommand('{host}')">
                <input type="text" name="command_text" id="command_text"/>
                <input type="button" name="run_command" value="Run" onclick="runCommand('{host}');"/>
                </form>
            "#,
            host = payload.instance
        ),
        entries.len() + 5,
        entries.join("\n")
    );
    form_http_response(body)
}

#[derive(Serialize, Deserialize)]
pub struct InstancesRequest {
    pub inst: String,
}

pub async fn get_instances(
    query: Query<InstancesRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let instances: Vec<_> = InstanceList::get_by_instance_family(&query.inst, &data.aws.pool)
        .await?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .collect();
    form_http_response(instances.join("\n"))
}

async fn novnc_status_response(number: usize, domain: &str) -> Result<String, Error> {
    let pids = get_websock_pids().await?;
    Ok(format!(
        r#"{} processes currenty running {:?}
            <br>
            <a href="https://{}:8787/vnc.html">Connect to NoVNC</a>
            <br>
            <input type="button" name="novnc" value="Stop NoVNC" onclick="noVncTab('/aws/novnc/stop')"/>
        "#,
        number, pids, &domain,
    ))
}

pub async fn novnc_launcher(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    if data.aws.config.novnc_path.is_none() {
        return form_http_response("NoVNC not configured".to_string());
    }
    data.aws.handle(NoVncStartRequest {}).await?;

    let number = data.aws.handle(NoVncStatusRequest {}).await;
    let body = novnc_status_response(number, &data.aws.config.domain).await?;
    form_http_response(body)
}

pub async fn novnc_shutdown(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    if data.aws.config.novnc_path.is_none() {
        return form_http_response("NoVNC not configured".to_string());
    }
    data.aws.handle(NoVncStopRequest {}).await?;
    form_http_response("novnc stopped".to_string())
}

pub async fn novnc_status(_: LoggedUser, data: Data<AppState>) -> Result<HttpResponse, Error> {
    if data.aws.config.novnc_path.is_none() {
        return form_http_response("NoVNC not configured".to_string());
    }
    let number = data.aws.handle(NoVncStatusRequest {}).await;
    let body = if number == 0 {
        r#"
            <input type="button" name="novnc" value="Start NoVNC" onclick="noVncTab('/aws/novnc/start')"/>
        "#.to_string()
    } else {
        novnc_status_response(number, &data.aws.config.domain).await?
    };
    form_http_response(body)
}
