use cached::{proc_macro::cached, SizedCache};
use itertools::Itertools;
use rweb::Schema;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use stack_string::{format_sstr, StackString};
use std::fmt::Display;
use tokio::try_join;

use aws_app_lib::{aws_app_interface::AwsAppInterface, ec2_instance::AmiInfo, errors::AwslibError};

use crate::errors::ServiceError as Error;

#[cached(
    ty = "SizedCache<StackString, Option<AmiInfo>>",
    create = "{ SizedCache::with_size(10) }",
    convert = r#"{ format_sstr!("{}-{}", ubuntu_release, arch) }"#,
    result = true
)]
async fn get_latest_ubuntu_ami(
    app: &AwsAppInterface,
    ubuntu_release: impl Display,
    arch: impl Display,
) -> Result<Option<AmiInfo>, AwslibError> {
    app.ec2.get_latest_ubuntu_ami(ubuntu_release, arch).await
}

pub fn print_tags(tags: impl IntoIterator<Item = (impl Display, impl Display)>) -> StackString {
    tags.into_iter()
        .map(|(k, v)| format_sstr!("{k} = {v}"))
        .join(", ")
        .into()
}

#[derive(Serialize, Deserialize, Schema)]
pub struct TerminateRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateImageRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub inst_id: StackString,
    #[schema(description = "Ami Name")]
    pub name: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteImageRequest {
    #[schema(description = "Ami ID")]
    pub ami: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ModifyVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Volume Size GiB")]
    pub size: i32,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteSnapshotRequest {
    #[schema(description = "Snapshot ID")]
    pub snapid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateSnapshotRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Snapshot Name")]
    pub name: Option<StackString>,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct TagItemRequest {
    #[schema(description = "Resource ID")]
    pub id: StackString,
    #[schema(description = "Tag")]
    pub tag: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteEcrImageRequest {
    #[schema(description = "ECR Repository Name")]
    pub reponame: StackString,
    #[schema(description = "Container Image ID")]
    pub imageid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct StatusRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(Serialize, Deserialize, Debug, Schema)]
pub struct CommandRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
    #[schema(description = "Command String")]
    pub command: StackString,
}

#[must_use]
pub fn get_volumes(current_vol: i64) -> SmallVec<[i64; 8]> {
    [8, 16, 32, 64, 100, 200, 400, 500]
        .iter()
        .map(|x| if *x < current_vol { current_vol } else { *x })
        .dedup()
        .collect()
}

/// # Errors
/// Returns error if db query fails
pub async fn get_ami_tags(app: &AwsAppInterface) -> Result<Vec<AmiInfo>, Error> {
    let ubuntu_ami = async {
        get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "amd64")
            .await
            .map_err(Into::into)
    };
    let ubuntu_ami_arm64 = async {
        get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "arm64")
            .await
            .map_err(Into::into)
    };

    let ami_tags = app.ec2.get_ami_tags();
    let (ubuntu_ami, ubuntu_ami_arm64, ami_tags) =
        try_join!(ubuntu_ami, ubuntu_ami_arm64, ami_tags)?;
    let mut ami_tags: Vec<_> = ami_tags.collect();

    ami_tags.sort_by(|x, y| x.name.cmp(&y.name));
    if let Some(ami) = ubuntu_ami {
        ami_tags.push(ami);
    }
    if let Some(ami) = ubuntu_ami_arm64 {
        ami_tags.push(ami);
    }
    Ok(ami_tags)
}
