use axum::extract::{Json, Path, Query, State};
use derive_more::{From, Into};
use futures::TryStreamExt;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::{StackString, format_sstr};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    fs::{File, read_to_string, remove_file},
    io::AsyncWriteExt,
    task::spawn,
    time::{Duration, sleep},
};
use utoipa::{IntoParams, OpenApi, PartialSchema, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_helper::{
    UtoipaResponse, html_response::HtmlResponse as HtmlBase,
    json_response::JsonResponse as JsonBase,
};
use uuid::Uuid;

use aws_app_lib::{
    ec2_instance::{AmiInfo, InstanceRequest, SpotRequest},
    errors::AwslibError,
    inbound_email::InboundEmail,
    models::{InboundEmailDB, InstanceFamily, InstanceList},
    s3_instance::S3Instance,
};

use super::{
    IamAccessKeyWrapper, IamUserWrapper, ResourceTypeWrapper,
    app::AppState,
    elements::{
        build_spot_request_body, edit_script_body, get_frontpage, get_index, inbound_email_body,
        instance_family_body, instance_status_body, instance_types_body, novnc_start_body,
        novnc_status_body, prices_body, textarea_body, textarea_fixed_size_body,
    },
    errors::ServiceError as Error,
    ipv4addr_wrapper::Ipv4AddrWrapper,
    logged_user::LoggedUser,
};

type WarpResult<T> = Result<T, Error>;

#[derive(UtoipaResponse)]
#[response(description = "Main Page", content = "text/html")]
#[rustfmt::skip]
struct AwsIndexResponse(HtmlBase::<StackString>);

#[utoipa::path(get, path = "/aws/index.html", responses(AwsIndexResponse, Error))]
// AWS App Main Page
async fn sync_frontpage(_: LoggedUser, data: State<Arc<AppState>>) -> WarpResult<AwsIndexResponse> {
    let body = get_index(&data.aws).await?;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct ResourceRequest {
    // Resource Type
    #[param(inline)]
    resource: ResourceTypeWrapper,
}

#[derive(UtoipaResponse)]
#[response(description = "List Resources", content = "text/html")]
#[rustfmt::skip]
struct AwsListResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/list",
    params(ResourceRequest),
    responses(AwsListResponse, Error)
)]
// List AWS Resources
async fn list(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<ResourceRequest>,
) -> WarpResult<AwsListResponse> {
    let Query(query) = query;
    let body = get_frontpage(query.resource.into(), &data.aws).await?;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct TerminateRequest {
    // Instance ID or Name Tag
    #[param(inline)]
    instance: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Deleted", content = "text/html", status = "NO_CONTENT")]
#[rustfmt::skip]
struct DeletedResource(HtmlBase::<&'static str>);

#[utoipa::path(
    delete,
    path = "/aws/terminate",
    params(TerminateRequest),
    responses(DeletedResource, Error)
)]
// Terminate Ec2 Instance
async fn terminate(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<TerminateRequest>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    data.aws
        .terminate(&[query.instance])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct CreateImageRequest {
    // Instance ID or Name Tag
    #[param(inline)]
    inst_id: StackString,
    // Ami Name
    #[param(inline)]
    name: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Image ID", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct CreateImageResponse(HtmlBase::<String>);

#[utoipa::path(
    post,
    path = "/aws/create_image",
    params(CreateImageRequest),
    responses(CreateImageResponse, Error)
)]
// Create EC2 AMI Image
async fn create_image(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CreateImageRequest>,
) -> WarpResult<CreateImageResponse> {
    let Query(query) = query;
    let body: String = data
        .aws
        .create_image(query.inst_id, query.name)
        .await
        .map_err(Into::<Error>::into)?
        .map_or_else(|| "failed to create ami".into(), Into::into);
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteImageRequest {
    // Ami ID
    #[param(inline)]
    ami: StackString,
}

#[utoipa::path(
    delete,
    path = "/aws/delete_image",
    params(DeleteImageRequest),
    responses(DeletedResource, Error)
)]
// Delete EC2 AMI Image
async fn delete_image(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<DeleteImageRequest>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    data.aws
        .delete_image(&query.ami)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteVolumeRequest {
    // Volume ID
    #[param(inline)]
    volid: StackString,
}

#[utoipa::path(
    delete,
    path = "/aws/delete_volume",
    params(DeleteVolumeRequest),
    responses(DeletedResource, Error)
)]
// Delete EC2 Volume
async fn delete_volume(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<DeleteVolumeRequest>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    data.aws
        .delete_ebs_volume(&query.volid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct ModifyVolumeRequest {
    // Volume ID
    #[param(inline)]
    volid: StackString,
    // Volume Size GiB
    size: i32,
}

#[derive(UtoipaResponse)]
#[response(description = "Finished", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct FinishedResource(HtmlBase::<&'static str>);

#[utoipa::path(
    patch,
    path = "/aws/modify_volume",
    params(ModifyVolumeRequest),
    responses(FinishedResource, Error)
)]
// Modify EC2 Volume
async fn modify_volume(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<ModifyVolumeRequest>,
) -> WarpResult<FinishedResource> {
    let Query(query) = query;
    data.aws
        .modify_ebs_volume(&query.volid, query.size)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteSnapshotRequest {
    // Snapshot ID
    #[param(inline)]
    snapid: StackString,
}

#[utoipa::path(
    delete,
    path = "/aws/delete_snapshot",
    params(DeleteSnapshotRequest),
    responses(DeletedResource, Error)
)]
// Delete EC2 Snapshot
async fn delete_snapshot(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<DeleteSnapshotRequest>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    data.aws
        .delete_ebs_snapshot(&query.snapid)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct CreateSnapshotRequest {
    // Volume ID
    #[param(inline)]
    volid: StackString,
    // Snapshot Name
    #[param(inline)]
    name: Option<StackString>,
}

#[utoipa::path(
    post,
    path = "/aws/create_snapshot",
    params(CreateSnapshotRequest),
    responses(FinishedResource, Error)
)]
// Create EC2 Snapshot
async fn create_snapshot(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CreateSnapshotRequest>,
) -> WarpResult<FinishedResource> {
    let Query(query) = query;

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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct TagItemRequest {
    // Resource ID
    #[param(inline)]
    id: StackString,
    // Tag
    #[param(inline)]
    tag: StackString,
}

#[utoipa::path(
    patch,
    path = "/aws/tag_item",
    params(TagItemRequest),
    responses(FinishedResource, Error)
)]
// Tag EC2 Resource
async fn tag_item(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<TagItemRequest>,
) -> WarpResult<FinishedResource> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteEcrImageRequest {
    // ECR Repository Name
    #[param(inline)]
    reponame: StackString,
    // Container Image ID
    #[param(inline)]
    imageid: StackString,
}

