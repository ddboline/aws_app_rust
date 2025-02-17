use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::{TryFrom, TryInto},
    fmt,
};
use time::{Duration, OffsetDateTime, UtcOffset};
use tokio::process::Command;

use crate::{date_time_wrapper::DateTimeWrapper, errors::AwslibError as Error};

#[derive(Default, Clone)]
pub struct SystemdInstance {
    services: BTreeSet<StackString>,
}

impl SystemdInstance {
    pub fn new(services: &[impl AsRef<str>]) -> Self {
        let services = services.iter().map(AsRef::as_ref).map(Into::into).collect();
        Self { services }
    }

    /// # Errors
    /// Returns error if spawn of systemctl fails
    pub async fn list_running_services(&self) -> Result<BTreeMap<StackString, RunStatus>, Error> {
        let command = Command::new("systemctl")
            .args(["list-units"])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&command.stdout);
        let mut services: BTreeMap<_, _> = stdout
            .split('\n')
            .filter_map(|line| {
                if let Some(service) = line
                    .split_whitespace()
                    .next()
                    .and_then(|x| x.split('.').next())
                {
                    if self.services.contains(service) {
                        return Some((service.into(), RunStatus::Running));
                    }
                }
                None
            })
            .collect();
        for service in &self.services {
            if !services.contains_key(service) {
                services.insert(service.clone(), RunStatus::NotRunning);
            }
        }
        Ok(services)
    }

    /// # Errors
    /// Return error if spawn of systemctl fails
    pub async fn get_service_status(
        &self,
        service: impl AsRef<str>,
    ) -> Result<ServiceStatus, Error> {
        let command = Command::new("systemctl")
            .args(["show", service.as_ref()])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&command.stdout);
        let mut status = ServiceStatus::default();
        for (key, val) in stdout.split('\n').filter_map(|line| {
            let mut iter = line.split('=');
            let key = iter.next()?;
            let val = iter.next()?;
            Some((key, val))
        }) {
            match key {
                "ActiveState" => status.active_state = val.into(),
                "SubState" => status.sub_state = val.into(),
                "LoadState" => status.load_state = val.into(),
                "MainPID" => status.main_pid = val.parse().ok(),
                "TasksCurrent" => status.tasks = val.parse().ok(),
                "MemoryCurrent" => status.memory = val.parse().ok(),
                _ => (),
            }
        }
        Ok(status)
    }

    /// # Errors
    /// Return error if
    ///     * spawn of journalctl fails
    ///     * parsing of log line fails
    pub async fn get_service_logs(
        &self,
        service: impl AsRef<str>,
    ) -> Result<Vec<ServiceLogEntry>, Error> {
        let command = Command::new("journalctl")
            .args([
                "-b",
                "-u",
                service.as_ref(),
                "-o",
                "json",
                "-n",
                "100",
                "-r",
            ])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&command.stdout);
        stdout
            .split('\n')
            .filter(|line| line.contains("__REALTIME_TIMESTAMP"))
            .map(|line| {
                let log: ServiceLogLine = serde_json::from_str(line)?;
                log.try_into()
            })
            .collect()
    }

    /// # Errors
    /// Returns error if spawn of systemctl fails
    pub async fn service_action(
        &self,
        action: impl AsRef<str>,
        service: impl AsRef<str>,
    ) -> Result<StackString, Error> {
        let command = Command::new("sudo")
            .args(["systemctl", action.as_ref(), service.as_ref()])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&command.stdout);
        Ok(stdout.as_ref().into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    NotRunning,
}

impl RunStatus {
    #[must_use]
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::NotRunning => "not running",
        }
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServiceStatus {
    active_state: StackString,
    sub_state: StackString,
    load_state: StackString,
    main_pid: Option<u64>,
    tasks: Option<u64>,
    memory: Option<u64>,
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {} {} {}",
            self.active_state,
            self.sub_state,
            self.load_state,
            self.main_pid.unwrap_or(0),
            self.tasks.unwrap_or(0),
            self.memory.unwrap_or(0)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceLogLine<'a> {
    #[serde(alias = "__REALTIME_TIMESTAMP")]
    timestamp: &'a str,
    #[serde(alias = "MESSAGE")]
    message: StackString,
    #[serde(alias = "_HOSTNAME")]
    hostname: StackString,
}

impl TryFrom<ServiceLogLine<'_>> for ServiceLogEntry {
    type Error = Error;
    fn try_from(line: ServiceLogLine) -> Result<Self, Self::Error> {
        let timestamp: i64 = line.timestamp.parse().inspect_err(|e| {
            println!("{e} {}", line.timestamp);
        })?;
        let ts = timestamp / 1_000_000;
        let ns = (timestamp % 1_000_000) * 1000;
        let timestamp = (OffsetDateTime::from_unix_timestamp(ts)
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            + Duration::nanoseconds(ns))
        .to_offset(UtcOffset::UTC)
        .into();
        Ok(Self {
            timestamp,
            message: line.message,
            hostname: line.hostname,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceLogEntry {
    timestamp: DateTimeWrapper,
    message: StackString,
    hostname: StackString,
}

impl fmt::Display for ServiceLogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.timestamp, self.hostname, self.message)
    }
}

#[cfg(test)]
mod tests {
    use crate::{errors::AwslibError as Error, systemd_instance::SystemdInstance};

    #[tokio::test]
    #[ignore]
    async fn test_systemd_list() -> Result<(), Error> {
        let systemd = SystemdInstance::new(&["aws-app-http", "auth-server-rust"]);
        let services = systemd.list_running_services().await?;
        println!("{:#?}", services);
        assert_eq!(services.len(), 2);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_service_status() -> Result<(), Error> {
        let systemd = SystemdInstance::new(&["aws-app-http", "auth-server-rust"]);
        let status = systemd.get_service_status("aws-app-http").await?;
        assert_eq!(status.active_state, "active");
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_service_logs() -> Result<(), Error> {
        let systemd = SystemdInstance::new(&["aws-app-http", "auth-server-rust"]);
        let logs = systemd.get_service_logs("aws-app-http").await?;
        assert!(logs.len() > 0);
        Ok(())
    }
}
