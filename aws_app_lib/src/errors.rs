use aws_sdk_ec2::operation::{
    attach_volume::AttachVolumeError,
    cancel_spot_instance_requests::CancelSpotInstanceRequestsError, create_image::CreateImageError,
    create_snapshot::CreateSnapshotError, create_tags::CreateTagsError,
    create_volume::CreateVolumeError, delete_snapshot::DeleteSnapshotError,
    delete_volume::DeleteVolumeError, deregister_image::DeregisterImageError,
    describe_availability_zones::DescribeAvailabilityZonesError,
    describe_images::DescribeImagesError, describe_instances::DescribeInstancesError,
    describe_key_pairs::DescribeKeyPairsError, describe_regions::DescribeRegionsError,
    describe_reserved_instances::DescribeReservedInstancesError,
    describe_snapshots::DescribeSnapshotsError,
    describe_spot_instance_requests::DescribeSpotInstanceRequestsError,
    describe_spot_price_history::DescribeSpotPriceHistoryError,
    describe_volumes::DescribeVolumesError, detach_volume::DetachVolumeError,
    modify_volume::ModifyVolumeError, request_spot_instances::RequestSpotInstancesError,
    run_instances::RunInstancesError, terminate_instances::TerminateInstancesError,
};
use aws_sdk_ecr::operation::{
    batch_delete_image::BatchDeleteImageError,
    describe_images::DescribeImagesError as DescribeEcrImagesError,
    describe_repositories::DescribeRepositoriesError,
};
use aws_sdk_iam::operation::{
    add_user_to_group::AddUserToGroupError, create_access_key::CreateAccessKeyError,
    create_user::CreateUserError, delete_access_key::DeleteAccessKeyError,
    delete_user::DeleteUserError, get_user::GetUserError, list_access_keys::ListAccessKeysError,
    list_groups::ListGroupsError, list_groups_for_user::ListGroupsForUserError,
    list_users::ListUsersError, remove_user_from_group::RemoveUserFromGroupError,
};
use aws_sdk_pricing::operation::{
    describe_services::DescribeServicesError, get_attribute_values::GetAttributeValuesError,
    get_products::GetProductsError,
};
use aws_sdk_route53::operation::{
    change_resource_record_sets::ChangeResourceRecordSetsError,
    list_hosted_zones::ListHostedZonesError,
    list_resource_record_sets::ListResourceRecordSetsError,
};
use aws_sdk_s3::operation::{
    copy_object::CopyObjectError, create_bucket::CreateBucketError,
    delete_bucket::DeleteBucketError, delete_object::DeleteObjectError, get_object::GetObjectError,
    list_buckets::ListBucketsError, list_objects::ListObjectsError, put_object::PutObjectError,
};
use aws_sdk_ses::operation::{
    get_send_quota::GetSendQuotaError, get_send_statistics::GetSendStatisticsError,
    send_email::SendEmailError,
};
use aws_sdk_sts::operation::get_caller_identity::GetCallerIdentityError;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::{
    byte_stream::error::Error as AwsByteStreamError, error::operation::BuildError as AwsBuildError,
};
use deadpool_postgres::{BuildError as DeadpoolBuildError, ConfigError as DeadpoolConfigError};
use envy::Error as EnvyError;
use postgres_query::{Error as PqError, extract::Error as PqExtractError};
use rand::distr::uniform::Error as RandUniformError;
use refinery::Error as RefineryError;
use reqwest::Error as ReqwestError;
use roxmltree::Error as RoXmlTreeError;
use serde_json::Error as SerdeJsonError;
use stack_string::StackString;
use std::{
    net::AddrParseError,
    num::{ParseFloatError, ParseIntError},
    str::Utf8Error,
    string::FromUtf8Error,
};
use stdout_channel::StdoutChannelError;
use thiserror::Error;
use time::error::{
    ComponentRange as TimeComponentRangeError, Format as TimeFormatError, Parse as TimeParseError,
};
use tokio::task::JoinError;
use tokio_postgres::error::Error as TokioPostgresError;
use url::ParseError as UrlParseError;
use zip::result::ZipError;

type DeadPoolError = deadpool::managed::PoolError<TokioPostgresError>;

type AwsSdkError<T> = SdkError<T, aws_smithy_runtime_api::http::Response>;

