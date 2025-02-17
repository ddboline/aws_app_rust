#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::cast_possible_wrap)]

pub mod aws_app_interface;
pub mod aws_app_opts;
pub mod config;
pub mod date_time_wrapper;
pub mod ec2_instance;
pub mod ecr_instance;
pub mod errors;
pub mod iam_instance;
pub mod inbound_email;
pub mod instance_family;
pub mod instance_opt;
pub mod models;
pub mod novnc_instance;
pub mod pgpool;
pub mod pricing_instance;
pub mod resource_type;
pub mod route53_instance;
pub mod s3_instance;
pub mod scrape_instance_info;
pub mod scrape_pricing_info;
pub mod ses_client;
pub mod spot_request_opt;
pub mod ssh_instance;
pub mod sysinfo_instance;
pub mod systemd_instance;

use rand::{
    distr::{Distribution, Uniform},
    rng as thread_rng,
};
use std::{convert::TryFrom, future::Future};
use tokio::time::{sleep, Duration};

use crate::errors::AwslibError as Error;

/// # Errors
/// Returns error if timeout is reached
pub async fn exponential_retry<T, U, F>(f: T) -> Result<U, Error>
where
    T: Fn() -> F,
    F: Future<Output = Result<U, Error>>,
{
    let mut timeout: f64 = 1.0;
    let range = Uniform::try_from(0..1000)?;
    loop {
        match f().await {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                sleep(Duration::from_millis((timeout * 1000.0) as u64)).await;
                timeout *= 4.0 * f64::from(range.sample(&mut thread_rng())) / 1000.0;
                if timeout >= 64.0 {
                    return Err(err);
                }
            }
        }
    }
}
