use anyhow::Error;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use tokio::process::Command;

#[derive(Default, Clone)]
pub struct SystemdInstance {
    services: BTreeSet<StackString>,
}

impl SystemdInstance {
    pub fn new(services: &[impl AsRef<str>]) -> Self {
        let services = services.iter().map(AsRef::as_ref).map(Into::into).collect();
        Self { services }
    }

    pub async fn list_running_services(&self) -> Result<BTreeMap<StackString, RunStatus>, Error> {
        let command = Command::new("systemctl")
            .args(&["list-units"])
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

    pub async fn get_service_status(&self, service: &str) -> Result<ServiceStatus, Error> {
        let command = Command::new("systemctl")
            .args(&["show", service])
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

    pub async fn get_service_logs(&self, service: &str) -> Result<Vec<ServiceLogEntry>, Error> {
        let command = Command::new("journalctl")
            .args(&["-b", "-u", service, "-o", "json"])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&command.stdout);
        stdout
            .split('\n')
            .filter(|line| {
                line.contains(r#""UNIT""#) && line.contains("_SOURCE_REALTIME_TIMESTAMP")
            })
            .map(|line| {
                let log: ServiceLogLine = serde_json::from_str(line)?;
                log.try_into()
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RunStatus {
    Running,
    NotRunning,
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::NotRunning => write!(f, "not running"),
        }
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
    #[serde(alias = "_SOURCE_REALTIME_TIMESTAMP")]
    timestamp: &'a str,
    #[serde(alias = "UNIT")]
    unit: StackString,
    #[serde(alias = "MESSAGE")]
    message: StackString,
    #[serde(alias = "_HOSTNAME")]
    hostname: StackString,
}

impl TryFrom<ServiceLogLine<'_>> for ServiceLogEntry {
    type Error = Error;
    fn try_from(line: ServiceLogLine) -> Result<Self, Self::Error> {
        let timestamp: u64 = line.timestamp.parse().map_err(|e| {
            println!("{}", line.timestamp);
            e
        })?;
        let timestamp = NaiveDateTime::from_timestamp(
            (timestamp / 1_000_000) as i64,
            (timestamp % 1_000_000) as u32,
        );
        let timestamp = DateTime::from_utc(timestamp, Utc);
        Ok(Self {
            timestamp,
            unit: line.unit,
            message: line.message,
            hostname: line.hostname,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceLogEntry {
    timestamp: DateTime<Utc>,
    unit: StackString,
    message: StackString,
    hostname: StackString,
}

impl fmt::Display for ServiceLogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.timestamp, self.unit, self.hostname, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::systemd_instance::SystemdInstance;

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