#[derive(Error, Debug)]
pub enum AwslibError {
    #[error("{0}")]
    CustomError(StackString),
    #[error("{0}")]
    StaticCustomError(&'static str),
    #[error("RandUniformError {0}")]
    RandUniformError(#[from] RandUniformError),
    #[error("AwsByteStreamError {0}")]
    AwsByteStreamError(#[from] AwsByteStreamError),
    #[error("DeadpoolBuildError {0}")]
    DeadpoolBuildError(#[from] DeadpoolBuildError),
    #[error("DeadpoolConfigError {0}")]
    DeadpoolConfigError(#[from] DeadpoolConfigError),
    #[error("TokioPostgresError {0}")]
    TokioPostgresError(#[from] TokioPostgresError),
    #[error("PqError {0}")]
    PqError(Box<PqError>),
    #[error("PqExtractError {0}")]
    PqExtractError(Box<PqExtractError>),
    #[error("EnvyError {0}")]
    EnvyError(#[from] EnvyError),
    #[error("TimeParseError {0}")]
    TimeParseError(Box<TimeParseError>),
    #[error("TimeFormatError {0}")]
    TimeFormatError(#[from] TimeFormatError),
    #[error("TimeComponentRangeError {0}")]
    TimeComponentRangeError(Box<TimeComponentRangeError>),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("Utf8Error {0}")]
    Utf8Error(#[from] Utf8Error),
    #[error("tokio join error {0}")]
    JoinError(#[from] JoinError),
    #[error("ZipError {0}")]
    ZipError(#[from] ZipError),
    #[error("RoXmlTreeError {0}")]
    RoXmlTreeError(Box<RoXmlTreeError>),
    #[error("FromUtf8Error {0}")]
    FromUtf8Error(Box<FromUtf8Error>),
    #[error("DeadPoolError {0}")]
    DeadPoolError(#[from] DeadPoolError),
    #[error("StdoutChannelError {0}")]
    StdoutChannelError(#[from] StdoutChannelError),
    #[error("RefineryError {0}")]
    RefineryError(#[from] RefineryError),
    #[error("UrlParseError {0}")]
    UrlParseError(#[from] UrlParseError),
    #[error("ParseFloatError {0}")]
    ParseFloatError(#[from] ParseFloatError),
    #[error("ParseIntError {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("ReqwestError {0}")]
    ReqwestError(#[from] ReqwestError),
    #[error("AddrParseError {0}")]
    AddrParseError(#[from] AddrParseError),
    #[error("AwsBuildError {0}")]
    AwsBuildError(Box<AwsBuildError>),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),

    #[error("ListObjectsError {0}")]
    ListObjectsError(Box<AwsSdkError<ListObjectsError>>),
    #[error("GetObjectError {0}")]
    GetObjectError(Box<AwsSdkError<GetObjectError>>),
    #[error("PutObjectError {0}")]
    PutObjectError(Box<AwsSdkError<PutObjectError>>),
    #[error("DescribeImagesError {0}")]
    DescribeImagesError(Box<AwsSdkError<DescribeImagesError>>),
    #[error("DescribeRegionsError {0}")]
    DescribeRegionsError(Box<AwsSdkError<DescribeRegionsError>>),
    #[error("DescribeInstancesError {0}")]
    DescribeInstancesError(Box<AwsSdkError<DescribeInstancesError>>),
    #[error("DescribeReservedInstancesError {0}")]
    DescribeReservedInstancesError(Box<AwsSdkError<DescribeReservedInstancesError>>),
    #[error("DescribeAvailabilityZonesError {0}")]
    DescribeAvailabilityZonesError(Box<AwsSdkError<DescribeAvailabilityZonesError>>),
    #[error("DescribeKeyPairsError {0}")]
    DescribeKeyPairsError(Box<AwsSdkError<DescribeKeyPairsError>>),
    #[error("DescribeSpotPriceHistoryError {0}")]
    DescribeSpotPriceHistoryError(Box<AwsSdkError<DescribeSpotPriceHistoryError>>),
    #[error("DeleteSnapshotError {0}")]
    DeleteSnapshotError(Box<AwsSdkError<DeleteSnapshotError>>),
    #[error("CreateSnapshotError {0}")]
    CreateSnapshotError(Box<AwsSdkError<CreateSnapshotError>>),
    #[error("ModifyVolumeError {0}")]
    ModifyVolumeError(Box<AwsSdkError<ModifyVolumeError>>),
    #[error("DetachVolumeError {0}")]
    DetachVolumeError(Box<AwsSdkError<DetachVolumeError>>),
    #[error("AttachVolumeError {0}")]
    AttachVolumeError(Box<AwsSdkError<AttachVolumeError>>),
    #[error("DeleteVolumeError {0}")]
    DeleteVolumeError(Box<AwsSdkError<DeleteVolumeError>>),
    #[error("CreateVolumeError {0}")]
    CreateVolumeError(Box<AwsSdkError<CreateVolumeError>>),
    #[error("DeregisterImageError {0}")]
    DeregisterImageError(Box<AwsSdkError<DeregisterImageError>>),
    #[error("CreateImageError {0}")]
    CreateImageError(Box<AwsSdkError<CreateImageError>>),
    #[error("CreateTagsError {0}")]
    CreateTagsError(Box<AwsSdkError<CreateTagsError>>),
    #[error("CancelSpotInstanceRequestsError {0}")]
    CancelSpotInstanceRequestsError(Box<AwsSdkError<CancelSpotInstanceRequestsError>>),
    #[error("RequestSpotInstancesError {0}")]
    RequestSpotInstancesError(Box<AwsSdkError<RequestSpotInstancesError>>),
    #[error("TerminateInstancesError {0}")]
    TerminateInstancesError(Box<AwsSdkError<TerminateInstancesError>>),
    #[error("DescribeSnapshotsError {0}")]
    DescribeSnapshotsError(Box<AwsSdkError<DescribeSnapshotsError>>),
    #[error("DescribeVolumesError {0}")]
    DescribeVolumesError(Box<AwsSdkError<DescribeVolumesError>>),
    #[error("DescribeSpotInstanceRequestsError {0}")]
    DescribeSpotInstanceRequestsError(Box<AwsSdkError<DescribeSpotInstanceRequestsError>>),
    #[error("RunInstancesError {0}")]
    RunInstancesError(Box<AwsSdkError<RunInstancesError>>),
    #[error("DescribeRepositoriesError {0}")]
    DescribeRepositoriesError(Box<AwsSdkError<DescribeRepositoriesError>>),
    #[error("DescribeEcrImagesError {0}")]
    DescribeEcrImagesError(Box<AwsSdkError<DescribeEcrImagesError>>),
    #[error("BatchDeleteImageError {0}")]
    BatchDeleteImageError(Box<AwsSdkError<BatchDeleteImageError>>),
    #[error("ListUsersError {0}")]
    ListUsersError(Box<AwsSdkError<ListUsersError>>),
    #[error("GetUserError {0}")]
    GetUserError(Box<AwsSdkError<GetUserError>>),
    #[error("ListGroupsError {0}")]
    ListGroupsError(Box<AwsSdkError<ListGroupsError>>),
    #[error("DeleteUserError {0}")]
    DeleteUserError(Box<AwsSdkError<DeleteUserError>>),
    #[error("AddUserToGroupError {0}")]
    AddUserToGroupError(Box<AwsSdkError<AddUserToGroupError>>),
    #[error("RemoveUserFromGroupError {0}")]
    RemoveUserFromGroupError(Box<AwsSdkError<RemoveUserFromGroupError>>),
    #[error("ListAccessKeysError {0}")]
    ListAccessKeysError(Box<AwsSdkError<ListAccessKeysError>>),
    #[error("ListGroupsForUserError {0}")]
    ListGroupsForUserError(Box<AwsSdkError<ListGroupsForUserError>>),
    #[error("CreateUserError {0}")]
    CreateUserError(Box<AwsSdkError<CreateUserError>>),
    #[error("CreateAccessKeyError {0}")]
    CreateAccessKeyError(Box<AwsSdkError<CreateAccessKeyError>>),
    #[error("DeleteAccessKeyError {0}")]
    DeleteAccessKeyError(Box<AwsSdkError<DeleteAccessKeyError>>),
    #[error("GetCallerIdentityError {0}")]
    GetCallerIdentityError(Box<AwsSdkError<GetCallerIdentityError>>),
    #[error("DescribeServicesError {0}")]
    DescribeServicesError(Box<AwsSdkError<DescribeServicesError>>),
    #[error("GetAttributeValuesError {0}")]
    GetAttributeValuesError(Box<AwsSdkError<GetAttributeValuesError>>),
    #[error("GetProductsError {0}")]
    GetProductsError(Box<AwsSdkError<GetProductsError>>),
    #[error("ListHostedZonesError {0}")]
    ListHostedZonesError(Box<AwsSdkError<ListHostedZonesError>>),
    #[error("ListResourceRecordSetsError {0}")]
    ListResourceRecordSetsError(Box<AwsSdkError<ListResourceRecordSetsError>>),
    #[error("ChangeResourceRecordSetsError {0}")]
    ChangeResourceRecordSetsError(Box<AwsSdkError<ChangeResourceRecordSetsError>>),
    #[error("ListBucketsError {0}")]
    ListBucketsError(Box<AwsSdkError<ListBucketsError>>),
    #[error("DeleteBucketError {0}")]
    DeleteBucketError(Box<AwsSdkError<DeleteBucketError>>),
    #[error("DeleteObjectError {0}")]
    DeleteObjectError(Box<AwsSdkError<DeleteObjectError>>),
    #[error("CopyObjectError {0}")]
    CopyObjectError(Box<AwsSdkError<CopyObjectError>>),
    #[error("CreateBucketError {0}")]
    CreateBucketError(Box<AwsSdkError<CreateBucketError>>),
    #[error("SendEmailError {0}")]
    SendEmailError(Box<AwsSdkError<SendEmailError>>),
    #[error("GetSendStatisticsError {0}")]
    GetSendStatisticsError(Box<AwsSdkError<GetSendStatisticsError>>),
    #[error("GetSendQuotaError {0}")]
    GetSendQuotaError(Box<AwsSdkError<GetSendQuotaError>>),
}

impl From<AwsSdkError<ListObjectsError>> for AwslibError {
    fn from(value: AwsSdkError<ListObjectsError>) -> Self {
        Self::ListObjectsError(Box::new(value))
    }
}

impl From<AwsSdkError<GetObjectError>> for AwslibError {
    fn from(value: AwsSdkError<GetObjectError>) -> Self {
        Self::GetObjectError(Box::new(value))
    }
}

impl From<AwsSdkError<PutObjectError>> for AwslibError {
    fn from(value: AwsSdkError<PutObjectError>) -> Self {
        Self::PutObjectError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeImagesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeImagesError>) -> Self {
        Self::DescribeImagesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeRegionsError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeRegionsError>) -> Self {
        Self::DescribeRegionsError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeInstancesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeInstancesError>) -> Self {
        Self::DescribeInstancesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeReservedInstancesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeReservedInstancesError>) -> Self {
        Self::DescribeReservedInstancesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeAvailabilityZonesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeAvailabilityZonesError>) -> Self {
        Self::DescribeAvailabilityZonesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeKeyPairsError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeKeyPairsError>) -> Self {
        Self::DescribeKeyPairsError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeSpotPriceHistoryError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeSpotPriceHistoryError>) -> Self {
        Self::DescribeSpotPriceHistoryError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteSnapshotError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteSnapshotError>) -> Self {
        Self::DeleteSnapshotError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateSnapshotError>> for AwslibError {
    fn from(value: AwsSdkError<CreateSnapshotError>) -> Self {
        Self::CreateSnapshotError(Box::new(value))
    }
}

impl From<AwsSdkError<ModifyVolumeError>> for AwslibError {
    fn from(value: AwsSdkError<ModifyVolumeError>) -> Self {
        Self::ModifyVolumeError(Box::new(value))
    }
}

impl From<AwsSdkError<DetachVolumeError>> for AwslibError {
    fn from(value: AwsSdkError<DetachVolumeError>) -> Self {
        Self::DetachVolumeError(Box::new(value))
    }
}

impl From<AwsSdkError<AttachVolumeError>> for AwslibError {
    fn from(value: AwsSdkError<AttachVolumeError>) -> Self {
        Self::AttachVolumeError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteVolumeError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteVolumeError>) -> Self {
        Self::DeleteVolumeError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateVolumeError>> for AwslibError {
    fn from(value: AwsSdkError<CreateVolumeError>) -> Self {
        Self::CreateVolumeError(Box::new(value))
    }
}

impl From<AwsSdkError<DeregisterImageError>> for AwslibError {
    fn from(value: AwsSdkError<DeregisterImageError>) -> Self {
        Self::DeregisterImageError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateImageError>> for AwslibError {
    fn from(value: AwsSdkError<CreateImageError>) -> Self {
        Self::CreateImageError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateTagsError>> for AwslibError {
    fn from(value: AwsSdkError<CreateTagsError>) -> Self {
        Self::CreateTagsError(Box::new(value))
    }
}

impl From<AwsSdkError<CancelSpotInstanceRequestsError>> for AwslibError {
    fn from(value: AwsSdkError<CancelSpotInstanceRequestsError>) -> Self {
        Self::CancelSpotInstanceRequestsError(Box::new(value))
    }
}

impl From<AwsSdkError<RequestSpotInstancesError>> for AwslibError {
    fn from(value: AwsSdkError<RequestSpotInstancesError>) -> Self {
        Self::RequestSpotInstancesError(Box::new(value))
    }
}

impl From<AwsSdkError<TerminateInstancesError>> for AwslibError {
    fn from(value: AwsSdkError<TerminateInstancesError>) -> Self {
        Self::TerminateInstancesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeSnapshotsError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeSnapshotsError>) -> Self {
        Self::DescribeSnapshotsError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeVolumesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeVolumesError>) -> Self {
        Self::DescribeVolumesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeSpotInstanceRequestsError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeSpotInstanceRequestsError>) -> Self {
        Self::DescribeSpotInstanceRequestsError(Box::new(value))
    }
}

impl From<AwsSdkError<RunInstancesError>> for AwslibError {
    fn from(value: AwsSdkError<RunInstancesError>) -> Self {
        Self::RunInstancesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeRepositoriesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeRepositoriesError>) -> Self {
        Self::DescribeRepositoriesError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeEcrImagesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeEcrImagesError>) -> Self {
        Self::DescribeEcrImagesError(Box::new(value))
    }
}

impl From<AwsSdkError<BatchDeleteImageError>> for AwslibError {
    fn from(value: AwsSdkError<BatchDeleteImageError>) -> Self {
        Self::BatchDeleteImageError(Box::new(value))
    }
}

impl From<AwsSdkError<ListUsersError>> for AwslibError {
    fn from(value: AwsSdkError<ListUsersError>) -> Self {
        Self::ListUsersError(Box::new(value))
    }
}

impl From<AwsSdkError<GetUserError>> for AwslibError {
    fn from(value: AwsSdkError<GetUserError>) -> Self {
        Self::GetUserError(Box::new(value))
    }
}

impl From<AwsSdkError<ListGroupsError>> for AwslibError {
    fn from(value: AwsSdkError<ListGroupsError>) -> Self {
        Self::ListGroupsError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteUserError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteUserError>) -> Self {
        Self::DeleteUserError(Box::new(value))
    }
}

impl From<AwsSdkError<AddUserToGroupError>> for AwslibError {
    fn from(value: AwsSdkError<AddUserToGroupError>) -> Self {
        Self::AddUserToGroupError(Box::new(value))
    }
}

impl From<AwsSdkError<RemoveUserFromGroupError>> for AwslibError {
    fn from(value: AwsSdkError<RemoveUserFromGroupError>) -> Self {
        Self::RemoveUserFromGroupError(Box::new(value))
    }
}

impl From<AwsSdkError<ListAccessKeysError>> for AwslibError {
    fn from(value: AwsSdkError<ListAccessKeysError>) -> Self {
        Self::ListAccessKeysError(Box::new(value))
    }
}

impl From<AwsSdkError<ListGroupsForUserError>> for AwslibError {
    fn from(value: AwsSdkError<ListGroupsForUserError>) -> Self {
        Self::ListGroupsForUserError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateUserError>> for AwslibError {
    fn from(value: AwsSdkError<CreateUserError>) -> Self {
        Self::CreateUserError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateAccessKeyError>> for AwslibError {
    fn from(value: AwsSdkError<CreateAccessKeyError>) -> Self {
        Self::CreateAccessKeyError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteAccessKeyError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteAccessKeyError>) -> Self {
        Self::DeleteAccessKeyError(Box::new(value))
    }
}

impl From<AwsSdkError<GetCallerIdentityError>> for AwslibError {
    fn from(value: AwsSdkError<GetCallerIdentityError>) -> Self {
        Self::GetCallerIdentityError(Box::new(value))
    }
}

impl From<AwsSdkError<DescribeServicesError>> for AwslibError {
    fn from(value: AwsSdkError<DescribeServicesError>) -> Self {
        Self::DescribeServicesError(Box::new(value))
    }
}

impl From<AwsSdkError<GetAttributeValuesError>> for AwslibError {
    fn from(value: AwsSdkError<GetAttributeValuesError>) -> Self {
        Self::GetAttributeValuesError(Box::new(value))
    }
}

impl From<AwsSdkError<GetProductsError>> for AwslibError {
    fn from(value: AwsSdkError<GetProductsError>) -> Self {
        Self::GetProductsError(Box::new(value))
    }
}

impl From<AwsSdkError<ListHostedZonesError>> for AwslibError {
    fn from(value: AwsSdkError<ListHostedZonesError>) -> Self {
        Self::ListHostedZonesError(Box::new(value))
    }
}

impl From<AwsSdkError<ListResourceRecordSetsError>> for AwslibError {
    fn from(value: AwsSdkError<ListResourceRecordSetsError>) -> Self {
        Self::ListResourceRecordSetsError(Box::new(value))
    }
}

impl From<AwsSdkError<ChangeResourceRecordSetsError>> for AwslibError {
    fn from(value: AwsSdkError<ChangeResourceRecordSetsError>) -> Self {
        Self::ChangeResourceRecordSetsError(Box::new(value))
    }
}

impl From<AwsSdkError<ListBucketsError>> for AwslibError {
    fn from(value: AwsSdkError<ListBucketsError>) -> Self {
        Self::ListBucketsError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteBucketError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteBucketError>) -> Self {
        Self::DeleteBucketError(Box::new(value))
    }
}

impl From<AwsSdkError<DeleteObjectError>> for AwslibError {
    fn from(value: AwsSdkError<DeleteObjectError>) -> Self {
        Self::DeleteObjectError(Box::new(value))
    }
}

impl From<AwsSdkError<CopyObjectError>> for AwslibError {
    fn from(value: AwsSdkError<CopyObjectError>) -> Self {
        Self::CopyObjectError(Box::new(value))
    }
}

impl From<AwsSdkError<CreateBucketError>> for AwslibError {
    fn from(value: AwsSdkError<CreateBucketError>) -> Self {
        Self::CreateBucketError(Box::new(value))
    }
}

impl From<AwsSdkError<SendEmailError>> for AwslibError {
    fn from(value: AwsSdkError<SendEmailError>) -> Self {
        Self::SendEmailError(Box::new(value))
    }
}

impl From<AwsSdkError<GetSendStatisticsError>> for AwslibError {
    fn from(value: AwsSdkError<GetSendStatisticsError>) -> Self {
        Self::GetSendStatisticsError(Box::new(value))
    }
}

impl From<AwsSdkError<GetSendQuotaError>> for AwslibError {
    fn from(value: AwsSdkError<GetSendQuotaError>) -> Self {
        Self::GetSendQuotaError(Box::new(value))
    }
}

impl From<PqError> for AwslibError {
    fn from(value: PqError) -> Self {
        Self::PqError(Box::new(value))
    }
}

impl From<PqExtractError> for AwslibError {
    fn from(value: PqExtractError) -> Self {
        Self::PqExtractError(Box::new(value))
    }
}

impl From<TimeParseError> for AwslibError {
    fn from(value: TimeParseError) -> Self {
        Self::TimeParseError(Box::new(value))
    }
}

impl From<TimeComponentRangeError> for AwslibError {
    fn from(value: TimeComponentRangeError) -> Self {
        Self::TimeComponentRangeError(Box::new(value))
    }
}

impl From<RoXmlTreeError> for AwslibError {
    fn from(value: RoXmlTreeError) -> Self {
        Self::RoXmlTreeError(Box::new(value))
    }
}

impl From<FromUtf8Error> for AwslibError {
    fn from(value: FromUtf8Error) -> Self {
        Self::FromUtf8Error(Box::new(value))
    }
}

impl From<AwsBuildError> for AwslibError {
    fn from(value: AwsBuildError) -> Self {
        Self::AwsBuildError(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use aws_sdk_ec2::operation::{
        attach_volume::AttachVolumeError,
        cancel_spot_instance_requests::CancelSpotInstanceRequestsError,
        create_image::CreateImageError, create_snapshot::CreateSnapshotError,
        create_tags::CreateTagsError, create_volume::CreateVolumeError,
        delete_snapshot::DeleteSnapshotError, delete_volume::DeleteVolumeError,
        deregister_image::DeregisterImageError,
        describe_availability_zones::DescribeAvailabilityZonesError,
        describe_images::DescribeImagesError, describe_instances::DescribeInstancesError,
        describe_key_pairs::DescribeKeyPairsError, describe_regions::DescribeRegionsError,
        describe_reserved_instances::DescribeReservedInstancesError,
        describe_snapshots::DescribeSnapshotsError,
        describe_spot_instance_requests::DescribeSpotInstanceRequestsError,
        describe_spot_price_history::DescribeSpotPriceHistoryError,
        describe_volumes::DescribeVolumesError, detach_volume::DetachVolumeError,
        modify_volume::ModifyVolumeError, request_spot_instances::RequestSpotInstancesError,
        run_instances::RunInstancesError, terminate_instances::TerminateInstancesError,
    };
    use aws_sdk_ecr::operation::{
        batch_delete_image::BatchDeleteImageError,
        describe_images::DescribeImagesError as DescribeEcrImagesError,
        describe_repositories::DescribeRepositoriesError,
    };
    use aws_sdk_iam::operation::{
        add_user_to_group::AddUserToGroupError, create_access_key::CreateAccessKeyError,
        create_user::CreateUserError, delete_access_key::DeleteAccessKeyError,
        delete_user::DeleteUserError, get_user::GetUserError,
        list_access_keys::ListAccessKeysError, list_groups::ListGroupsError,
        list_groups_for_user::ListGroupsForUserError, list_users::ListUsersError,
        remove_user_from_group::RemoveUserFromGroupError,
    };
    use aws_sdk_pricing::operation::{
        describe_services::DescribeServicesError, get_attribute_values::GetAttributeValuesError,
        get_products::GetProductsError,
    };
    use aws_sdk_route53::operation::{
        change_resource_record_sets::ChangeResourceRecordSetsError,
        list_hosted_zones::ListHostedZonesError,
        list_resource_record_sets::ListResourceRecordSetsError,
    };
    use aws_sdk_s3::operation::{
        copy_object::CopyObjectError, create_bucket::CreateBucketError,
        delete_bucket::DeleteBucketError, delete_object::DeleteObjectError,
        get_object::GetObjectError, list_buckets::ListBucketsError, list_objects::ListObjectsError,
        put_object::PutObjectError,
    };
    use aws_sdk_ses::operation::{
        get_send_quota::GetSendQuotaError, get_send_statistics::GetSendStatisticsError,
        send_email::SendEmailError,
    };
    use aws_sdk_sts::operation::get_caller_identity::GetCallerIdentityError;
    use aws_smithy_types::{
        byte_stream::error::Error as AwsByteStreamError,
        error::operation::BuildError as AwsBuildError,
    };
    use deadpool_postgres::{BuildError as DeadpoolBuildError, ConfigError as DeadpoolConfigError};
    use envy::Error as EnvyError;
    use postgres_query::{Error as PqError, extract::Error as PqExtractError};
    use rand::distr::uniform::Error as RandUniformError;
    use refinery::Error as RefineryError;
    use reqwest::Error as ReqwestError;
    use roxmltree::Error as RoXmlTreeError;
    use serde_json::Error as SerdeJsonError;
    use std::{
        net::AddrParseError,
        num::{ParseFloatError, ParseIntError},
        str::Utf8Error,
        string::FromUtf8Error,
    };
    use stdout_channel::StdoutChannelError;
    use time::error::{
        ComponentRange as TimeComponentRangeError, Format as TimeFormatError,
        Parse as TimeParseError,
    };
    use tokio::task::JoinError;
    use tokio_postgres::error::Error as TokioPostgresError;
    use url::ParseError as UrlParseError;
    use zip::result::ZipError;

    use crate::errors::{AwsSdkError, AwslibError, DeadPoolError};

    #[test]
    fn test_error_size() {
        println!(
            "RandUniformError {}",
            std::mem::size_of::<RandUniformError>()
        );
        println!(
            "AwsByteStreamError {}",
            std::mem::size_of::<AwsByteStreamError>()
        );
        println!(
            "DeadpoolBuildError {}",
            std::mem::size_of::<DeadpoolBuildError>()
        );
        println!(
            "DeadpoolConfigError {}",
            std::mem::size_of::<DeadpoolConfigError>()
        );
        println!(
            "TokioPostgresError {}",
            std::mem::size_of::<TokioPostgresError>()
        );
        println!("PqError {}", std::mem::size_of::<PqError>());
        println!("PqExtractError {}", std::mem::size_of::<PqExtractError>());
        println!("EnvyError {}", std::mem::size_of::<EnvyError>());
        println!("TimeParseError {}", std::mem::size_of::<TimeParseError>());
        println!("TimeFormatError {}", std::mem::size_of::<TimeFormatError>());
        println!(
            "TimeComponentRangeError {}",
            std::mem::size_of::<TimeComponentRangeError>()
        );
        println!("std::io::Error {}", std::mem::size_of::<std::io::Error>());
        println!("Utf8Error {}", std::mem::size_of::<Utf8Error>());
        println!("JoinError {}", std::mem::size_of::<JoinError>());
        println!("ZipError {}", std::mem::size_of::<ZipError>());
        println!("RoXmlTreeError {}", std::mem::size_of::<RoXmlTreeError>());
        println!("FromUtf8Error {}", std::mem::size_of::<FromUtf8Error>());
        println!("DeadPoolError {}", std::mem::size_of::<DeadPoolError>());
        println!(
            "StdoutChannelError {}",
            std::mem::size_of::<StdoutChannelError>()
        );
        println!("RefineryError {}", std::mem::size_of::<RefineryError>());
        println!("UrlParseError {}", std::mem::size_of::<UrlParseError>());
        println!("ParseFloatError {}", std::mem::size_of::<ParseFloatError>());
        println!("ParseIntError {}", std::mem::size_of::<ParseIntError>());
        println!("ReqwestError {}", std::mem::size_of::<ReqwestError>());
        println!("AddrParseError {}", std::mem::size_of::<AddrParseError>());
        println!("AwsBuildError {}", std::mem::size_of::<AwsBuildError>());
        println!("SerdeJsonError {}", std::mem::size_of::<SerdeJsonError>());

        println!(
            "ListObjectsError {}",
            std::mem::size_of::<AwsSdkError<ListObjectsError>>()
        );
        println!(
            "GetObjectError {}",
            std::mem::size_of::<AwsSdkError<GetObjectError>>()
        );
        println!(
            "PutObjectError {}",
            std::mem::size_of::<AwsSdkError<PutObjectError>>()
        );
        println!(
            "DescribeImagesError {}",
            std::mem::size_of::<AwsSdkError<DescribeImagesError>>()
        );
        println!(
            "DescribeRegionsError {}",
            std::mem::size_of::<AwsSdkError<DescribeRegionsError>>()
        );
        println!(
            "DescribeInstancesError {}",
            std::mem::size_of::<AwsSdkError<DescribeInstancesError>>()
        );
        println!(
            "DescribeReservedInstancesError {}",
            std::mem::size_of::<AwsSdkError<DescribeReservedInstancesError>>()
        );
        println!(
            "DescribeAvailabilityZonesError {}",
            std::mem::size_of::<AwsSdkError<DescribeAvailabilityZonesError>>()
        );
        println!(
            "DescribeKeyPairsError {}",
            std::mem::size_of::<AwsSdkError<DescribeKeyPairsError>>()
        );
        println!(
            "DescribeSpotPriceHistoryError {}",
            std::mem::size_of::<AwsSdkError<DescribeSpotPriceHistoryError>>()
        );
        println!(
            "DeleteSnapshotError {}",
            std::mem::size_of::<AwsSdkError<DeleteSnapshotError>>()
        );
        println!(
            "CreateSnapshotError {}",
            std::mem::size_of::<AwsSdkError<CreateSnapshotError>>()
        );
        println!(
            "ModifyVolumeError {}",
            std::mem::size_of::<AwsSdkError<ModifyVolumeError>>()
        );
        println!(
            "DetachVolumeError {}",
            std::mem::size_of::<AwsSdkError<DetachVolumeError>>()
        );
        println!(
            "AttachVolumeError {}",
            std::mem::size_of::<AwsSdkError<AttachVolumeError>>()
        );
        println!(
            "DeleteVolumeError {}",
            std::mem::size_of::<AwsSdkError<DeleteVolumeError>>()
        );
        println!(
            "CreateVolumeError {}",
            std::mem::size_of::<AwsSdkError<CreateVolumeError>>()
        );
        println!(
            "DeregisterImageError {}",
            std::mem::size_of::<AwsSdkError<DeregisterImageError>>()
        );
        println!(
            "CreateImageError {}",
            std::mem::size_of::<AwsSdkError<CreateImageError>>()
        );
        println!(
            "CreateTagsError {}",
            std::mem::size_of::<AwsSdkError<CreateTagsError>>()
        );
        println!(
            "CancelSpotInstanceRequestsError {}",
            std::mem::size_of::<AwsSdkError<CancelSpotInstanceRequestsError>>()
        );
        println!(
            "RequestSpotInstancesError {}",
            std::mem::size_of::<AwsSdkError<RequestSpotInstancesError>>()
        );
        println!(
            "TerminateInstancesError {}",
            std::mem::size_of::<AwsSdkError<TerminateInstancesError>>()
        );
        println!(
            "DescribeSnapshotsError {}",
            std::mem::size_of::<AwsSdkError<DescribeSnapshotsError>>()
        );
        println!(
            "DescribeVolumesError {}",
            std::mem::size_of::<AwsSdkError<DescribeVolumesError>>()
        );
        println!(
            "DescribeSpotInstanceRequestsError {}",
            std::mem::size_of::<AwsSdkError<DescribeSpotInstanceRequestsError>>()
        );
        println!(
            "RunInstancesError {}",
            std::mem::size_of::<AwsSdkError<RunInstancesError>>()
        );
        println!(
            "DescribeRepositoriesError {}",
            std::mem::size_of::<AwsSdkError<DescribeRepositoriesError>>()
        );
        println!(
            "DescribeEcrImagesError {}",
            std::mem::size_of::<AwsSdkError<DescribeEcrImagesError>>()
        );
        println!(
            "BatchDeleteImageError {}",
            std::mem::size_of::<AwsSdkError<BatchDeleteImageError>>()
        );
        println!(
            "ListUsersError {}",
            std::mem::size_of::<AwsSdkError<ListUsersError>>()
        );
        println!(
            "GetUserError {}",
            std::mem::size_of::<AwsSdkError<GetUserError>>()
        );
        println!(
            "ListGroupsError {}",
            std::mem::size_of::<AwsSdkError<ListGroupsError>>()
        );
        println!(
            "DeleteUserError {}",
            std::mem::size_of::<AwsSdkError<DeleteUserError>>()
        );
        println!(
            "AddUserToGroupError {}",
            std::mem::size_of::<AwsSdkError<AddUserToGroupError>>()
        );
        println!(
            "RemoveUserFromGroupError {}",
            std::mem::size_of::<AwsSdkError<RemoveUserFromGroupError>>()
        );
        println!(
            "ListAccessKeysError {}",
            std::mem::size_of::<AwsSdkError<ListAccessKeysError>>()
        );
        println!(
            "ListGroupsForUserError {}",
            std::mem::size_of::<AwsSdkError<ListGroupsForUserError>>()
        );
        println!(
            "CreateUserError {}",
            std::mem::size_of::<AwsSdkError<CreateUserError>>()
        );
        println!(
            "CreateAccessKeyError {}",
            std::mem::size_of::<AwsSdkError<CreateAccessKeyError>>()
        );
        println!(
            "DeleteAccessKeyError {}",
            std::mem::size_of::<AwsSdkError<DeleteAccessKeyError>>()
        );
        println!(
            "GetCallerIdentityError {}",
            std::mem::size_of::<AwsSdkError<GetCallerIdentityError>>()
        );
        println!(
            "DescribeServicesError {}",
            std::mem::size_of::<AwsSdkError<DescribeServicesError>>()
        );
        println!(
            "GetAttributeValuesError {}",
            std::mem::size_of::<AwsSdkError<GetAttributeValuesError>>()
        );
        println!(
            "GetProductsError {}",
            std::mem::size_of::<AwsSdkError<GetProductsError>>()
        );
        println!(
            "ListHostedZonesError {}",
            std::mem::size_of::<AwsSdkError<ListHostedZonesError>>()
        );
        println!(
            "ListResourceRecordSetsError {}",
            std::mem::size_of::<AwsSdkError<ListResourceRecordSetsError>>()
        );
        println!(
            "ChangeResourceRecordSetsError {}",
            std::mem::size_of::<AwsSdkError<ChangeResourceRecordSetsError>>()
        );
        println!(
            "ListBucketsError {}",
            std::mem::size_of::<AwsSdkError<ListBucketsError>>()
        );
        println!(
            "DeleteBucketError {}",
            std::mem::size_of::<AwsSdkError<DeleteBucketError>>()
        );
        println!(
            "DeleteObjectError {}",
            std::mem::size_of::<AwsSdkError<DeleteObjectError>>()
        );
        println!(
            "CopyObjectError {}",
            std::mem::size_of::<AwsSdkError<CopyObjectError>>()
        );
        println!(
            "CreateBucketError {}",
            std::mem::size_of::<AwsSdkError<CreateBucketError>>()
        );
        println!(
            "SendEmailError {}",
            std::mem::size_of::<AwsSdkError<SendEmailError>>()
        );
        println!(
            "GetSendStatisticsError {}",
            std::mem::size_of::<AwsSdkError<GetSendStatisticsError>>()
        );
        println!(
            "GetSendQuotaError {}",
            std::mem::size_of::<AwsSdkError<GetSendQuotaError>>()
        );

        println!("AwslibError {}", std::mem::size_of::<AwslibError>());
        assert_eq!(std::mem::size_of::<AwslibError>(), 32);
    }
}
