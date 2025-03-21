use futures::TryStreamExt;
use maplit::hashmap;
use rweb::{delete, get, patch, post, Json, Query, Rejection, Schema};
use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, RwebResponse,
    UuidWrapper,
};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    fs::{read_to_string, remove_file, File},
    io::AsyncWriteExt,
    task::spawn,
    time::{sleep, Duration},
};

use aws_app_lib::{
    ec2_instance::{AmiInfo, InstanceRequest, SpotRequest},
    errors::AwslibError,
    inbound_email::InboundEmail,
    models::{InboundEmailDB, InstanceFamily, InstanceList},
    s3_instance::S3Instance,
};

use super::{
    app::AppState,
    elements::{
        build_spot_request_body, edit_script_body, get_frontpage, get_index, inbound_email_body,
        instance_family_body, instance_status_body, instance_types_body, novnc_start_body,
        novnc_status_body, prices_body, textarea_body, textarea_fixed_size_body,
    },
    errors::ServiceError as Error,
    ipv4addr_wrapper::Ipv4AddrWrapper,
    logged_user::LoggedUser,
    IamAccessKeyWrapper, IamUserWrapper, ResourceTypeWrapper,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

#[derive(RwebResponse)]
#[response(description = "Main Page", content = "html")]
struct AwsIndexResponse(HtmlBase<StackString, Error>);

#[get("/aws/index.html")]
#[openapi(description = "AWS App Main Page")]
pub async fn sync_frontpage(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<AwsIndexResponse> {
    let body = get_index(&data.aws).await?;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ResourceRequest {
    #[schema(description = "Resource Type")]
    resource: ResourceTypeWrapper,
}

#[derive(RwebResponse)]
#[response(description = "List Resources", content = "html")]
struct AwsListResponse(HtmlBase<StackString, Error>);

#[get("/aws/list")]
#[openapi(description = "List AWS Resources")]
pub async fn list(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ResourceRequest>,
) -> WarpResult<AwsListResponse> {
    let query = query.into_inner();
    let body = get_frontpage(query.resource.into(), &data.aws).await?;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct TerminateRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Deleted", content = "html", status = "NO_CONTENT")]
struct DeletedResource(HtmlBase<&'static str, Error>);

#[delete("/aws/terminate")]
#[openapi(description = "Terminate Ec2 Instance")]
pub async fn terminate(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<TerminateRequest>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    data.aws
        .terminate(&[query.instance])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateImageRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub inst_id: StackString,
    #[schema(description = "Ami Name")]
    pub name: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Image ID", content = "html", status = "CREATED")]
struct CreateImageResponse(HtmlBase<String, Error>);

#[post("/aws/create_image")]
#[openapi(description = "Create EC2 AMI Image")]
pub async fn create_image(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateImageRequest>,
) -> WarpResult<CreateImageResponse> {
    let query = query.into_inner();
    let body: String = data
        .aws
        .create_image(query.inst_id, query.name)
        .await
        .map_err(Into::<Error>::into)?
        .map_or_else(|| "failed to create ami".into(), Into::into);
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteImageRequest {
    #[schema(description = "Ami ID")]
    pub ami: StackString,
}

#[delete("/aws/delete_image")]
#[openapi(description = "Delete EC2 AMI Image")]
pub async fn delete_image(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteImageRequest>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    data.aws
        .delete_image(&query.ami)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
}

#[delete("/aws/delete_volume")]
#[openapi(description = "Delete EC2 Volume")]
pub async fn delete_volume(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteVolumeRequest>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    data.aws
        .delete_ebs_volume(&query.volid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ModifyVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Volume Size GiB")]
    pub size: i32,
}

#[derive(RwebResponse)]
#[response(description = "Finished", content = "html", status = "CREATED")]
struct FinishedResource(HtmlBase<&'static str, Error>);

#[patch("/aws/modify_volume")]
#[openapi(description = "Modify EC2 Volume")]
pub async fn modify_volume(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteSnapshotRequest {
    #[schema(description = "Snapshot ID")]
    pub snapid: StackString,
}

#[delete("/aws/delete_snapshot")]
#[openapi(description = "Delete EC2 Snapshot")]
pub async fn delete_snapshot(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteSnapshotRequest>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    data.aws
        .delete_ebs_snapshot(&query.snapid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateSnapshotRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Snapshot Name")]
    pub name: Option<StackString>,
}

#[post("/aws/create_snapshot")]
#[openapi(description = "Create EC2 Snapshot")]
pub async fn create_snapshot(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateSnapshotRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();

    let tags = if let Some(name) = &query.name {
        hashmap! {"Name".into() => name.clone()}
    } else {
        HashMap::default()
    };
    data.aws
        .create_ebs_snapshot(query.volid.as_str(), &tags)
        .await
        .map_err(Into::<Error>::into)?;

    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct TagItemRequest {
    #[schema(description = "Resource ID")]
    pub id: StackString,
    #[schema(description = "Tag")]
    pub tag: StackString,
}

#[patch("/aws/tag_item")]
#[openapi(description = "Tag EC2 Resource")]
pub async fn tag_item(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<TagItemRequest>,
) -> WarpResult<FinishedResource> {
    let query = query.into_inner();
    data.aws
        .ec2
        .tag_aws_resource(
            query.id.as_str(),
            &hashmap! {
                "Name".into() => query.tag,
            },
        )
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteEcrImageRequest {
    #[schema(description = "ECR Repository Name")]
    pub reponame: StackString,
    #[schema(description = "Container Image ID")]
    pub imageid: StackString,
}

#[delete("/aws/delete_ecr_image")]
#[openapi(description = "Delete ECR Image")]
pub async fn delete_ecr_image(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteEcrImageRequest>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    data.aws
        .ecr
        .delete_ecr_images(&query.reponame, &[query.imageid])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[delete("/aws/cleanup_ecr_images")]
#[openapi(description = "Cleanup ECR Images")]
pub async fn cleanup_ecr_images(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<DeletedResource> {
    data.aws
        .ecr
        .cleanup_ecr_images()
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ScriptFilename {
    #[schema(description = "Script Filename")]
    pub filename: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Edit Script", content = "html")]
struct EditScriptResponse(HtmlBase<StackString, Error>);

#[get("/aws/edit_script")]
#[openapi(description = "Edit Script")]
pub async fn edit_script(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ScriptFilename>,
) -> WarpResult<EditScriptResponse> {
    let query = query.into_inner();
    let fname = &query.filename;
    let filename = data.aws.config.script_directory.join(fname);
    let text = if filename.exists() {
        read_to_string(&filename)
            .await
            .map_err(Into::<Error>::into)?
    } else {
        String::new()
    };
    let body = edit_script_body(fname.clone(), text.into())?.into();
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
#[openapi(description = "Replace Script")]
pub async fn replace_script(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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

#[delete("/aws/delete_script")]
#[openapi(description = "Delete Script")]
pub async fn delete_script(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<ScriptFilename>,
) -> WarpResult<DeletedResource> {
    let query = query.into_inner();
    let filename = data.aws.config.script_directory.join(&query.filename);
    if filename.exists() {
        remove_file(&filename).await.map_err(Into::<Error>::into)?;
    }
    Ok(HtmlBase::new("Deleted").into())
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
#[response(description = "Spot Request", content = "html", status = "CREATED")]
struct BuildSpotResponse(HtmlBase<StackString, Error>);

#[post("/aws/build_spot_request")]
#[openapi(description = "Build Spot Request")]
pub async fn build_spot_request(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<SpotBuilder>,
) -> WarpResult<BuildSpotResponse> {
    let query = query.into_inner();
    let mut amis: Vec<AmiInfo> = Box::pin(data.aws.get_all_ami_tags())
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .collect();

    move_element_to_front(&mut amis, |ami| ami.name.contains("tmpfs"));

    if let Some(query_ami) = &query.ami {
        move_element_to_front(&mut amis, |ami| &ami.id == query_ami);
    }

    let mut inst_fams: Vec<InstanceFamily> = InstanceFamily::get_all(&data.aws.pool, Some(true))
        .await
        .map_err(Into::<Error>::into)?
        .and_then(|fam| async move { Ok(fam) })
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?;

    if let Some(inst) = &query.inst {
        move_element_to_front(&mut inst_fams, |fam| {
            inst.contains(fam.family_name.as_str())
        });
    } else {
        move_element_to_front(&mut inst_fams, |fam| fam.family_name == "t3");
    }

    let inst = query.inst.unwrap_or_else(|| "t3".into());
    let instances: Vec<InstanceList> = InstanceList::get_by_instance_family(&inst, &data.aws.pool)
        .await
        .map_err(Into::<Error>::into)?
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?;

    let mut files = data.aws.get_all_scripts();

    if let Some(script) = &query.script {
        move_element_to_front(&mut files, |f| f == script);
    }

    let keys: Vec<_> = data
        .aws
        .ec2
        .get_all_key_pairs()
        .await
        .map_err(Into::<Error>::into)?
        .collect();

    let body = build_spot_request_body(
        amis,
        inst_fams,
        instances,
        files,
        keys,
        data.aws.config.clone(),
    )?
    .into();

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
            script: item.script.as_str().into(),
            key_name: item.key_name,
            price: item.price.parse().ok(),
            tags: hashmap! { "Name".into() => item.name },
        }
    }
}

impl From<SpotRequestData> for InstanceRequest {
    fn from(value: SpotRequestData) -> Self {
        Self {
            ami: value.ami,
            instance_type: value.instance_type,
            security_group: value.security_group,
            script: value.script.as_str().into(),
            key_name: value.key_name,
            tags: hashmap! { "Name".into() => value.name },
        }
    }
}

#[post("/aws/request_spot")]
pub async fn request_spot(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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

#[post("/aws/run_instance")]
pub async fn run_instance(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    req: Json<SpotRequestData>,
) -> WarpResult<FinishedResource> {
    let req: InstanceRequest = req.into_inner().into();
    data.aws
        .ec2
        .run_ec2_instance(&req)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CancelSpotRequest {
    #[schema(description = "Spot Request ID")]
    pub spot_id: StackString,
}

#[derive(RwebResponse)]
#[response(
    description = "Cancelled Spot",
    content = "html",
    status = "NO_CONTENT"
)]
struct CancelledResponse(HtmlBase<StackString, Error>);

#[delete("/aws/cancel_spot")]
#[openapi(description = "Cancel Spot Request")]
pub async fn cancel_spot(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CancelSpotRequest>,
) -> WarpResult<CancelledResponse> {
    let query = query.into_inner();
    data.aws
        .ec2
        .cancel_spot_instance_request(&[query.spot_id.clone()])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!("cancelled {}", query.spot_id)).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct PriceRequest {
    #[schema(description = "Search String")]
    pub search: Option<StackString>,
}

#[derive(RwebResponse)]
#[response(description = "Prices", content = "html")]
struct PricesResponse(HtmlBase<StackString, Error>);

#[get("/aws/prices")]
#[openapi(description = "Get Ec2 Prices")]
pub async fn get_prices(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<PriceRequest>,
) -> WarpResult<PricesResponse> {
    let query = query.into_inner();

    let body = if let Some(search) = query.search {
        let prices = data
            .aws
            .get_ec2_prices(&[search])
            .await
            .map_err(Into::<Error>::into)?;
        prices_body(prices)?.into()
    } else {
        let mut inst_fam: Vec<InstanceFamily> = InstanceFamily::get_all(&data.aws.pool, None)
            .await
            .map_err(Into::<Error>::into)?
            .try_collect()
            .await
            .map_err(Into::<Error>::into)?;
        move_element_to_front(&mut inst_fam, |fam| fam.family_name == "m5");
        instance_family_body(inst_fam)?.into()
    };

    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Update", content = "html", status = "CREATED")]
struct UpdateResponse(HtmlBase<StackString, Error>);

#[post("/aws/update")]
#[openapi(description = "Update Data")]
pub async fn update(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<UpdateResponse> {
    let entries: Vec<StackString> = data
        .aws
        .update()
        .await
        .map_err(Into::<Error>::into)?
        .collect();
    let body = textarea_body(entries, "diary_editor_form".into())?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct StatusRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Instance Status", content = "html")]
struct InstanceStatusResponse(HtmlBase<StackString, Error>);

#[get("/aws/instance_status")]
#[openapi(description = "Get Ec2 Instance Status")]
pub async fn instance_status(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
        Err(_) => Err(AwslibError::StaticCustomError("Timeout")),
    }
    .map_err(Into::<Error>::into)?;
    let body = instance_status_body(entries, query.instance)?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Run Command on Instance",
    content = "html",
    status = "CREATED"
)]
struct CommandResponse(HtmlBase<StackString, Error>);

#[derive(Serialize, Deserialize, Debug, Schema)]
pub struct CommandRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
    #[schema(description = "Command String")]
    pub command: StackString,
}

#[post("/aws/command")]
#[openapi(description = "Run command on Ec2 Instance")]
pub async fn command(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
        Err(_) => Err(AwslibError::StaticCustomError("Timeout")),
    }
    .map_err(Into::<Error>::into)?;

    let body = instance_status_body(entries, payload.instance)?.into();
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
#[openapi(description = "List Ec2 Instances")]
pub async fn get_instances(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<InstancesRequest>,
) -> WarpResult<InstancesResponse> {
    let query = query.into_inner();
    let instances: Vec<InstanceList> =
        InstanceList::get_by_instance_family(&query.inst, &data.aws.pool)
            .await
            .map_err(Into::<Error>::into)?
            .try_collect()
            .await
            .map_err(Into::<Error>::into)?;
    let body = instance_types_body(instances)?;
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Start NoVNC", content = "html", status = "CREATED")]
struct NovncStartResponse(HtmlBase<StackString, Error>);

#[post("/aws/novnc/start")]
#[openapi(description = "Start NoVNC Service")]
pub async fn novnc_launcher(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStartResponse> {
    if let Some(novnc_path) = &data.aws.config.novnc_path {
        let home_dir =
            dirs::home_dir().ok_or_else(|| Error::BadRequest("No home directory".into()))?;
        let certdir = home_dir.join(".vnc");
        let cert = certdir.join("fullchain.pem");
        let key = certdir.join("privkey.pem");
        let cert = data.aws.config.novnc_cert_path.as_ref().unwrap_or(&cert);
        let key = data.aws.config.novnc_key_path.as_ref().unwrap_or(&key);
        data.novnc
            .novnc_start(novnc_path, cert, key)
            .await
            .map_err(Into::<Error>::into)?;
        let number = data.novnc.get_novnc_status().await;
        let pids = data
            .novnc
            .get_websock_pids()
            .await
            .map_err(Into::<Error>::into)?;
        let body = novnc_status_body(number, data.aws.config.domain.clone(), pids)?.into();
        Ok(HtmlBase::new(body).into())
    } else {
        Ok(HtmlBase::new("NoVNC not configured".into()).into())
    }
}

#[derive(RwebResponse)]
#[response(description = "Stop NoVNC", content = "html", status = "CREATED")]
struct NovncStopResponse(HtmlBase<StackString, Error>);

#[post("/aws/novnc/stop")]
#[openapi(description = "Stop NoVNC Service")]
pub async fn novnc_shutdown(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStopResponse> {
    if data.aws.config.novnc_path.is_none() {
        return Ok(HtmlBase::new("NoVNC not configured".into()).into());
    }
    let output = data
        .novnc
        .novnc_stop_request()
        .await
        .map_err(Into::<Error>::into)?;
    let body = textarea_body(output, "novnc-stop".into())?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "NoVNC Status", content = "html")]
struct NovncStatusResponse(HtmlBase<StackString, Error>);

#[get("/aws/novnc/status")]
#[openapi(description = "NoVNC Service Status")]
pub async fn novnc_status(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<NovncStatusResponse> {
    if data.aws.config.novnc_path.is_none() {
        return Ok(HtmlBase::new("NoVNC not configured".into()).into());
    }
    let number = data.novnc.get_novnc_status().await;
    let body = if number == 0 {
        novnc_start_body()?.into()
    } else {
        let pids = data
            .novnc
            .get_websock_pids()
            .await
            .map_err(Into::<Error>::into)?;
        novnc_status_body(number, data.aws.config.domain.clone(), pids)?.into()
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Logged in User")]
struct UserResponse(JsonBase<LoggedUser, Error>);

#[get("/aws/user")]
#[openapi(description = "User Object if logged in")]
pub async fn user(#[filter = "LoggedUser::filter"] user: LoggedUser) -> WarpResult<UserResponse> {
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

#[post("/aws/create_user")]
#[openapi(description = "Create IAM User")]
pub async fn create_user(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
#[response(
    description = "Delete Iam User",
    content = "html",
    status = "NO_CONTENT"
)]
struct DeleteUserResponse(HtmlBase<StackString, Error>);

#[delete("/aws/delete_user")]
#[openapi(description = "Delete IAM User")]
pub async fn delete_user(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateUserRequest>,
) -> WarpResult<DeleteUserResponse> {
    let query = query.into_inner();
    data.aws
        .delete_user(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!("{} deleted", query.user_name)).into())
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
struct AddUserGroupResponse(HtmlBase<StackString, Error>);

#[patch("/aws/add_user_to_group")]
#[openapi(description = "Add IAM User to Group")]
pub async fn add_user_to_group(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<AddUserGroupResponse> {
    let query = query.into_inner();
    data.aws
        .add_user_to_group(query.user_name.as_str(), query.group_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!(
        "added {} to {}",
        query.user_name,
        query.group_name
    ))
    .into())
}

#[derive(RwebResponse)]
#[response(
    description = "Remove User to Group",
    content = "html",
    status = "NO_CONTENT"
)]
struct RemoveUserGroupResponse(HtmlBase<StackString, Error>);

#[delete("/aws/remove_user_from_group")]
#[openapi(description = "Remove IAM User from Group")]
pub async fn remove_user_from_group(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<RemoveUserGroupResponse> {
    let query = query.into_inner();
    data.aws
        .remove_user_from_group(query.user_name.as_str(), query.group_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!(
        "removed {} from {}",
        query.user_name,
        query.group_name
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
struct CreateKeyResponse(JsonBase<Option<IamAccessKeyWrapper>, Error>);

#[post("/aws/create_access_key")]
#[openapi(description = "Create Access Key for IAM User")]
pub async fn create_access_key(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<CreateUserRequest>,
) -> WarpResult<CreateKeyResponse> {
    let query = query.into_inner();
    let access_key = data
        .aws
        .create_access_key(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(JsonBase::new(access_key.map(Into::into)).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Delete Access Key",
    content = "html",
    status = "NO_CONTENT"
)]
struct DeleteKeyResponse(HtmlBase<StackString, Error>);

#[delete("/aws/delete_access_key")]
#[openapi(description = "Delete Access Key for IAM User")]
pub async fn delete_access_key(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    query: Query<DeleteAccesssKeyRequest>,
) -> WarpResult<DeleteKeyResponse> {
    let query = query.into_inner();
    data.aws
        .delete_access_key(query.user_name.as_str(), query.access_key_id.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!(
        "delete {} for {}",
        query.access_key_id,
        query.user_name
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
struct UpdateDnsResponse(HtmlBase<StackString, Error>);

#[patch("/aws/update_dns_name")]
#[openapi(description = "Update DNS Name")]
pub async fn update_dns_name(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
    Ok(HtmlBase::new(format_sstr!(
        "update {} from {} to {}",
        query.dns_name,
        query.old_ip,
        query.new_ip
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
struct SystemdActionResponse(HtmlBase<StackString, Error>);

#[post("/aws/systemd_action")]
#[openapi(description = "Perform Systemd Action")]
pub async fn systemd_action(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
    Ok(HtmlBase::new(output).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Restart All Systemd Services",
    status = "CREATED",
    content = "html"
)]
struct SystemdRestartAllResponse(HtmlBase<String, Error>);

#[post("/aws/systemd_restart_all")]
#[openapi(description = "Restart all Systemd Services")]
pub async fn systemd_restart_all(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
        spawn(async move {
            sleep(Duration::from_secs(1)).await;
            data.aws
                .systemd
                .service_action("restart", "aws-app-http")
                .await
        });
    }
    Ok(HtmlBase::new(output.join("\n")).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Systemd Logs", content = "html")]
struct SystemdLogResponse(HtmlBase<StackString, Error>);

#[get("/aws/systemd_logs/{service}")]
#[openapi(description = "Get Systemd Logs for Service")]
pub async fn systemd_logs(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    service: StackString,
) -> WarpResult<SystemdLogResponse> {
    let entries: Vec<StackString> = data
        .aws
        .systemd
        .get_service_logs(&service)
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(|log| log.to_string().into())
        .collect();
    let body = textarea_body(entries, "systemd-logs".into())?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Crontab Logs", content = "html")]
struct CrontabLogResponse(HtmlBase<StackString, Error>);

#[get("/aws/crontab_logs/{crontab_type}")]
#[openapi(description = "Get Crontab Logs")]
pub async fn crontab_logs(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    crontab_type: StackString,
    #[data] data: AppState,
) -> WarpResult<CrontabLogResponse> {
    let crontab_path = if crontab_type == "user" {
        &data.aws.config.user_crontab
    } else {
        &data.aws.config.root_crontab
    };
    let body = if crontab_path.exists() {
        textarea_fixed_size_body(
            read_to_string(crontab_path)
                .await
                .map_err(Into::<Error>::into)?
                .into(),
            "systemd_logs".into(),
        )?
        .into()
    } else {
        StackString::new()
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Inbound Email Detail", content = "html")]
struct InboundEmailDetailResponse(HtmlBase<String, Error>);

#[get("/aws/inbound-email/{id}")]
pub async fn inbound_email_detail(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    id: UuidWrapper,
) -> WarpResult<InboundEmailDetailResponse> {
    let body = if let Some(email) = InboundEmailDB::get_by_id(&data.aws.pool, id.into())
        .await
        .map_err(Into::<Error>::into)?
    {
        inbound_email_body(email.text_content, email.html_content, email.raw_email)?
    } else {
        String::new()
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Delete Inbound Email",
    content = "html",
    status = "NO_CONTENT"
)]
struct DeleteEmailResponse(HtmlBase<&'static str, Error>);

#[delete("/aws/inbound-email/{id}")]
pub async fn inbound_email_delete(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
    id: UuidWrapper,
) -> WarpResult<DeleteEmailResponse> {
    let id = id.into();
    let body = if let Some(email) = InboundEmailDB::get_by_id(&data.aws.pool, id)
        .await
        .map_err(Into::<Error>::into)?
    {
        InboundEmailDB::delete_entry_by_id(id, &data.aws.pool)
            .await
            .map_err(Into::<Error>::into)?;
        data.aws
            .s3
            .delete_key(&email.s3_bucket, &email.s3_key)
            .await
            .map_err(Into::<Error>::into)?;
        "Deleted"
    } else {
        "Id Not Found"
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Sync Inbound Email",
    content = "html",
    status = "CREATED"
)]
struct SyncEmailResponse(HtmlBase<StackString, Error>);

#[post("/aws/inbound-email/sync")]
pub async fn sync_inboud_email(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] data: AppState,
) -> WarpResult<SyncEmailResponse> {
    let sdk_config = aws_config::load_from_env().await;
    let s3 = S3Instance::new(&sdk_config);
    let (new_keys, new_attachments) = InboundEmail::sync_db(&data.aws.config, &s3, &data.aws.pool)
        .await
        .map_err(Into::<Error>::into)
        .map(|(k, a)| (k.join("\n"), a.join("\n")))?;
    let new_records = InboundEmail::parse_dmarc_records(&data.aws.config, &s3, &data.aws.pool)
        .await
        .map_err(Into::<Error>::into)?
        .len();
    let body =
        format!("keys {new_keys}\n\nattachments {new_attachments}\n dmarc_records {new_records}");
    Ok(HtmlBase::new(body.into()).into())
}
