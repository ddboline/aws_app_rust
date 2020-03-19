use anyhow::{format_err, Error};
use lazy_static::lazy_static;
use log::debug;
use std::collections::HashMap;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};

lazy_static! {
    static ref LOCK_CACHE: RwLock<HashMap<String, Mutex<()>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub struct SSHInstance {
    pub user: String,
    pub host: String,
    pub port: u16,
}

impl SSHInstance {
    pub async fn new(user: &str, host: &str, port: u16) -> Self {
        LOCK_CACHE.write().await.insert(host.into(), Mutex::new(()));
        Self {
            user: user.into(),
            host: host.into(),
            port,
        }
    }

    pub fn get_ssh_username_host(&self) -> Result<String, Error> {
        let ssh_str = if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("-p {} {}@{}", self.port, self.user, self.host)
        };

        Ok(ssh_str)
    }

    pub async fn run_command_stream_stdout(&self, cmd: &str) -> Result<Vec<String>, Error> {
        if let Some(host_lock) = LOCK_CACHE.read().await.get(&self.host) {
            let _ = host_lock.lock().await;
            debug!("cmd {}", cmd);
            let user_host = self.get_ssh_username_host()?;

            let output = Command::new("ssh")
                .args(&[&user_host, "--"])
                .args(cmd.split_whitespace())
                .kill_on_drop(true)
                .output()
                .await?;
            let output = String::from_utf8(output.stdout)?;
            let output: Vec<_> = output.split('\n').map(ToString::to_string).collect();
            Ok(output)
        } else {
            Err(format_err!("Failed to acquire lock"))
        }
    }
}
