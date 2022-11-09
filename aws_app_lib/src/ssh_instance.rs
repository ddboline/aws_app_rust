use anyhow::{format_err, Error};
use lazy_static::lazy_static;
use log::debug;
use stack_string::{format_sstr, StackString};
use std::collections::HashMap;
use tokio::{
    process::Command,
    sync::{Mutex, RwLock},
};

lazy_static! {
    static ref LOCK_CACHE: RwLock<HashMap<StackString, Mutex<()>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub struct SSHInstance {
    pub user: StackString,
    pub host: StackString,
    pub port: u16,
}

impl SSHInstance {
    pub async fn new(
        user: impl Into<StackString>,
        host: impl Into<StackString>,
        port: u16,
    ) -> Self {
        let host = host.into();
        LOCK_CACHE
            .write()
            .await
            .insert(host.clone(), Mutex::new(()));
        Self {
            user: user.into(),
            host,
            port,
        }
    }

    #[must_use]
    pub fn get_ssh_username_host(&self) -> StackString {
        if self.port == 22 {
            format_sstr!("{}@{}", self.user, self.host)
        } else {
            format_sstr!("-p {} {}@{}", self.port, self.user, self.host)
        }
    }

    /// # Errors
    /// Returns error if stdout is not utf8
    pub async fn run_command_stream_stdout(
        &self,
        cmd: impl AsRef<str>,
    ) -> Result<Vec<StackString>, Error> {
        let cmd = cmd.as_ref();
        if let Some(host_lock) = LOCK_CACHE.read().await.get(&self.host) {
            let _lock = host_lock.lock().await;
            debug!("cmd {}", cmd);
            let user_host = self.get_ssh_username_host();

            let output = Command::new("ssh")
                .args([&user_host, "--"])
                .args(cmd.split_whitespace())
                .kill_on_drop(true)
                .output()
                .await?;
            let output = StackString::from_utf8_vec(output.stdout)?;
            let output: Vec<_> = output.split('\n').map(Into::into).collect();
            Ok(output)
        } else {
            Err(format_err!("Failed to acquire lock"))
        }
    }
}