#[utoipa::path(
    delete,
    path = "/aws/delete_ecr_image",
    params(DeleteEcrImageRequest),
    responses(DeletedResource, Error)
)]
// Delete ECR Image
async fn delete_ecr_image(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<DeleteEcrImageRequest>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    data.aws
        .ecr
        .delete_ecr_images(&query.reponame, &[query.imageid])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[utoipa::path(
    delete,
    path = "/aws/cleanup_ecr_images",
    responses(DeletedResource, Error)
)]
// Cleanup ECR Images
async fn cleanup_ecr_images(
    _: LoggedUser,
    data: State<Arc<AppState>>,
) -> WarpResult<DeletedResource> {
    data.aws
        .ecr
        .cleanup_ecr_images()
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct ScriptFilename {
    // Script Filename
    #[param(inline)]
    filename: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Edit Script", content = "text/html")]
#[rustfmt::skip]
struct EditScriptResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/edit_script",
    params(ScriptFilename),
    responses(EditScriptResponse, Error)
)]
// Edit Script
async fn edit_script(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<ScriptFilename>,
) -> WarpResult<EditScriptResponse> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema)]
struct ReplaceData {
    // Script Filename
    #[schema(inline)]
    filename: StackString,
    // Script Text
    #[schema(inline)]
    text: StackString,
}

