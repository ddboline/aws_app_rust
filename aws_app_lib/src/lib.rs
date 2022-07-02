#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::default_trait_access)]

pub mod aws_app_interface;
pub mod aws_app_opts;
pub mod config;
pub mod date_time_wrapper;
pub mod ec2_instance;
pub mod ecr_instance;
pub mod iam_instance;
pub mod instance_family;
pub mod instance_opt;
pub mod models;
pub mod novnc_instance;
pub mod pgpool;
pub mod pricing_instance;
pub mod resource_type;
pub mod route53_instance;
pub mod scrape_instance_info;
pub mod scrape_pricing_info;
pub mod spot_request_opt;
pub mod ssh_instance;
pub mod sysinfo_instance;
pub mod systemd_instance;

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use sysinfo::{ProcessExt, System, SystemExt};

    #[test]
    fn test_process() -> Result<(), Error> {
        let mut sys = System::default();
        sys.refresh_processes();
        let procs = sys.processes_by_name("aws-app-http");

        for proc in procs {
            println!(
                "pid:{} name:{} cpu:{} memory:{} read:{} write:{}",
                proc.pid(),
                proc.name(),
                proc.cpu_usage(),
                proc.memory(),
                proc.disk_usage().total_read_bytes,
                proc.disk_usage().total_written_bytes,
            );
        }
        assert!(false);
        Ok(())
    }
}
