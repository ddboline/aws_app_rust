use aws_app_rust::models::AwsGeneration;
use aws_app_rust::scrape_instance_info::scrape_instance_info;

fn main() {
    let body = scrape_instance_info(AwsGeneration::PV).unwrap();
}