#[utoipa::path(post, path = "/aws/replace_script", request_body = ReplaceData, responses(FinishedResource, Error))]
// Replace Script
async fn replace_script(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    req: Json<ReplaceData>,
) -> WarpResult<FinishedResource> {
    let Json(req) = req;
    let filename = data.aws.config.script_directory.join(&req.filename);
    let mut f = File::create(&filename).await.map_err(Into::<Error>::into)?;
    f.write_all(req.text.as_bytes())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[utoipa::path(
    delete,
    path = "/aws/delete_script",
    params(ScriptFilename),
    responses(DeletedResource, Error)
)]
// Delete Script
async fn delete_script(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<ScriptFilename>,
) -> WarpResult<DeletedResource> {
    let Query(query) = query;
    let filename = data.aws.config.script_directory.join(&query.filename);
    if filename.exists() {
        remove_file(&filename).await.map_err(Into::<Error>::into)?;
    }
    Ok(HtmlBase::new("Deleted").into())
}

#[derive(Serialize, Deserialize, Debug, ToSchema, IntoParams)]
struct SpotBuilder {
    // AMI ID
    #[param(inline)]
    ami: Option<StackString>,
    // Instance Type
    #[param(inline)]
    inst: Option<StackString>,
    // Script
    #[param(inline)]
    script: Option<StackString>,
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

#[derive(UtoipaResponse)]
#[response(description = "Spot Request", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct BuildSpotResponse(HtmlBase::<StackString>);

#[utoipa::path(
    post,
    path = "/aws/build_spot_request",
    params(SpotBuilder),
    responses(BuildSpotResponse, Error)
)]
// Build Spot Request
async fn build_spot_request(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<SpotBuilder>,
) -> WarpResult<BuildSpotResponse> {
    let Query(query) = query;
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

    let security_groups: Vec<_> = data
        .aws
        .ec2
        .get_all_security_groups()
        .await
        .map_err(Into::<Error>::into)?
        .collect();

    let body = build_spot_request_body(
        amis,
        inst_fams,
        instances,
        files,
        keys,
        security_groups,
        data.aws.config.clone(),
    )?
    .into();

    Ok(HtmlBase::new(body).into())
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, ToSchema)]
struct SpotRequestData {
    // Ami ID
    #[schema(inline)]
    ami: StackString,
    // Instance Type
    #[schema(inline)]
    instance_type: StackString,
    // Security Group
    #[schema(inline)]
    security_group: StackString,
    // Script Filename
    #[schema(inline)]
    script: StackString,
    // SSH Key Name
    #[schema(inline)]
    key_name: StackString,
    // Spot Price
    #[schema(inline)]
    price: StackString,
    // Spot Request Name Tag
    #[schema(inline)]
    name: StackString,
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

#[utoipa::path(post, path = "/aws/request_spot", request_body = SpotRequestData, responses(FinishedResource, Error))]
async fn request_spot(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    req: Json<SpotRequestData>,
) -> WarpResult<FinishedResource> {
    let Json(req) = req;
    let req: SpotRequest = req.into();
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
        spawn(async move { ec2.tag_spot_instance(&spot_id, &tags).await });
    }
    Ok(HtmlBase::new("Finished").into())
}

#[utoipa::path(post, path = "/aws/run_instance", request_body = SpotRequestData, responses(FinishedResource, Error))]
async fn run_instance(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    req: Json<SpotRequestData>,
) -> WarpResult<FinishedResource> {
    let Json(req) = req;
    let req: InstanceRequest = req.into();
    data.aws
        .ec2
        .run_ec2_instance(&req)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new("Finished").into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct CancelSpotRequest {
    // Spot Request ID
    #[param(inline)]
    spot_id: StackString,
}

#[derive(UtoipaResponse)]
#[response(
    description = "Cancelled Spot",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct CancelledResponse(HtmlBase::<StackString>);

#[utoipa::path(
    delete,
    path = "/aws/cancel_spot",
    params(CancelSpotRequest),
    responses(CancelledResponse, Error)
)]
// Cancel Spot Request
async fn cancel_spot(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CancelSpotRequest>,
) -> WarpResult<CancelledResponse> {
    let Query(query) = query;
    data.aws
        .ec2
        .cancel_spot_instance_request(&[query.spot_id.clone()])
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!("cancelled {}", query.spot_id)).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct PriceRequest {
    // Search String
    #[param(inline)]
    search: Option<StackString>,
}

