use serde::Deserialize;
use stack_string::StackString;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use crate::errors::AwslibError as Error;

static CONFIG_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| dirs::config_dir().expect("No CONFIG directory"));
static HOME_DIR: LazyLock<PathBuf> = LazyLock::new(|| dirs::home_dir().expect("No HOME directory"));

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
    pub novnc_cert_path: Option<PathBuf>,
    pub novnc_key_path: Option<PathBuf>,
    #[serde(default = "default_secret_path")]
    pub secret_path: PathBuf,
    #[serde(default = "default_secret_path")]
    pub jwt_secret_path: PathBuf,
    #[serde(default = "Vec::new")]
    pub systemd_services: Vec<StackString>,
    #[serde(default = "default_root_crontab")]
    pub root_crontab: PathBuf,
    #[serde(default = "default_user_crontab")]
    pub user_crontab: PathBuf,
    pub inbound_email_bucket: Option<StackString>,
}

fn default_user_crontab() -> PathBuf {
    HOME_DIR.join("crontab.log")
}
fn default_root_crontab() -> PathBuf {
    Path::new("/tmp").join("crontab_root.log")
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

#[derive(Default, Debug, PartialEq)]
pub struct Config(Arc<ConfigInner>);

impl Clone for Config {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

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

        dotenvy::dotenv().ok();

        if env_file.exists() {
            dotenvy::from_path(env_file).ok();
        }

        let conf: ConfigInner = envy::from_env()?;

        Ok(Self(Arc::new(conf)))
    }
}
