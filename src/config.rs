use failure::{err_msg, format_err, Error};
use std::env::var;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

#[derive(Default, Debug)]
pub struct ConfigInner {
    pub database_url: String,
    pub aws_region_name: String,
    pub my_owner_id: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_inner(self) -> Result<ConfigInner, Error> {
        Arc::try_unwrap(self.0).map_err(|_| err_msg("Failed unwrapping"))
    }

    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
    }

    pub fn init_config() -> Result<Self, Error> {
        let fname = "config.env";

        let home_dir = var("HOME").map_err(|e| format_err!("No HOME directory {}", e))?;

        let default_fname = format!("{}/.config/aws_app_rust/config.env", home_dir);

        let env_file = if Path::new(fname).exists() {
            fname.to_string()
        } else {
            default_fname
        };

        dotenv::dotenv().ok();

        if Path::new(&env_file).exists() {
            dotenv::from_path(&env_file).ok();
        } else if Path::new("config.env").exists() {
            dotenv::from_filename("config.env").ok();
        }

        let database_url =
            var("DATABASE_URL").map_err(|e| format_err!("DATABASE_URL must be set {}", e))?;
        let aws_region_name = var("AWS_REGION_NAME").unwrap_or_else(|_| "us-east-1".to_string());
        let my_owner_id = var("MY_OWNER_ID").ok();

        let conf = ConfigInner {
            database_url,
            aws_region_name,
            my_owner_id,
        };

        Ok(Config(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
