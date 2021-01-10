use actix_web::{
    http::StatusCode,
    web::{Data, Json, Query},
    HttpResponse,
};
use itertools::Itertools;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::sync::Arc;
use tokio::{
    fs::{read_to_string, remove_file, File},
    io::AsyncWriteExt,
    task::spawn,
};

use aws_app_lib::{
    ec2_instance::SpotRequest,
    models::{InstanceFamily, InstanceList},
    resource_type::ResourceType,
};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{
        get_websock_pids, CleanupEcrImagesRequest, CommandRequest, CreateImageRequest,
        CreateSnapshotRequest, DeleteEcrImageRequest, DeleteImageRequest, DeleteSnapshotRequest,
        DeleteVolumeRequest, HandleRequest, ModifyVolumeRequest, NoVncStartRequest,
        NoVncStatusRequest, NoVncStopRequest, StatusRequest, TagItemRequest, TerminateRequest,
    },
};

pub type HttpResult = Result<HttpResponse, Error>;

fn form_http_response(body: String) -> HttpResult {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

fn to_json<T>(js: T) -> HttpResult
where
    T: Serialize,
{
    Ok(HttpResponse::Ok().json(js))
}

pub async fn sync_frontpage(_: LoggedUser, data: Data<AppState>) -> HttpResult {
    let results = data.aws.handle(ResourceType::Instances).await?;
    let body =
        include_str!("../../templates/index.html").replace("DISPLAY_TEXT", &results.join("\n"));
    form_http_response(body)
}

#[derive(Serialize, Deserialize)]
pub struct ResourceRequest {
    resource: ResourceType,
}

pub async fn list(
    query: Query<ResourceRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    let results = data.aws.handle(query.resource).await?;
    form_http_response(results.join("\n"))
}

pub async fn terminate(
    query: Query<TerminateRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn create_image(
    query: Query<CreateImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?.map_or_else(
        || form_http_response("failed to create ami".to_string()),
        |ami_id| form_http_response(ami_id.into()),
    )
}

pub async fn delete_image(
    query: Query<DeleteImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_volume(
    query: Query<DeleteVolumeRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn modify_volume(
    query: Query<ModifyVolumeRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_snapshot(
    query: Query<DeleteSnapshotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn create_snapshot(
    query: Query<CreateSnapshotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn tag_item(
    query: Query<TagItemRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn delete_ecr_image(
    query: Query<DeleteEcrImageRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws.handle(query).await?;
    form_http_response("finished".to_string())
}

pub async fn cleanup_ecr_images(_: LoggedUser, data: Data<AppState>) -> HttpResult {
    data.aws.handle(CleanupEcrImagesRequest {}).await?;
    form_http_response("finished".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct EditData {
    pub filename: StackString,
}

pub async fn edit_script(
    query: Query<EditData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    let filename = data.aws.config.script_directory.join(&query.filename);
    let text = if filename.exists() {
        read_to_string(&filename).await?
    } else {
        String::new()
    };
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
    pub filename: StackString,
    pub text: StackString,
}

pub async fn replace_script(
    req: Json<ReplaceData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let req = req.into_inner();
    let filename = data.aws.config.script_directory.join(&req.filename);
    let mut f = File::create(&filename).await?;
    f.write_all(req.text.as_bytes()).await?;
    form_http_response("done".to_string())
}

pub async fn delete_script(
    query: Query<EditData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    let filename = data.aws.config.script_directory.join(&query.filename);
    if filename.exists() {
        remove_file(&filename).await?;
    }
    form_http_response("done".to_string())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SpotBuilder {
    pub ami: Option<StackString>,
    pub inst: Option<StackString>,
    pub script: Option<StackString>,
}

fn move_element_to_front<T, F>(arr: &mut [T], filt: F)
where
    F: Fn(&T) -> bool,
{
    if let Some(idx) = arr
        .iter()
        .enumerate()
        .find_map(|(idx, item)| if filt(item) { Some(idx) } else { None })
    {
        for i in (0..idx).rev() {
            arr.swap(i + 1, i);
        }
    }
}

pub async fn build_spot_request(
    query: Query<SpotBuilder>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();

    let mut amis: Vec<_> = data
        .aws
        .get_all_ami_tags()
        .await?
        .into_iter()
        .map(Arc::new)
        .collect();

    if let Some(query_ami) = &query.ami {
        move_element_to_front(&mut amis, |ami| &ami.id == query_ami);
    }

    let amis = amis
        .into_iter()
        .map(|ami| format!(r#"<option value="{}">{}</option>"#, ami.id, ami.name,))
        .join("\n");

    let mut inst_fam: Vec<_> = InstanceFamily::get_all(&data.aws.pool)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect();

    if let Some(inst) = &query.inst {
        move_element_to_front(&mut inst_fam, |fam| inst.contains(fam.family_name.as_str()));
    } else {
        move_element_to_front(&mut inst_fam, |fam| fam.family_name == "t3");
    }

    let inst_fam = inst_fam
        .into_iter()
        .map(|fam| format!(r#"<option value="{n}">{n}</option>"#, n = fam.family_name,))
        .join("\n");

    let inst = query.inst.unwrap_or_else(|| "t3".into());
    let instances = InstanceList::get_by_instance_family(&inst, &data.aws.pool)
        .await?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .join("\n");

    let mut files = data.aws.get_all_scripts()?;

    if let Some(script) = &query.script {
        move_element_to_front(&mut files, |f| f == script);
    }

    let files = files
        .into_iter()
        .map(|f| format!(r#"<option value="{f}">{f}</option>"#, f = f))
        .join("\n");

    let keys = data
        .aws
        .ec2
        .get_all_key_pairs()
        .await?
        .map(|k| format!(r#"<option value="{k}">{k}</option>"#, k = k.0))
        .join("\n");

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
        ami = amis,
        inst_fam = inst_fam,
        inst = instances,
        sec = data
            .aws
            .config
            .spot_security_group
            .as_ref()
            .unwrap_or_else(|| data
                .aws
                .config
                .default_security_group
                .as_ref()
                .expect("NO DEFAULT_SECURITY_GROUP")),
        script = files,
        key = keys,
        price = data.aws.config.max_spot_price,
    );

    form_http_response(body)
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotRequestData {
    pub ami: StackString,
    pub instance_type: StackString,
    pub security_group: StackString,
    pub script: StackString,
    pub key_name: StackString,
    pub price: StackString,
    pub name: StackString,
}

impl From<SpotRequestData> for SpotRequest {
    fn from(item: SpotRequestData) -> Self {
        Self {
            ami: item.ami,
            instance_type: item.instance_type,
            security_group: item.security_group,
            script: item.script,
            key_name: item.key_name,
            price: item.price.parse().ok(),
            tags: hashmap! { "Name".into() => item.name },
        }
    }
}

pub async fn request_spot(
    req: Json<SpotRequestData>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let req: SpotRequest = req.into_inner().into();
    let tags = Arc::new(req.tags.clone());
    for spot_id in data.aws.ec2.request_spot_instance(&req).await? {
        let ec2 = data.aws.ec2.clone();
        let tags = tags.clone();
        spawn(async move { ec2.tag_spot_instance(&spot_id, &tags, 1000).await });
    }
    form_http_response("done".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct CancelSpotRequest {
    pub spot_id: StackString,
}

pub async fn cancel_spot(
    query: Query<CancelSpotRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    data.aws
        .ec2
        .cancel_spot_instance_request(&[query.spot_id])
        .await?;
    form_http_response("".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct PriceRequest {
    pub search: Option<StackString>,
}

pub async fn get_prices(
    query: Query<PriceRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let query = query.into_inner();
    let mut inst_fam = InstanceFamily::get_all(&data.aws.pool).await?;
    move_element_to_front(&mut inst_fam, |fam| fam.family_name == "m5");

    let inst_fam = inst_fam
        .into_iter()
        .map(|fam| {
            format!(
                r#"<option value="{n}.">{n} : {t}</option>"#,
                n = fam.family_name,
                t = fam.family_type,
            )
        })
        .join("\n");

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
                    if let Some(data_url) = price.data_url {
                        format!(r#"<a href="{}" target="_blank">{}</a>"#, data_url, price.instance_type)
                    } else {
                        price.instance_type.to_string()
                    },
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
            inst_fam,
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

pub async fn update(_: LoggedUser, data: Data<AppState>) -> HttpResult {
    let entries: Vec<_> = data.aws.update().await?.collect();
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
) -> HttpResult {
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
) -> HttpResult {
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
    pub inst: StackString,
}

pub async fn get_instances(
    query: Query<InstancesRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let instances = InstanceList::get_by_instance_family(&query.inst, &data.aws.pool)
        .await?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .join("\n");
    form_http_response(instances)
}

async fn novnc_status_response(number: usize, domain: &str) -> Result<String, Error> {
    let pids = get_websock_pids().await?;
    Ok(format!(
        r#"{} processes currenty running {:?}
            <br>
            <a href="https://{}:8787/vnc.html" target="_blank">Connect to NoVNC</a>
            <br>
            <input type="button" name="novnc" value="Stop NoVNC" onclick="noVncTab('/aws/novnc/stop')"/>
        "#,
        number, pids, &domain,
    ))
}

pub async fn novnc_launcher(_: LoggedUser, data: Data<AppState>) -> HttpResult {
    if data.aws.config.novnc_path.is_none() {
        return form_http_response("NoVNC not configured".to_string());
    }
    data.aws.handle(NoVncStartRequest {}).await?;

    let number = data.aws.handle(NoVncStatusRequest {}).await;
    let body = novnc_status_response(number, &data.aws.config.domain).await?;
    form_http_response(body)
}

pub async fn novnc_shutdown(_: LoggedUser, data: Data<AppState>) -> HttpResult {
    if data.aws.config.novnc_path.is_none() {
        return form_http_response("NoVNC not configured".to_string());
    }
    let output = data.aws.handle(NoVncStopRequest {}).await?;
    let body = format!(
        "<textarea cols=100 rows=50>{}</textarea>",
        output.join("\n")
    );
    form_http_response(body)
}

pub async fn novnc_status(_: LoggedUser, data: Data<AppState>) -> HttpResult {
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

pub async fn user(user: LoggedUser, _: Data<AppState>) -> HttpResult {
    to_json(user)
}

#[derive(Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub user_name: StackString,
}

pub async fn create_user(
    query: Query<CreateUserRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    data.aws
        .create_user(query.user_name.as_str())
        .await?
        .map_or_else(
            || form_http_response("create user failed".into()),
            |user| to_json(&user),
        )
}

pub async fn delete_user(
    query: Query<CreateUserRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    data.aws.delete_user(query.user_name.as_str()).await?;
    form_http_response(format!("{} deleted", query.user_name))
}

#[derive(Serialize, Deserialize)]
pub struct AddUserToGroupRequest {
    pub user_name: StackString,
    pub group_name: StackString,
}

pub async fn add_user_to_group(
    query: Query<AddUserToGroupRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    data.aws
        .add_user_to_group(query.user_name.as_str(), query.group_name.as_str())
        .await?;
    form_http_response("".into())
}

pub async fn remove_user_from_group(
    query: Query<AddUserToGroupRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    data.aws
        .remove_user_from_group(query.user_name.as_str(), query.group_name.as_str())
        .await?;
    form_http_response("".into())
}

#[derive(Serialize, Deserialize)]
pub struct DeleteAccesssKeyRequest {
    pub user_name: StackString,
    pub access_key_id: StackString,
}

pub async fn create_access_key(
    query: Query<CreateUserRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    let access_key = data.aws.create_access_key(query.user_name.as_str()).await?;
    to_json(&access_key)
}

pub async fn delete_access_key(
    query: Query<DeleteAccesssKeyRequest>,
    _: LoggedUser,
    data: Data<AppState>,
) -> HttpResult {
    data.aws
        .delete_access_key(query.user_name.as_str(), query.access_key_id.as_str())
        .await?;
    form_http_response("".into())
}