#[derive(UtoipaResponse)]
#[response(description = "Prices", content = "text/html")]
#[rustfmt::skip]
struct PricesResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/prices",
    params(PriceRequest),
    responses(PricesResponse, Error)
)]
// Get Ec2 Prices
async fn get_prices(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<PriceRequest>,
) -> WarpResult<PricesResponse> {
    let Query(query) = query;

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

#[derive(UtoipaResponse)]
#[response(description = "Update", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct UpdateResponse(HtmlBase::<StackString>);

#[utoipa::path(post, path = "/aws/update", responses(UpdateResponse, Error))]
// Update Data
async fn update(_: LoggedUser, data: State<Arc<AppState>>) -> WarpResult<UpdateResponse> {
    let entries: Vec<StackString> = data
        .aws
        .update()
        .await
        .map_err(Into::<Error>::into)?
        .collect();
    let body = textarea_body(entries, "diary_editor_form".into())?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct StatusRequest {
    // Instance ID or Name Tag
    #[param(inline)]
    instance: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Instance Status", content = "text/html")]
#[rustfmt::skip]
struct InstanceStatusResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/instance_status",
    params(StatusRequest),
    responses(InstanceStatusResponse, Error)
)]
// Get Ec2 Instance Status
async fn instance_status(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<StatusRequest>,
) -> WarpResult<InstanceStatusResponse> {
    let Query(query) = query;
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

#[derive(UtoipaResponse)]
#[response(
    description = "Run Command on Instance",
    content = "text/html",
    status = "CREATED"
)]
#[rustfmt::skip]
struct CommandResponse(HtmlBase::<StackString>);

#[derive(Serialize, Deserialize, Debug, ToSchema)]
struct CommandRequest {
    // Instance ID or Name Tag
    #[schema(inline)]
    instance: StackString,
    // Command String
    #[schema(inline)]
    command: StackString,
}

