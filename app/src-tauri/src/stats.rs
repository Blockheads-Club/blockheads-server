//! Resource use of this process for the dashboard. The game server runs inside the app, so the process's CPU and
//! memory are the server's (plus the little the UI host needs).

use serde::Serialize;
use std::sync::{Mutex, PoisonError};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Share of the whole machine's CPU time since the previous sample, 0-100.
    pub cpu: f32,
    /// Logical CPUs of the machine.
    pub cores: usize,
    /// Resident memory of this process, bytes.
    pub mem: u64,
    /// Physical memory of the machine, bytes.
    pub mem_total: u64,
}

/// Samples [`Stats`]; CPU use is measured between two samples, so the first one reads 0.
pub struct Meter {
    sys: Mutex<System>,
    pid: Option<Pid>,
    cores: usize,
}

impl Default for Meter {
    fn default() -> Self {
        Meter::new(sysinfo::get_current_pid().ok())
    }
}

impl Meter {
    fn new(pid: Option<Pid>) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        Meter { sys: Mutex::new(System::new()), pid, cores }
    }

    pub fn sample(&self) -> Stats {
        let mut sys = self.sys.lock().unwrap_or_else(PoisonError::into_inner);
        sys.refresh_memory();
        let (cpu, mem) = match self.pid {
            Some(pid) => {
                let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
                sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
                sys.process(pid).map_or((0.0, 0), |p| (p.cpu_usage(), p.memory()))
            }
            None => (0.0, 0),
        };
        Stats { cpu: (cpu / self.cores as f32).clamp(0.0, 100.0), cores: self.cores, mem, mem_total: sys.total_memory() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_this_process() {
        let m = Meter::default();
        m.sample();
        // burn a little CPU so the second sample has something to measure
        let mut x = 0u64;
        for i in 0..20_000_000u64 {
            x = x.wrapping_add(i * i);
        }
        std::hint::black_box(x);
        let s = m.sample();
        assert!(s.mem > 0 && s.mem_total >= s.mem, "{s:?}");
        assert!(s.cores >= 1);
        assert!((0.0..=100.0).contains(&s.cpu));
    }

    #[test]
    fn without_a_pid_reads_zero_use() {
        let s = Meter::new(None).sample();
        assert_eq!((s.cpu, s.mem), (0.0, 0));
        assert!(s.mem_total > 0);
    }
}
