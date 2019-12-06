use aws_app_rust::aws_app_opts::AwsAppOpts;

fn main() {
    AwsAppOpts::process_args().unwrap();

    // use aws_app_rust::config::Config;
    // use aws_app_rust::ec2_instance::Ec2Instance;
    // use aws_app_rust::ecr_instance::EcrInstance;
    // use aws_app_rust::models::InstanceList;
    // use aws_app_rust::pgpool::PgPool;
    // use aws_app_rust::models::{AwsGeneration, PricingType};
    // use aws_app_rust::scrape_instance_info::scrape_instance_info;
    // use aws_app_rust::scrape_pricing_info::scrape_pricing_info;

    // let config = Config::init_config().unwrap();

    // let pool = PgPool::new(&config.database_url);

    // scrape_instance_info(AwsGeneration::HVM, &pool).unwrap();
    // scrape_instance_info(AwsGeneration::PV, &pool).unwrap();

    // scrape_pricing_info(PricingType::Reserved, &pool).unwrap();
    // scrape_pricing_info(PricingType::OnDemand, &pool).unwrap();

    // let ec2 = Ec2Instance::new(config.clone());
    // let ecr = EcrInstance::new(config);

    // let ami_tags = ec2.get_ami_tags().unwrap();
    // let regions = ec2.get_all_regions().unwrap();
    // let instances = ec2.get_all_instances().unwrap();
    // let reserved = ec2.get_reserved_instances().unwrap();

    // let inst_list: Vec<_> = InstanceList::get_all_instances(&pool)
    //     .unwrap()
    //     .into_iter()
    //     .filter(|inst| inst.instance_type.contains("m5."))
    //     .map(|inst| inst.instance_type.to_string())
    //     .collect();

    // let prices = ec2.get_latest_spot_inst_prices(&inst_list).unwrap();

    // let requests = ec2.get_spot_instance_requests().unwrap();

    // let volumes = ec2.get_all_volumes().unwrap();

    // let snapshots = ec2.get_all_snapshots().unwrap();

    // println!("{:?}", ami_tags);
    // println!("{:?}", regions);
    // println!("{:?}", instances);
    // println!("{:?}", reserved);
    // println!("{:?}", prices);
    // println!("{:?}", requests);
    // println!("{:?}", volumes);
    // println!("{:?}", snapshots);

    // let repos = ecr.get_all_repositories().unwrap();

    // for repo in repos {
    //     let images = ecr.get_all_images(&repo).unwrap();
    //     println!("{} {:?}", repo, images);
    // }
}
