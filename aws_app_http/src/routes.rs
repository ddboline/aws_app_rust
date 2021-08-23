use anyhow::format_err;
use itertools::Itertools;
use maplit::hashmap;
use rweb::{get, post, Json, Query, Rejection, Schema};
use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, RwebResponse,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{path::Path, sync::Arc};
use tokio::{
    fs::{read_to_string, remove_file, File},
    io::AsyncWriteExt,
    task::spawn,
};

use aws_app_lib::{
    ec2_instance::SpotRequest,
    models::{InstanceFamily, InstanceList},
    novnc_instance::NoVncInstance,
    resource_type::ResourceType,
};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    ipv4addr_wrapper::Ipv4AddrWrapper,
    logged_user::LoggedUser,
    requests::{
        get_frontpage, CommandRequest, CreateImageRequest, CreateSnapshotRequest,
        DeleteEcrImageRequest, DeleteImageRequest, DeleteSnapshotRequest, DeleteVolumeRequest,
        ModifyVolumeRequest, StatusRequest, TagItemRequest, TerminateRequest,
    },
    IamAccessKeyWrapper, IamUserWrapper, ResourceTypeWrapper,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

#[derive(RwebResponse)]
#[response(description = "Main Page", content = "html")]
struct AwsIndexResponse(HtmlBase<String, Error>);

