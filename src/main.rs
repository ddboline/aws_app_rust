use aws_app_lib::aws_app_opts::AwsAppOpts;

fn main() {
    env_logger::init();
    AwsAppOpts::process_args().unwrap();
}
