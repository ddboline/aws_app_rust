use failure::Error;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use structopt::StructOpt;

use crate::aws_app_interface::AwsAppInterface;
use crate::config::Config;
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;

#[derive(StructOpt, Debug, Clone)]
pub enum AwsAppOpts {
    Update,
    List {
        #[structopt(short = "r")]
        /// Possible values are: "reserved", "spot", "ami", "volume", "snapshot", "ecr"
        resources: Vec<ResourceType>,
        #[structopt(long)]
        /// List all regions
        all_regions: bool,
    },
    Terminate {
        #[structopt(short = "i", long)]
        /// Instance IDs
        instance_ids: Vec<String>,
    },
    Request {
        ami: String,
        script: String,
        instance_type: String,
        tags: Vec<String>,
    },
}

impl AwsAppOpts {
    pub fn process_args() -> Result<(), Error> {
        let opts = AwsAppOpts::from_args();
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let app = AwsAppInterface::new(config, pool);

        match opts {
            AwsAppOpts::Update => app.update(),
            AwsAppOpts::List {
                resources,
                all_regions,
            } => {
                if all_regions {
                    let regions: Vec<_> = app.ec2.get_all_regions()?.keys().cloned().collect();
                    let results: Result<(), Error> = regions
                        .par_iter()
                        .map(|region| {
                            let mut app_ = app.clone();
                            app_.set_region(&region)?;
                            app_.list(&resources)
                        })
                        .collect();
                    results
                } else {
                    app.list(&resources)
                }
            }
            AwsAppOpts::Terminate { instance_ids } => app.ec2.terminate_instance(&instance_ids),
            AwsAppOpts::Request {
                ami,
                script,
                instance_type,
                tags,
            } => Ok(())
        }
    }
}