#[utoipa::path(post, path = "/aws/command", request_body = CommandRequest, responses(CommandResponse, Error))]
// Run command on Ec2 Instance
async fn command(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    payload: Json<CommandRequest>,
) -> WarpResult<CommandResponse> {
    let Json(payload) = payload;
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct InstancesRequest {
    // Instance ID or Name Tag
    #[param(inline)]
    inst: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Describe Instances", content = "text/html")]
#[rustfmt::skip]
struct InstancesResponse(HtmlBase::<String>);

#[utoipa::path(
    get,
    path = "/aws/instances",
    params(InstancesRequest),
    responses(InstancesResponse, Error)
)]
// List Ec2 Instances
async fn get_instances(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<InstancesRequest>,
) -> WarpResult<InstancesResponse> {
    let Query(query) = query;
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

#[derive(UtoipaResponse)]
#[response(description = "Start NoVNC", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct NovncStartResponse(HtmlBase::<StackString>);

#[utoipa::path(post, path = "/aws/novnc/start", responses(NovncStartResponse, Error))]
// Start NoVNC Service
async fn novnc_launcher(
    _: LoggedUser,
    data: State<Arc<AppState>>,
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

#[derive(UtoipaResponse)]
#[response(description = "Stop NoVNC", content = "text/html", status = "CREATED")]
#[rustfmt::skip]
struct NovncStopResponse(HtmlBase::<StackString>);

#[utoipa::path(post, path = "/aws/novnc/stop", responses(NovncStopResponse, Error))]
// Stop NoVNC Service
async fn novnc_shutdown(
    _: LoggedUser,
    data: State<Arc<AppState>>,
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

#[derive(UtoipaResponse)]
#[response(description = "NoVNC Status", content = "text/html")]
#[rustfmt::skip]
struct NovncStatusResponse(HtmlBase::<StackString>);

#[utoipa::path(get, path = "/aws/novnc/status", responses(NovncStatusResponse, Error))]
// NoVNC Service Status
async fn novnc_status(
    _: LoggedUser,
    data: State<Arc<AppState>>,
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

#[derive(UtoipaResponse)]
#[response(description = "Logged in User")]
#[rustfmt::skip]
struct UserResponse(JsonBase::<LoggedUser>);

#[utoipa::path(get, path = "/aws/user", responses(UserResponse, Error))]
// User Object if logged in
async fn user(user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct CreateUserRequest {
    // User Name
    #[param(inline)]
    user_name: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Created Iam User", status = "CREATED")]
#[rustfmt::skip]
struct CreateUserResponse(JsonBase::<IamUserWrapper>);

#[utoipa::path(
    post,
    path = "/aws/create_user",
    params(CreateUserRequest),
    responses(CreateUserResponse, Error)
)]
// Create IAM User
async fn create_user(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CreateUserRequest>,
) -> WarpResult<CreateUserResponse> {
    let Query(query) = query;
    let user = data
        .aws
        .create_user(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?
        .ok_or_else(|| Error::BadRequest("create user failed".into()))?;
    let resp = JsonBase::new(user.into());
    Ok(resp.into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Delete Iam User",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct DeleteUserResponse(HtmlBase::<StackString>);

#[utoipa::path(
    delete,
    path = "/aws/delete_user",
    params(CreateUserRequest),
    responses(DeleteUserResponse, Error)
)]
// Delete IAM User
async fn delete_user(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CreateUserRequest>,
) -> WarpResult<DeleteUserResponse> {
    let Query(query) = query;
    data.aws
        .delete_user(query.user_name.as_str())
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(format_sstr!("{} deleted", query.user_name)).into())
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct AddUserToGroupRequest {
    // User Name
    #[param(inline)]
    user_name: StackString,
    // Group Name
    #[param(inline)]
    group_name: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Add User to Group", content = "text/html")]
#[rustfmt::skip]
struct AddUserGroupResponse(HtmlBase::<StackString>);

#[utoipa::path(
    patch,
    path = "/aws/add_user_to_group",
    params(AddUserToGroupRequest),
    responses(AddUserGroupResponse, Error)
)]
// Add IAM User to Group
async fn add_user_to_group(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<AddUserGroupResponse> {
    let Query(query) = query;
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

#[derive(UtoipaResponse)]
#[response(
    description = "Remove User to Group",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct RemoveUserGroupResponse(HtmlBase::<StackString>);

#[utoipa::path(
    delete,
    path = "/aws/remove_user_from_group",
    params(AddUserToGroupRequest),
    responses(RemoveUserGroupResponse, Error)
)]
// Remove IAM User from Group
async fn remove_user_from_group(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<AddUserToGroupRequest>,
) -> WarpResult<RemoveUserGroupResponse> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteAccesssKeyRequest {
    // User Name
    #[param(inline)]
    user_name: StackString,
    // Access Key ID
    #[param(inline)]
    access_key_id: StackString,
}

#[derive(ToSchema, Serialize, Into, From)]
struct CreateKeyInner(Option<IamAccessKeyWrapper>);

#[derive(UtoipaResponse)]
#[response(description = "Create Access Key", status = "CREATED")]
#[rustfmt::skip]
struct CreateKeyResponse(JsonBase::<CreateKeyInner>);

#[utoipa::path(
    post,
    path = "/aws/create_access_key",
    params(CreateUserRequest),
    responses(CreateKeyResponse, Error)
)]
// Create Access Key for IAM User
async fn create_access_key(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<CreateUserRequest>,
) -> WarpResult<CreateKeyResponse> {
    let Query(query) = query;
    let access_key = data.aws.create_access_key(query.user_name.as_str()).await?;
    Ok(JsonBase::new(access_key.map(Into::into).into()).into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Delete Access Key",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct DeleteKeyResponse(HtmlBase::<StackString>);

#[utoipa::path(
    delete,
    path = "/aws/delete_access_key",
    params(DeleteAccesssKeyRequest),
    responses(DeleteKeyResponse, Error)
)]
// Delete Access Key for IAM User
async fn delete_access_key(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<DeleteAccesssKeyRequest>,
) -> WarpResult<DeleteKeyResponse> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct UpdateDnsNameRequest {
    // Route53 Zone
    #[param(inline)]
    zone: StackString,
    // DNS Name
    #[param(inline)]
    dns_name: StackString,
    // Old IPv4 Address
    #[param(inline)]
    old_ip: Ipv4AddrWrapper,
    // New IPv4 Address
    #[param(inline)]
    new_ip: Ipv4AddrWrapper,
}

#[derive(UtoipaResponse)]
#[response(description = "Update Dns", status = "CREATED", content = "text/html")]
#[rustfmt::skip]
struct UpdateDnsResponse(HtmlBase::<StackString>);

#[utoipa::path(
    patch,
    path = "/aws/update_dns_name",
    params(UpdateDnsNameRequest),
    responses(UpdateDnsResponse, Error)
)]
// Update DNS Name
async fn update_dns_name(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<UpdateDnsNameRequest>,
) -> WarpResult<UpdateDnsResponse> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy)]
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct SystemdAction {
    // SystemD Action
    action: SystemdActions,
    // SystemD Service
    #[param(inline)]
    service: StackString,
}

