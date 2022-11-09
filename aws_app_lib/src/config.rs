use anyhow::Error;
use lazy_static::lazy_static;
use serde::Deserialize;
use stack_string::StackString;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

lazy_static! {
    static ref CONFIG_DIR: PathBuf = dirs::config_dir().expect("No CONFIG directory");
}

#[derive(Default, Debug, Deserialize, PartialEq)]
pub struct ConfigInner {
    #[serde(default = "default_database_url")]
    pub database_url: StackString,
    #[serde(default = "default_aws_region_name")]
    pub aws_region_name: StackString,
    pub my_owner_id: Option<StackString>,
    #[serde(default = "default_max_spot_price")]
    pub max_spot_price: f32,
    pub default_security_group: Option<StackString>,
    pub spot_security_group: Option<StackString>,
    pub default_key_name: Option<StackString>,
    #[serde(default = "default_script_directory")]
    pub script_directory: PathBuf,
    #[serde(default = "default_ubuntu_release")]
    pub ubuntu_release: StackString,
    #[serde(default = "default_host")]
    pub host: StackString,
    #[serde(default = "default_port")]
    pub port: u32,
    #[serde(default = "default_domain")]
    pub domain: StackString,
    pub novnc_path: Option<PathBuf>,
    #[serde(default = "default_secret_path")]
    pub secret_path: PathBuf,
    #[serde(default = "default_secret_path")]
    pub jwt_secret_path: PathBuf,
    #[serde(default = "Vec::new")]
    pub systemd_services: Vec<StackString>,
}

fn default_database_url() -> StackString {
    "postgresql://user:password@host:1234/test_db".into()
}
fn default_aws_region_name() -> StackString {
    "us-east-1".into()
}
fn default_max_spot_price() -> f32 {
    0.20
}
fn default_script_directory() -> PathBuf {
    CONFIG_DIR.join("aws_app_rust").join("scripts")
}
fn default_ubuntu_release() -> StackString {
    "bionic-18.04".into()
}
fn default_host() -> StackString {
    "0.0.0.0".into()
}
fn default_port() -> u32 {
    3096
}
fn default_domain() -> StackString {
    "localhost".into()
}
fn default_secret_path() -> PathBuf {
    CONFIG_DIR.join("aws_app_rust").join("secret.bin")
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Config(Arc<ConfigInner>);

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Config {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
    }

    /// # Errors
    /// Returns error if deserialize from environment variables fails
    pub fn init_config() -> Result<Self, Error> {
        let fname = Path::new("config.env");
        let default_fname = CONFIG_DIR.join("aws_app_rust").join("config.env");

        let env_file = if fname.exists() {
            fname
        } else {
            &default_fname
        };

        dotenv::dotenv().ok();

        if env_file.exists() {
            dotenv::from_path(env_file).ok();
        }

        let conf: ConfigInner = envy::from_env()?;

        Ok(Self(Arc::new(conf)))
    }
}
