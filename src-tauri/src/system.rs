//! What the machine is doing, for the status bar: processor, memory and disk.

use serde::Serialize;
use std::path::Path;
use sysinfo::{Disks, System};

/// A reading of the machine. Sizes are bytes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    /// Processor use since the reading before, all cores together: 0 to 100.
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// The disk the user's folder is on; `None` when the system does not tell.
    pub disk: Option<Disk>,
}

#[derive(Debug, Serialize)]
pub struct Disk {
    pub free: u64,
    pub total: u64,
}

/// A `System` whose next reading can already tell the processor use: that figure is what
/// happened since the refresh before. `None` on a system that cannot be read.
pub fn start() -> Option<System> {
    let mut system = System::new().ok()?;
    system.refresh_cpu_usage();
    Some(system)
}

pub fn read(system: &mut System, home: &Path) -> Reading {
    system.refresh_cpu_usage();
    system.refresh_memory();
    Reading {
        cpu: system.global_cpu_usage().clamp(0.0, 100.0),
        memory_used: system.used_memory(),
        memory_total: system.total_memory(),
        disk: disk_of(home),
    }
}

/// The disk `folder` is on: the one mounted the deepest above it.
fn disk_of(folder: &Path) -> Option<Disk> {
    let disks = Disks::new_with_refreshed_list().ok()?;
    let disk = disks
        .iter()
        .filter(|disk| folder.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())?;
    Some(Disk {
        free: disk.available_space(),
        total: disk.total_space(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the machine the tests run on.
    #[test]
    fn a_reading_holds_together() {
        let mut system = start().unwrap();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let home = std::env::home_dir().unwrap();
        let reading = read(&mut system, &home);
        assert!((0.0..=100.0).contains(&reading.cpu), "{reading:?}");
        assert!(
            reading.memory_total > 0 && reading.memory_used <= reading.memory_total,
            "{reading:?}"
        );
        if let Some(disk) = &reading.disk {
            assert!(disk.total > 0 && disk.free <= disk.total, "{disk:?}");
        }
        println!("{reading:?}");
    }
}
