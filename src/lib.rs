#[macro_use]
extern crate diesel;

pub mod aws_app_interface;
pub mod aws_app_opts;
pub mod config;
pub mod ec2_instance;
pub mod ecr_instance;
pub mod models;
pub mod pgpool;
pub mod resource_type;
pub mod schema;
pub mod scrape_instance_info;
pub mod scrape_pricing_info;
