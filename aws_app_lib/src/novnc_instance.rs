use log::debug;
use stack_string::StackString;
use std::{path::Path, process::Stdio, sync::Arc};
use tokio::{
    process::{Child, Command},
    sync::RwLock,
};

use crate::errors::AwslibError as Error;

#[derive(Default, Clone)]
pub struct NoVncInstance {
    children: Arc<RwLock<Vec<Child>>>,
}

impl NoVncInstance {
    #[must_use]
    pub fn new() -> Self {
        Self {
            children: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// # Errors
    /// Returns error if spawn process fails
    pub async fn novnc_start(
        &self,
        novnc_path: &Path,
        cert: &Path,
        key: &Path,
    ) -> Result<(), Error> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| Error::StaticCustomError("No home directory"))?;
        let x11vnc = Path::new("/usr/bin/x11vnc");
        // let vncserver = Path::new("/usr/bin/vncserver");
        let vncpwd = home_dir.join(".vnc/passwd");
        let websockify = Path::new("/usr/bin/websockify");

        if !x11vnc.exists() {
            debug!("no x11vnc");
        }
        if !websockify.exists() {
            debug!("no websockify");
        };
        if !vncpwd.exists() {
            debug!("no vncpwd");
        }
        if !cert.exists() {
            debug!("no cert {cert:?}");
        }
        if !key.exists() {
            debug!("no key {key:?}");
        }

        if !x11vnc.exists()
            || !websockify.exists()
            || !vncpwd.exists()
            || !cert.exists()
            || !key.exists()
        {
            return Err(Error::StaticCustomError("Missing needed file(s)"));
        }

        let x11vnc_command = Command::new(x11vnc)
            .args([
                "-safer",
                "-rfbauth",
                &vncpwd.to_string_lossy(),
                "-forever",
                "-display",
                ":0",
            ])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let websockify_command = Command::new("sudo")
            .args([
                &websockify.to_string_lossy(),
                "8787",
                "--ssl-only",
                "--web",
                novnc_path.to_string_lossy().as_ref(),
                "--cert",
                &cert.to_string_lossy(),
                "--key",
                &key.to_string_lossy(),
                "localhost:5900",
            ])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut children = self.children.write().await;
        children.push(x11vnc_command);
        children.push(websockify_command);
        Ok(())
    }

    /// # Errors
    /// Returns error
    ///     * if `get_websock_pids` fails
    ///     * if spawn fails
    ///     * if `wait_with_output` fails
    ///     * if `StackString::from_utf8_vec` fails
    pub async fn novnc_stop_request(&self) -> Result<Vec<StackString>, Error> {
        let mut children = self.children.write().await;
        for child in children.iter_mut() {
            if let Err(e) = child.kill().await {
                debug!("Failed to kill {}", e);
            }
        }

        let mut kill = Command::new("sudo");
        kill.args(["kill", "-9"]);
        let ids = self
            .get_websock_pids()
            .await?
            .into_iter()
            .map(StackString::from_display);
        kill.args(ids);
        let kill = kill.spawn()?;
        kill.wait_with_output().await?;

        let mut output = Vec::new();
        while let Some(mut child) = children.pop() {
            if let Err(e) = child.kill().await {
                debug!("Failed to kill {}", e);
            }
            let result = child.wait_with_output().await?;
            output.push(StackString::from_utf8_vec(result.stdout)?);
            output.push(StackString::from_utf8_vec(result.stderr)?);
        }
        children.clear();
        Ok(output)
    }

    /// # Errors
    /// Returns error
    ///     * if spawn fails
    ///     * if `wait_with_output` fails
    ///     * if `StackString::from_utf8_vec` fails
    pub async fn get_websock_pids(&self) -> Result<Vec<usize>, Error> {
        let websock = Command::new("ps")
            .args(["-eF"])
            .stdout(Stdio::piped())
            .spawn()?;
        let output = websock.wait_with_output().await?;
        let output = StackString::from_utf8_vec(output.stdout)?;
        let result: Vec<_> = output
            .split('\n')
            .filter_map(|s| {
                if s.contains("websockify") {
                    s.split_whitespace().nth(1).and_then(|x| x.parse().ok())
                } else {
                    None
                }
            })
            .collect();
        Ok(result)
    }

    pub async fn get_novnc_status(&self) -> usize {
        self.children.read().await.len()
    }
}
