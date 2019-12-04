use aws_app_rust::config::Config;
use aws_app_rust::ec2_instance::Ec2Instance;
use aws_app_rust::pgpool::PgPool;
// use aws_app_rust::models::{AwsGeneration, PricingType};
// use aws_app_rust::scrape_instance_info::scrape_instance_info;
// use aws_app_rust::scrape_pricing_info::scrape_pricing_info;

fn main() {
    let config = Config::init_config().unwrap();
    let pool = PgPool::new(&config.database_url);

    // scrape_instance_info(AwsGeneration::HVM, &pool).unwrap();
    // scrape_instance_info(AwsGeneration::PV, &pool).unwrap();

    // scrape_pricing_info(PricingType::Reserved, &pool).unwrap();
    // scrape_pricing_info(PricingType::OnDemand, &pool).unwrap();

    let ec2 = Ec2Instance::new("us-east-1");

    let ami_tags = ec2.get_ami_tags(&config).unwrap();
    let regions = ec2.get_all_regions().unwrap();
    let instances = ec2.get_all_instances().unwrap();
    let reserved = ec2.get_reserved_instances().unwrap();
    let prices = ec2.get_latest_spot_inst_prices().unwrap();
    println!("{:?}", ami_tags);
    println!("{:?}", regions);
    println!("{:?}", instances);
    println!("{:?}", reserved);
    println!("{:?}", prices);
}
