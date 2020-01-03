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
    pub max_spot_price: f32,
    pub default_security_group: String,
    pub spot_security_group: String,
    pub default_key_name: String,
    pub script_directory: String,
    pub ubuntu_release: String,
    pub port: u32,
    pub secret_key: String,
    pub domain: String,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Self {
        Self::default()
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
        let max_spot_price: f32 = var("MAX_SPOT_PRICE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.20);
        let default_security_group: String = var("DEFAULT_SECURITY_GROUP")
            .map_err(|e| format_err!("DEFAULT_SECURITY_GROUP mut be set {}", e))?;
        let spot_security_group: String =
            var("SPOT_SECURITY_GROUP").unwrap_or_else(|_| default_security_group.clone());
        let default_key_name: String = var("DEFAULT_KEY_NAME")
            .map_err(|e| format_err!("DEFAULT_KEY_NAME mut be set {}", e))?;
        let script_directory: String = var("SCRIPT_DIRECTORY")
            .unwrap_or_else(|_| format!("{}/.config/aws_app_rust/scripts", home_dir));
        let ubuntu_release: String =
            var("UBUNTU_RELEASE").unwrap_or_else(|_| "bionic-18.04".to_string());
        let port = var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3096);
        let secret_key = var("SECRET_KEY").unwrap_or_else(|_| "0123".repeat(8));
        let domain = var("DOMAIN").unwrap_or_else(|_| "localhost".to_string());

        let conf = ConfigInner {
            database_url,
            aws_region_name,
            my_owner_id,
            max_spot_price,
            default_security_group,
            spot_security_group,
            default_key_name,
            script_directory,
            ubuntu_release,
            port,
            secret_key,
            domain,
        };

        Ok(Self(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
