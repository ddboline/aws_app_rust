#[macro_use]
extern crate diesel;

pub mod config;
pub mod ec2_instance;
pub mod models;
pub mod pgpool;
pub mod schema;
pub mod scrape_instance_info;
pub mod scrape_pricing_info;
