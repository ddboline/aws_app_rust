use anyhow::{format_err, Error};
use std::{env::var, ops::Deref, path::Path, sync::Arc};

use crate::stack_string::StackString;

#[derive(Default, Debug)]
pub struct ConfigInner {
    pub database_url: StackString,
    pub aws_region_name: StackString,
    pub my_owner_id: Option<StackString>,
    pub max_spot_price: f32,
    pub default_security_group: StackString,
    pub spot_security_group: StackString,
    pub default_key_name: StackString,
    pub script_directory: StackString,
    pub ubuntu_release: StackString,
    pub port: u32,
    pub secret_key: StackString,
    pub domain: StackString,
    pub novnc_path: Option<StackString>,
}

macro_rules! set_config_ok {
    ($s:ident, $id:ident) => {
        $s.$id = var(&stringify!($id).to_uppercase()).ok().map(Into::into);
    };
}

macro_rules! set_config_parse {
    ($s:ident, $id:ident, $d:expr) => {
        $s.$id = var(&stringify!($id).to_uppercase()).ok().and_then(|x| x.parse().ok()).unwrap_or_else(|| $d);
    };
}

macro_rules! set_config_must {
    ($s:ident, $id:ident) => {
        $s.$id = var(&stringify!($id).to_uppercase()).map(Into::into)
            .map_err(|e| format_err!("{} must be set: {}", stringify!($id).to_uppercase(), e))?;
    };
}

macro_rules! set_config_default {
    ($s:ident, $id:ident, $d:expr) => {
        $s.$id = var(&stringify!($id).to_uppercase()).map_or_else(|_| $d, Into::into);
    };
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
        let config_dir = dirs::config_dir().ok_or_else(|| format_err!("No CONFIG directory"))?;
        let default_fname = config_dir.join("aws_app_rust").join("config.env");

        let env_file = if fname.exists() {
            fname
        } else {
            &default_fname
        };

        dotenv::dotenv().ok();

        if env_file.exists() {
            dotenv::from_path(env_file).ok();
        }

        let mut conf = ConfigInner::default();

        set_config_must!(conf, database_url);
        set_config_must!(conf, default_security_group);
        set_config_must!(conf, default_key_name);

        set_config_default!(conf, aws_region_name, "us-east-1".into());
        set_config_default!(
            conf,
            spot_security_group,
            conf.default_security_group.clone()
        );
        set_config_default!(
            conf,
            script_directory,
            config_dir
                .join("aws_app_rust")
                .join("scripts")
                .to_string_lossy().to_string()
                .into()
        );
        set_config_default!(conf, ubuntu_release, "bionic-18.04".into());
        set_config_default!(conf, secret_key, "0123".repeat(8).into());
        set_config_default!(conf, domain, "localhost".into());

        set_config_ok!(conf, my_owner_id);
        set_config_parse!(conf, max_spot_price, 0.20);
        set_config_parse!(conf, port, 3096);
        set_config_ok!(conf, novnc_path);

        Ok(Self(Arc::new(conf)))
    }
}
