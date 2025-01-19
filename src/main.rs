use anyhow::Error;

use aws_app_lib::aws_app_opts::AwsAppOpts;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    tokio::spawn(async move { AwsAppOpts::process_args().await }).await?
}
