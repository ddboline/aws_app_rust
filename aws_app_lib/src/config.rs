use anyhow::Error;
use serde::Deserialize;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::stack_string::StackString;

#[derive(Default, Debug, Deserialize)]
pub struct ConfigInner {
    pub database_url: StackString,
    #[serde(default = "default_aws_region_name")]
    pub aws_region_name: StackString,
    pub my_owner_id: Option<StackString>,
    #[serde(default = "default_max_spot_price")]
    pub max_spot_price: f32,
    pub default_security_group: StackString,
    pub spot_security_group: Option<StackString>,
    pub default_key_name: StackString,
    #[serde(default = "default_script_directory")]
    pub script_directory: PathBuf,
    #[serde(default = "default_ubuntu_release")]
    pub ubuntu_release: StackString,
    #[serde(default = "default_port")]
    pub port: u32,
    #[serde(default = "default_secret_key")]
    pub secret_key: StackString,
    #[serde(default = "default_domain")]
    pub domain: StackString,
    pub novnc_path: Option<PathBuf>,
}

fn config_dir() -> PathBuf {
    dirs::config_dir().expect("No CONFIG directory")
}
fn default_aws_region_name() -> StackString {
    "us-east-1".into()
}
fn default_max_spot_price() -> f32 {
    0.20
}
fn default_script_directory() -> PathBuf {
    config_dir().join("aws_app_rust").join("scripts")
}
fn default_ubuntu_release() -> StackString {
    "bionic-18.04".into()
}
fn default_port() -> u32 {
    3096
}
fn default_secret_key() -> StackString {
    "0123".repeat(8).into()
}
fn default_domain() -> StackString {
    "localhost".into()
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
    }

    pub fn init_config() -> Result<Self, Error> {
        let fname = Path::new("config.env");
        let default_fname = config_dir().join("aws_app_rust").join("config.env");

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
