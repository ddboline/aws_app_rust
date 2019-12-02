use reqwest::Url;

use aws_app_rust::models::AwsGeneration;
use aws_app_rust::scrape_instance_info::scrape_instance_info;

fn main() {
    let url: Url = "https://aws.amazon.com/ec2/instance-types/"
        .parse()
        .unwrap();
    let body = scrape_instance_info(url, AwsGeneration::HVM).unwrap();
}
