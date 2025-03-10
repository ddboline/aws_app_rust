use cached::{proc_macro::cached, SizedCache};
use itertools::Itertools;
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
    let ubuntu_ami =
        async { get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "amd64").await };
    let ubuntu_ami_arm64 =
        async { get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "arm64").await };

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
