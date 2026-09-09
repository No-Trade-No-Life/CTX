use std::{
    io,
    path::{Path, PathBuf},
    time::Instant,
};

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, Networks, Pid, ProcessesToUpdate, System};
use thiserror::Error;

use crate::db::{Database, DatabaseError};

#[derive(Clone, Debug, Serialize)]
pub struct SystemResourcesSnapshot {
    pub sampled_at: i64,
    pub sample_interval_ms: u64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub network: NetworkSnapshot,
    pub disk: Option<DiskSnapshot>,
    pub sqlite: SqliteSnapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct CpuSnapshot {
    pub usage_percent: f32,
    pub load_1m: f64,
    pub logical_cpus: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemorySnapshot {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub process_used_bytes: u64,
    pub other_used_bytes: u64,
    pub usage_percent: f64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NetworkSnapshot {
    pub receive_bytes_per_second: u64,
    pub transmit_bytes_per_second: u64,
    pub interfaces: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiskSnapshot {
    pub mount_point: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SqliteSnapshot {
    pub main_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub total_bytes: u64,
    pub freelist_bytes: u64,
    pub freelist_percent: f64,
}

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("the system resource sample could not read SQLite")]
    Database(#[from] DatabaseError),
    #[error("the system resource sample could not read a database file")]
    Io(#[from] io::Error),
    #[error("the system resource monitor is unavailable")]
    Poisoned,
}

pub struct ResourceMonitor {
    database: Database,
    database_path: PathBuf,
    system: System,
    disks: Disks,
    networks: Networks,
    last_sample: Instant,
}

impl ResourceMonitor {
    pub fn new(database: Database) -> Self {
        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_list(CpuRefreshKind::nothing().with_cpu_usage());
        let database_path = database.database_path();
        Self {
            database,
            database_path,
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last_sample: Instant::now(),
        }
    }

    pub fn sample(&mut self) -> Result<SystemResourcesSnapshot, ResourceError> {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        let process_id = Pid::from_u32(std::process::id());
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[process_id]), true);
        self.disks.refresh(true);
        self.networks.refresh(true);

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample);
        self.last_sample = now;
        let sample_seconds = elapsed
            .max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL)
            .as_secs_f64();
        let received = self
            .networks
            .values()
            .map(sysinfo::NetworkData::received)
            .sum::<u64>();
        let transmitted = self
            .networks
            .values()
            .map(sysinfo::NetworkData::transmitted)
            .sum::<u64>();
        let used_memory = self.system.used_memory();
        let total_memory = self.system.total_memory();
        let process_used_bytes = self
            .system
            .process(process_id)
            .map(sysinfo::Process::memory)
            .unwrap_or_default();

        Ok(SystemResourcesSnapshot {
            sampled_at: chrono::Utc::now().timestamp(),
            sample_interval_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            cpu: CpuSnapshot {
                usage_percent: self.system.global_cpu_usage(),
                load_1m: System::load_average().one,
                logical_cpus: self.system.cpus().len(),
            },
            memory: MemorySnapshot {
                used_bytes: used_memory,
                total_bytes: total_memory,
                available_bytes: self.system.available_memory(),
                process_used_bytes,
                other_used_bytes: used_memory.saturating_sub(process_used_bytes),
                usage_percent: percentage(used_memory, total_memory),
                swap_used_bytes: self.system.used_swap(),
                swap_total_bytes: self.system.total_swap(),
            },
            network: NetworkSnapshot {
                receive_bytes_per_second: rate(received, sample_seconds),
                transmit_bytes_per_second: rate(transmitted, sample_seconds),
                interfaces: self.networks.len(),
            },
            disk: disk_usage(&self.disks, &self.database_path),
            sqlite: sqlite_usage(&self.database, &self.database_path)?,
        })
    }
}

fn disk_usage(disks: &Disks, database_path: &Path) -> Option<DiskSnapshot> {
    let disk = disks
        .list()
        .iter()
        .filter(|disk| database_path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())?;
    let total_bytes = disk.total_space();
    let available_bytes = disk.available_space();
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    Some(DiskSnapshot {
        mount_point: disk.mount_point().to_string_lossy().into_owned(),
        used_bytes,
        total_bytes,
        available_bytes,
        usage_percent: percentage(used_bytes, total_bytes),
    })
}

fn sqlite_usage(
    database: &Database,
    database_path: &Path,
) -> Result<SqliteSnapshot, ResourceError> {
    let page_usage = database.sqlite_page_usage()?;
    let mut snapshot = sqlite_file_usage(database_path)?;
    snapshot.freelist_bytes = page_usage
        .page_size
        .saturating_mul(page_usage.freelist_count);
    snapshot.freelist_percent = percentage(page_usage.freelist_count, page_usage.page_count);
    Ok(snapshot)
}

fn sqlite_file_usage(database_path: &Path) -> io::Result<SqliteSnapshot> {
    let main_bytes = file_size(database_path)?;
    let wal_bytes = file_size(&with_suffix(database_path, "-wal"))?;
    let shm_bytes = file_size(&with_suffix(database_path, "-shm"))?;
    Ok(SqliteSnapshot {
        main_bytes,
        wal_bytes,
        shm_bytes,
        total_bytes: main_bytes
            .saturating_add(wal_bytes)
            .saturating_add(shm_bytes),
        freelist_bytes: 0,
        freelist_percent: 0.0,
    })
}

fn file_size(path: &Path) -> io::Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

fn percentage(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

fn rate(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds).round() as u64
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ResourceMonitor, sqlite_file_usage};
    use crate::db::Database;

    #[test]
    fn samples_host_resources_and_sqlite_storage() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let mut monitor = ResourceMonitor::new(database);
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL + Duration::from_millis(1));

        let sample = monitor.sample().unwrap();

        assert!(sample.cpu.logical_cpus > 0);
        assert!(sample.memory.total_bytes > 0);
        assert_eq!(
            sample.memory.other_used_bytes,
            sample
                .memory
                .used_bytes
                .saturating_sub(sample.memory.process_used_bytes)
        );
        assert!(sample.sampled_at > 0);
    }

    #[test]
    fn sqlite_usage_includes_wal_and_shared_memory_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ctx.sqlite3");
        std::fs::write(&path, [0_u8; 11]).unwrap();
        std::fs::write(format!("{}-wal", path.display()), [0_u8; 7]).unwrap();
        std::fs::write(format!("{}-shm", path.display()), [0_u8; 3]).unwrap();

        let usage = sqlite_file_usage(&path).unwrap();

        assert_eq!(usage.main_bytes, 11);
        assert_eq!(usage.wal_bytes, 7);
        assert_eq!(usage.shm_bytes, 3);
        assert_eq!(usage.total_bytes, 21);
    }
}