#[derive(UtoipaResponse)]
#[response(
    description = "Systemd Action Output",
    status = "CREATED",
    content = "text/html"
)]
#[rustfmt::skip]
struct SystemdActionResponse(HtmlBase::<StackString>);

#[utoipa::path(
    post,
    path = "/aws/systemd_action",
    params(SystemdAction),
    responses(SystemdActionResponse, Error)
)]
// Perform Systemd Action
async fn systemd_action(
    _: LoggedUser,
    data: State<Arc<AppState>>,
    query: Query<SystemdAction>,
) -> WarpResult<SystemdActionResponse> {
    let Query(query) = query;
    let output = data
        .aws
        .systemd
        .service_action(query.action.as_str(), &query.service)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(HtmlBase::new(output).into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Restart All Systemd Services",
    status = "CREATED",
    content = "text/html"
)]
#[rustfmt::skip]
struct SystemdRestartAllResponse(HtmlBase::<String>);

#[utoipa::path(
    post,
    path = "/aws/systemd_restart_all",
    responses(SystemdRestartAllResponse, Error)
)]
// Restart all Systemd Services
async fn systemd_restart_all(
    _: LoggedUser,
    data: State<Arc<AppState>>,
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

#[derive(UtoipaResponse)]
#[response(description = "Get Systemd Logs", content = "text/html")]
#[rustfmt::skip]
struct SystemdLogResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/systemd_logs/{service}",
    params(("service" = inline(StackString), description = "Systemd Service")),
    responses(SystemdLogResponse, Error)
)]
// Get Systemd Logs for Service
async fn systemd_logs(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    service: Path<StackString>,
) -> WarpResult<SystemdLogResponse> {
    let Path(service) = service;
    let entries: Vec<StackString> = data
        .aws
        .systemd
        .get_service_logs(&service)
        .await?
        .into_iter()
        .map(|log| log.to_string().into())
        .collect();
    let body = textarea_body(entries, "systemd-logs".into())?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(UtoipaResponse)]
