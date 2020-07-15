#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::used_underscore_binding)]

#[macro_use]
extern crate diesel;

pub mod aws_app_interface;
pub mod aws_app_opts;
pub mod config;
pub mod ec2_instance;
pub mod ecr_instance;
pub mod instance_family;
pub mod models;
pub mod pgpool;
pub mod resource_type;
pub mod schema;
pub mod scrape_instance_info;
pub mod scrape_pricing_info;
pub mod ssh_instance;
pub mod stdout_channel;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
