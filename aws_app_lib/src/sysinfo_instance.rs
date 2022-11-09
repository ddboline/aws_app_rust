use parking_lot::Mutex;
use stack_string::StackString;
use std::{collections::BTreeSet, fmt, sync::Arc};
use sysinfo::{PidExt, Process, ProcessExt, System, SystemExt};

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: StackString,
    pub cpu_usage: f32,
    pub memory: u64,
    pub read_disk_bytes: u64,
    pub write_disk_bytes: u64,
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "pid:{pid} name:{name} cpu_usage:{cpu_usage} memory:{memory} \
             read_disk_bytes:{read_disk_bytes} write_dist_bytes:{write_disk_bytes}",
            pid = self.pid,
            name = self.name,
            cpu_usage = self.cpu_usage,
            memory = self.memory,
            read_disk_bytes = self.read_disk_bytes,
            write_disk_bytes = self.write_disk_bytes,
        )
    }
}

impl From<&Process> for ProcessInfo {
    fn from(proc: &Process) -> Self {
        let disk_usage = proc.disk_usage();
        let read_disk_bytes = disk_usage.total_read_bytes;
        let write_disk_bytes = disk_usage.total_written_bytes;
        Self {
            pid: proc.pid().as_u32(),
            name: proc.name().into(),
            cpu_usage: proc.cpu_usage(),
            memory: proc.memory(),
            read_disk_bytes,
            write_disk_bytes,
        }
    }
}

#[derive(Clone)]
pub struct SysinfoInstance {
    system: Arc<Mutex<System>>,
    process_names: Arc<BTreeSet<StackString>>,
}

impl SysinfoInstance {
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = impl Into<StackString>>) -> Self {
        let process_names = names.into_iter().map(Into::into).collect();
        let process_names = Arc::new(process_names);
        let mut sys = System::default();
        sys.refresh_processes();
        let system = Arc::new(Mutex::new(sys));
        Self {
            system,
            process_names,
        }
    }

    #[must_use]
    pub fn get_process_info(&self) -> Vec<ProcessInfo> {
        let mut sys = self.system.lock();
        sys.refresh_processes();
        self.process_names
            .iter()
            .flat_map(|name| {
                let name = if name.len() > 15 { &name[..15] } else { name };
                sys.processes_by_name(name).map(Into::into)
            })
            .collect()
    }

    #[must_use]
    pub fn get_process_info_by_name(&self, name: &str) -> Vec<ProcessInfo> {
        let name = if name.len() > 15 { &name[..15] } else { name };
        let mut sys = self.system.lock();
        sys.refresh_processes();
        sys.processes_by_name(name).map(Into::into).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::sysinfo_instance::SysinfoInstance;
    use anyhow::Error;

    #[test]
    fn test_sysinfo_instance() -> Result<(), Error> {
        let names = vec!["cargo"];

        let sys_instance = SysinfoInstance::new(names);
        let procs = sys_instance.get_process_info();
        assert_eq!(procs.len(), 1);
        for proc in procs {
            println!("{proc}");
        }
        Ok(())
    }
}