#[response(description = "Get Crontab Logs", content = "text/html")]
#[rustfmt::skip]
struct CrontabLogResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/aws/crontab_logs/{crontab_type}",
    params(("crontab_type" = inline(StackString), description = "Crontab Type")),
    responses(CrontabLogResponse, Error)
)]
// Get Crontab Logs
async fn crontab_logs(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    crontab_type: Path<StackString>,
) -> WarpResult<CrontabLogResponse> {
    let Path(crontab_type) = crontab_type;
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

#[derive(UtoipaResponse)]
#[response(description = "Get Inbound Email Detail", content = "text/html")]
#[rustfmt::skip]
struct InboundEmailDetailResponse(HtmlBase::<String>);

#[utoipa::path(
    get,
    path = "/aws/inbound-email/{id}",
    params(("id" = inline(Uuid), description = "Email ID")),
    responses(InboundEmailDetailResponse, Error)
)]
async fn inbound_email_detail(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    id: Path<Uuid>,
) -> WarpResult<InboundEmailDetailResponse> {
    let Path(id) = id;
    let body = if let Some(email) = InboundEmailDB::get_by_id(&data.aws.pool, id)
        .await
        .map_err(Into::<Error>::into)?
    {
        inbound_email_body(email.text_content, email.html_content, email.raw_email)?
    } else {
        String::new()
    };
    Ok(HtmlBase::new(body).into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Delete Inbound Email",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct DeleteEmailResponse(HtmlBase::<&'static str>);

#[utoipa::path(
    delete,
    path = "/aws/inbound-email/{id}",
    params(("id" = inline(Uuid), description = "Email ID")),
    responses(DeleteEmailResponse, Error)
)]
async fn inbound_email_delete(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    id: Path<Uuid>,
) -> WarpResult<DeleteEmailResponse> {
    let Path(id) = id;
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct DeleteSecurityGroupRequest {
    // Security Group ID
    #[param(inline)]
    group_id: StackString,
}

#[derive(UtoipaResponse)]
#[response(
    description = "Delete Security Group",
    content = "text/html",
    status = "NO_CONTENT"
)]
#[rustfmt::skip]
struct DeleteSecurityGroupResponse(HtmlBase::<&'static str>);

#[utoipa::path(
    delete,
    path = "/aws/delete_security_group",
    params(DeleteSecurityGroupRequest),
    responses(DeleteSecurityGroupResponse, Error)
)]
async fn delete_security_group(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    query: Query<DeleteSecurityGroupRequest>,
) -> WarpResult<DeleteSecurityGroupResponse> {
    let Query(DeleteSecurityGroupRequest { group_id }) = query;

    data.aws.ec2.delete_security_group(&group_id).await?;

    Ok(HtmlBase::new("finished").into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Sync Inbound Email",
    content = "text/html",
    status = "CREATED"
)]
#[rustfmt::skip]
struct SyncEmailResponse(HtmlBase::<StackString>);

#[utoipa::path(
    post,
    path = "/aws/inbound-email/sync",
    responses(SyncEmailResponse, Error)
)]
async fn sync_inboud_email(
    _: LoggedUser,
    data: State<Arc<AppState>>,
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

pub fn get_aws_path(app: &AppState) -> OpenApiRouter {
    let app = Arc::new(app.clone());

    OpenApiRouter::new()
        .routes(routes!(sync_frontpage))
        .routes(routes!(list))
        .routes(routes!(terminate))
        .routes(routes!(create_image))
        .routes(routes!(delete_image))
        .routes(routes!(delete_volume))
        .routes(routes!(modify_volume))
        .routes(routes!(delete_snapshot))
        .routes(routes!(create_snapshot))
        .routes(routes!(tag_item))
        .routes(routes!(delete_ecr_image))
        .routes(routes!(cleanup_ecr_images))
        .routes(routes!(edit_script))
        .routes(routes!(replace_script))
        .routes(routes!(delete_script))
        .routes(routes!(create_user))
        .routes(routes!(delete_user))
        .routes(routes!(add_user_to_group))
        .routes(routes!(remove_user_from_group))
        .routes(routes!(create_access_key))
        .routes(routes!(delete_access_key))
        .routes(routes!(build_spot_request))
        .routes(routes!(request_spot))
        .routes(routes!(run_instance))
        .routes(routes!(cancel_spot))
        .routes(routes!(get_prices))
        .routes(routes!(update))
        .routes(routes!(instance_status))
        .routes(routes!(command))
        .routes(routes!(get_instances))
        .routes(routes!(user))
        .routes(routes!(novnc_launcher))
        .routes(routes!(novnc_status))
        .routes(routes!(novnc_shutdown))
        .routes(routes!(update_dns_name))
        .routes(routes!(systemd_action))
        .routes(routes!(systemd_logs))
        .routes(routes!(systemd_restart_all))
        .routes(routes!(crontab_logs))
        .routes(routes!(inbound_email_detail))
        .routes(routes!(inbound_email_delete))
        .routes(routes!(sync_inboud_email))
        .routes(routes!(delete_security_group))
        .with_state(app)
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Frontend for AWS",
        description = "Web Frontend for AWS Services",
    ),
    components(schemas(
        IamAccessKeyWrapper,
        SystemdActions,
        ResourceTypeWrapper,
        Ipv4AddrWrapper
    ))
)]
pub struct ApiDoc;