#[get("/aws/index.html")]
pub async fn sync_frontpage(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<AwsIndexResponse> {
    let results = get_frontpage(ResourceType::Instances, &data.aws).await?;
    let body =
        include_str!("../../templates/index.html").replace("DISPLAY_TEXT", &results.join("\n"));
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ResourceRequest {
    #[schema(description = "Resource Type")]
    resource: ResourceTypeWrapper,
}

#[derive(RwebResponse)]
#[response(description = "List Resources", content = "html")]
struct AwsListResponse(HtmlBase<String, Error>);

#[get("/aws/list")]
pub async fn list(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ResourceRequest>,
) -> WarpResult<AwsListResponse> {
    let query = query.into_inner();
    let results = get_frontpage(query.resource.into(), &data.aws).await?;
    Ok(HtmlBase::new(results.join("\n")).into())
}

#[derive(RwebResponse)]
#[response(description = "Finished", content = "html")]
struct FinishedResource(HtmlBase<&'static str, Error>);

#[get("/aws/terminate")]
pub async fn terminate(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<TerminateRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .terminate(&[query.instance])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(RwebResponse)]
#[response(description = "Image ID", content = "html")]
struct CreateImageResponse(HtmlBase<String, Error>);

#[get("/aws/create_image")]
pub async fn create_image(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateImageRequest>,
) -> WarpResult<CreateImageResponse> {
    let query = query.into_inner();
    let body: String = data
        .aws
        .create_image(&query.inst_id, &query.name)
        .await
        .map_err(Into::<Error>::into)?
        .map_or_else(|| "failed to create ami".into(), |ami_id| ami_id.into());
    Ok(HtmlBase::new(body).into())
}

#[get("/aws/delete_image")]
pub async fn delete_image(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteImageRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .delete_image(&query.ami)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/delete_volume")]
pub async fn delete_volume(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteVolumeRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .delete_ebs_volume(&query.volid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/modify_volume")]
pub async fn modify_volume(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ModifyVolumeRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .modify_ebs_volume(&query.volid, query.size)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/delete_snapshot")]
pub async fn delete_snapshot(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteSnapshotRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .delete_ebs_snapshot(&query.snapid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/create_snapshot")]
pub async fn create_snapshot(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateSnapshotRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    query.handle(&data.aws).await?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/tag_item")]
pub async fn tag_item(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<TagItemRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    query.handle(&data.aws).await?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/delete_ecr_image")]
pub async fn delete_ecr_image(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteEcrImageRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .ecr
        .delete_ecr_images(&query.reponame, &[query.imageid])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/cleanup_ecr_images")]
pub async fn cleanup_ecr_images(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<FinishedResource> {
    data.aws
        .ecr
        .cleanup_ecr_images()
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ScriptFilename {
    #[schema(description = "Script Filename")]
    pub filename: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Edit Script", content = "html")]
struct EditScriptResponse(HtmlBase<String, Error>);

#[get("/aws/edit_script")]
pub async fn edit_script(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ScriptFilename>,
) -> WarpResult<EditScriptResponse> {
    let query = query.into_inner();
    let filename = data.aws.config.script_directory.join(&query.filename);
    let text = if filename.exists() {
        read_to_string(&filename)
            .await
            .map_err(Into::<Error>::into)?
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
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ReplaceData {
    #[schema(description = "Script Filename")]
    pub filename: StackString,
    #[schema(description = "Script Text")]
    pub text: StackString,
}

#[post("/aws/replace_script")]
pub async fn replace_script(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    req: Json<ReplaceData>,
) -> WarpResult<FinishedResource> {
    let req = req.into_inner();
    let filename = data.aws.config.script_directory.join(&req.filename);
    let mut f = File::create(&filename).await.map_err(Into::<Error>::into)?;
    f.write_all(req.text.as_bytes())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[get("/aws/delete_script")]
pub async fn delete_script(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ScriptFilename>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    let filename = data.aws.config.script_directory.join(&query.filename);
    if filename.exists() {
        remove_file(&filename).await.map_err(Into::<Error>::into)?;
    }
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Debug, Schema)]
pub struct SpotBuilder {
    #[schema(description = "AMI ID")]
    pub ami: Option<StackString>,
    #[schema(description = "Instance Type")]
    pub inst: Option<StackString>,
    #[schema(description = "Script")]
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

#[derive(RwebResponse)]
#[response(description = "Spot Request", content = "html")]
struct BuildSpotResponse(HtmlBase<String, Error>);

#[get("/aws/build_spot_request")]
pub async fn build_spot_request(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<SpotBuilder>,
) -> WarpResult<BuildSpotResponse> {
    let query = query.into_inner();
    let mut amis: Vec<_> = data
        .aws
        .get_all_ami_tags()
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Arc::new)
        .collect();

    move_element_to_front(&mut amis, |ami| ami.name.contains("tmpfs"));

    if let Some(query_ami) = &query.ami {
        move_element_to_front(&mut amis, |ami| &ami.id == query_ami);
    }

    let amis = amis
        .into_iter()
        .map(|ami| format!(r#"<option value="{}">{}</option>"#, ami.id, ami.name,))
        .join("\n");

    let mut inst_fam: Vec<_> = InstanceFamily::get_all(&data.aws.pool)
        .await
        .map_err(Into::<Error>::into)?
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
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .join("\n");

    let mut files = data.aws.get_all_scripts().map_err(Into::<Error>::into)?;

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
        .await
        .map_err(Into::<Error>::into)?
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
    Ok(HtmlBase::new(body).into())
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Schema)]
pub struct SpotRequestData {
    #[schema(description = "Ami ID")]
    pub ami: StackString,
    #[schema(description = "Instance Type")]
    pub instance_type: StackString,
    #[schema(description = "Security Group")]
    pub security_group: StackString,
    #[schema(description = "Script Filename")]
    pub script: StackString,
    #[schema(description = "SSH Key Name")]
    pub key_name: StackString,
    #[schema(description = "Spot Price")]
    pub price: StackString,
    #[schema(description = "Spot Request Name Tag")]
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

#[post("/aws/request_spot")]
pub async fn request_spot(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    req: Json<SpotRequestData>,
) -> WarpResult<FinishedResource> {
    let req: SpotRequest = req.into_inner().into();
    let tags = Arc::new(req.tags.clone());
    for spot_id in data
        .aws
        .ec2
        .request_spot_instance(&req)
        .await
        .map_err(Into::<Error>::into)?
    {
        let ec2 = data.aws.ec2.clone();
        let tags = tags.clone();
        spawn(async move { ec2.tag_spot_instance(&spot_id, &tags, 1000).await });
    }
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CancelSpotRequest {
    #[schema(description = "Spot Request ID")]
    pub spot_id: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Cancelled Spot", content = "html")]
struct CancelledResponse(HtmlBase<String, Error>);

#[get("/aws/cancel_spot")]
pub async fn cancel_spot(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CancelSpotRequest>,
) -> WarpResult<CancelledResponse> {
    let query = query.into_inner();
    data.aws
        .ec2
        .cancel_spot_instance_request(&[query.spot_id.clone()])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!("cancelled {}", query.spot_id)).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct PriceRequest {
    #[schema(description = "Search String")]
    pub search: Option<StackString>,
}

#[derive(RwebResponse)]
#[response(description = "Prices", content = "html")]
struct PricesResponse(HtmlBase<String, Error>);

#[get("/aws/prices")]
pub async fn get_prices(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<PriceRequest>,
) -> WarpResult<PricesResponse> {
    let query = query.into_inner();
    let mut inst_fam = InstanceFamily::get_all(&data.aws.pool)
        .await
        .map_err(Into::<Error>::into)?;
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
            .await.map_err(Into::<Error>::into)
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

    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Update", content = "html")]
struct UpdateResponse(HtmlBase<String, Error>);

#[get("/aws/update")]
pub async fn update(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<UpdateResponse> {
    let entries: Vec<_> = data
        .aws
        .update()
        .await
        .map_err(Into::<Error>::into)?
        .collect();
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows={} cols=100>{}</textarea>"#,
        entries.len() + 5,
        entries.join("\n"),
    );
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Instance Status", content = "html")]
struct InstanceStatusResponse(HtmlBase<String, Error>);

#[get("/aws/instance_status")]
pub async fn instance_status(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<StatusRequest>,
) -> WarpResult<InstanceStatusResponse> {
    let query = query.into_inner();
    let entries = match tokio::time::timeout(
        tokio::time::Duration::from_secs(60),
        data.aws.get_status(&query.instance),
    )
    .await
    {
        Ok(x) => x,
        Err(_) => Err(format_err!("Timeout")),
    }
    .map_err(Into::<Error>::into)?;
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
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Run Command on Instance", content = "html")]
struct CommandResponse(HtmlBase<String, Error>);

#[post("/aws/command")]
pub async fn command(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    payload: Json<CommandRequest>,
) -> WarpResult<CommandResponse> {
    let payload = payload.into_inner();
    let entries = match tokio::time::timeout(
        tokio::time::Duration::from_secs(60),
        data.aws.run_command(&payload.instance, &payload.command),
    )
    .await
    {
        Ok(x) => x,
        Err(_) => Err(format_err!("Timeout")),
    }
    .map_err(Into::<Error>::into)?;

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
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct InstancesRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub inst: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Describe Instances", content = "html")]
struct InstancesResponse(HtmlBase<String, Error>);

#[get("/aws/instances")]
pub async fn get_instances(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<InstancesRequest>,
) -> WarpResult<InstancesResponse> {
    let query = query.into_inner();
    let instances = InstanceList::get_by_instance_family(&query.inst, &data.aws.pool)
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(|i| format!(r#"<option value="{i}">{i}</option>"#, i = i.instance_type,))
        .join("\n");
    Ok(HtmlBase::new(instances).into())
}

async fn novnc_status_response(
    novnc: &NoVncInstance,
    number: usize,
    domain: &str,
) -> Result<String, Error> {
    let pids = novnc.get_websock_pids().await?;
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

#[derive(RwebResponse)]
#[response(description = "Start NoVNC", content = "html")]
struct NovncStartResponse(HtmlBase<String, Error>);

#[get("/aws/novnc/start")]
pub async fn novnc_launcher(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStartResponse> {
    if let Some(novnc_path) = &data.aws.config.novnc_path {
        let certdir = Path::new("/etc/letsencrypt/live/").join(&data.aws.config.domain);
        let cert = certdir.join("fullchain.pem");
        let key = certdir.join("privkey.pem");
        data.novnc
            .novnc_start(novnc_path, &cert, &key)
            .await
            .map_err(Into::<Error>::into)?;
        let number = data.novnc.get_novnc_status().await;
        let body = novnc_status_response(&data.novnc, number, &data.aws.config.domain).await?;
        Ok(HtmlBase::new(body).into())
    } else {
        Ok(HtmlBase::new("NoVNC not configured".to_string()).into())
    }
}

#[derive(RwebResponse)]
#[response(description = "Stop NoVNC", content = "html")]
struct NovncStopResponse(HtmlBase<String, Error>);

#[get("/aws/novnc/stop")]
pub async fn novnc_shutdown(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStopResponse> {
    if data.aws.config.novnc_path.is_none() {
        return Ok(HtmlBase::new("NoVNC not configured".to_string()).into());
    }
    let output = data
        .novnc
        .novnc_stop_request()
        .await
        .map_err(Into::<Error>::into)?;
    let body = format!(
        "<textarea cols=100 rows=50>{}</textarea>",
        output.join("\n")
    );
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "NoVNC Status", content = "html")]
struct NovncStatusResponse(HtmlBase<String, Error>);

#[get("/aws/novnc/status")]
pub async fn novnc_status(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStatusResponse> {
    if data.aws.config.novnc_path.is_none() {
        return Ok(HtmlBase::new("NoVNC not configured".to_string()).into());
    }
    let number = data.novnc.get_novnc_status().await;
    let body = if number == 0 {
        r#"
            <input type="button" name="novnc" value="Start NoVNC" onclick="noVncTab('/aws/novnc/start')"/>
        "#.to_string()
    } else {
        novnc_status_response(&data.novnc, number, &data.aws.config.domain)
            .await
            .map_err(Into::<Error>::into)?
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Logged in User")]
struct UserResponse(JsonBase<LoggedUser, Error>);

#[get("/aws/user")]
pub async fn user(#[cookie = "jwt"] user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateUserRequest {
    #[schema(description = "User Name")]
    pub user_name: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Created Iam User", status = "CREATED")]
struct CreateUserResponse(JsonBase<IamUserWrapper, Error>);

#[get("/aws/create_user")]
pub async fn create_user(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateUserRequest>,
) -> WarpResult<CreateUserResponse> {
    let query = query.into_inner();
    let user = data
        .aws
        .create_user(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?
        .ok_or_else(|| Error::BadRequest("create user failed".into()))?;
    let resp = JsonBase::new(user.into());
    Ok(resp.into())
}

#[derive(RwebResponse)]
#[response(description = "Delete Iam User", content = "html")]
struct DeleteUserResponse(HtmlBase<String, Error>);

#[get("/aws/delete_user")]
pub async fn delete_user(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateUserRequest>,
) -> WarpResult<DeleteUserResponse> {
    let query = query.into_inner();
    data.aws
        .delete_user(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!("{} deleted", query.user_name)).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct AddUserToGroupRequest {
    #[schema(description = "User Name")]
    pub user_name: StackString,
    #[schema(description = "Group Name")]
    pub group_name: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Add User to Group", content = "html")]
struct AddUserGroupResponse(HtmlBase<String, Error>);

#[get("/aws/add_user_to_group")]
pub async fn add_user_to_group(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<AddUserGroupResponse> {
    let query = query.into_inner();
    data.aws
        .add_user_to_group(query.user_name.as_str(), query.group_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!("added {} to {}", query.user_name, query.group_name)).into())
}

#[derive(RwebResponse)]
#[response(description = "Remove User to Group", content = "html")]
struct RemoveUserGroupResponse(HtmlBase<String, Error>);

#[get("/aws/remove_user_from_group")]
pub async fn remove_user_from_group(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<RemoveUserGroupResponse> {
    let query = query.into_inner();
    data.aws
        .remove_user_from_group(query.user_name.as_str(), query.group_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!(
        "removed {} from {}",
        query.user_name, query.group_name
    ))
    .into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteAccesssKeyRequest {
    #[schema(description = "User Name")]
    pub user_name: StackString,
    #[schema(description = "Access Key ID")]
    pub access_key_id: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Create Access Key", status = "CREATED")]
struct CreateKeyResponse(JsonBase<IamAccessKeyWrapper, Error>);

#[get("/aws/create_access_key")]
pub async fn create_access_key(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateUserRequest>,
) -> WarpResult<CreateKeyResponse> {
    let query = query.into_inner();
    let access_key = data
        .aws
        .create_access_key(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(JsonBase::new(access_key.into()).into())
}

#[derive(RwebResponse)]
#[response(description = "Delete Access Key", content = "html")]
struct DeleteKeyResponse(HtmlBase<String, Error>);

#[get("/aws/delete_access_key")]
pub async fn delete_access_key(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteAccesssKeyRequest>,
) -> WarpResult<DeleteKeyResponse> {
    let query = query.into_inner();
    data.aws
        .delete_access_key(query.user_name.as_str(), query.access_key_id.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!(
        "delete {} for {}",
        query.access_key_id, query.user_name
    ))
    .into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct UpdateDnsNameRequest {
    #[schema(description = "Route53 Zone")]
    zone: StackString,
    #[schema(description = "DNS Name")]
    dns_name: StackString,
    #[schema(description = "Old IPv4 Address")]
    old_ip: Ipv4AddrWrapper,
    #[schema(description = "New IPv4 Address")]
    new_ip: Ipv4AddrWrapper,
}

#[derive(RwebResponse)]
#[response(description = "Update Dns", status = "CREATED", content = "html")]
struct UpdateDnsResponse(HtmlBase<String, Error>);

#[get("/aws/update_dns_name")]
pub async fn update_dns_name(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<UpdateDnsNameRequest>,
) -> WarpResult<UpdateDnsResponse> {
    let query = query.into_inner();
    data.aws
        .route53
        .update_dns_record(
            &query.zone,
            &query.dns_name,
            query.old_ip.into(),
            query.new_ip.into(),
        )
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format!(
        "update {} from {} to {}",
        query.dns_name, query.old_ip, query.new_ip
    ))
    .into())
}

#[derive(Serialize, Deserialize, Schema, Clone, Copy)]
enum SystemdActions {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "restart")]
    Restart,
}

impl SystemdActions {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[derive(Serialize, Deserialize, Schema)]
struct SystemdAction {
    #[schema(description = "SystemD Action")]
    action: SystemdActions,
    #[schema(description = "SystemD Service")]
    service: StackString,
}

#[derive(RwebResponse)]
#[response(
    description = "Systemd Action Output",
    status = "CREATED",
    content = "html"
)]
struct SystemdActionResponse(HtmlBase<String, Error>);

#[get("/aws/systemd_action")]
pub async fn systemd_action(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<SystemdAction>,
) -> WarpResult<SystemdActionResponse> {
    let query = query.into_inner();
    let output = data
        .aws
        .systemd
        .service_action(query.action.as_str(), &query.service)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(output.to_string()).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Restart All Systemd Services",
    status = "CREATED",
    content = "html"
)]
struct SystemdRestartAllResponse(HtmlBase<String, Error>);

#[get("/aws/systemd_restart_all")]
pub async fn systemd_restart_all(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<SystemdRestartAllResponse> {
    let mut output = Vec::new();
    let blacklist_service = &["nginx"];
    let aws_service = "aws-app-http".into();
    for service in &data.aws.config.systemd_services {
        if service == &aws_service || blacklist_service.contains(&service.as_str()) {
            continue;
        }
        output.push(
            data.aws
                .systemd
                .service_action("restart", service)
                .await
                .map_err(Into::<Error>::into)?,
        );
    }
    if data.aws.config.systemd_services.contains(&aws_service) {
        output.push(
            data.aws
                .systemd
                .service_action("restart", "aws-app-http")
                .await
                .map_err(Into::<Error>::into)?,
        );
    }
    Ok(HtmlBase::new(output.join("\n")).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Systemd Logs", content = "html")]
struct SystemdLogResponse(HtmlBase<String, Error>);

#[get("/aws/systemd_logs/{service}")]
pub async fn systemd_logs(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] data: AppState,
    service: StackString,
) -> WarpResult<SystemdLogResponse> {
    let entries = data
        .aws
        .systemd
        .get_service_logs(&service)
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(|log| log.to_string())
        .join("\n");
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="systemd_logs"
            rows=50 cols=100>{}</textarea>"#,
        entries
    );
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Crontab Logs", content = "html")]
struct CrontabLogResponse(HtmlBase<String, Error>);

#[get("/aws/crontab_logs/{crontab_type}")]
pub async fn crontab_logs(
    #[cookie = "jwt"] _: LoggedUser,
    crontab_type: StackString,
) -> WarpResult<CrontabLogResponse> {
    let crontab_path = if crontab_type == "user" {
        Path::new("/tmp/crontab.log")
    } else {
        Path::new("/tmp/crontab_root.log")
    };
    let body = if crontab_path.exists() {
        read_to_string(crontab_path)
            .await
            .map_err(Into::<Error>::into)?
    } else {
        String::new()
    };
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="systemd_logs"
            rows=50 cols=100>{}</textarea>"#,
        body
    );
    Ok(HtmlBase::new(body).into())
}
