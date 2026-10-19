//! What the game server itself uses, for the dashboard.
//!
//! The server runs inside the app on its own thread (`bh-server`, see `blockheads_server::host`). CPU: on Linux and
//! Android that thread's CPU time from `/proc/self/task`; elsewhere the whole process (threads are not told apart
//! there). Memory cannot be split by thread, so it is the process's resident memory above what the app used before
//! any server ran (sampled when the meter is created, at app start).

use serde::Serialize;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Name of the game server's thread.
const SERVER_THREAD: &str = "bh-server";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Share of the machine's CPU the server used since the previous sample, 0-100.
    pub cpu: f32,
    /// Memory the server holds, bytes.
    pub mem: u64,
    /// True when `cpu` is the server thread alone, false when it is the whole app process.
    pub thread_cpu: bool,
}

struct State {
    sys: System,
    /// Server thread CPU time (seconds) and when it was read.
    last: Option<(f64, Instant)>,
}

/// Samples [`Stats`]; CPU use is measured between two samples, so the first one reads 0.
pub struct Meter {
    state: Mutex<State>,
    pid: Option<Pid>,
    cores: usize,
    /// Resident memory of the app before any server ran.
    baseline: u64,
}

impl Default for Meter {
    fn default() -> Self {
        Meter::new(sysinfo::get_current_pid().ok())
    }
}

impl Meter {
    fn new(pid: Option<Pid>) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let mut m = Meter { state: Mutex::new(State { sys: System::new(), last: None }), pid, cores, baseline: 0 };
        m.baseline = m.process().1;
        m
    }

    /// CPU use (percent of one core) and resident memory of the whole process.
    fn process(&self) -> (f32, u64) {
        let Some(pid) = self.pid else { return (0.0, 0) };
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
        st.sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
        st.sys.process(pid).map_or((0.0, 0), |p| (p.cpu_usage(), p.memory()))
    }

    pub fn sample(&self) -> Stats {
        let (process_cpu, rss) = self.process();
        let mem = rss.saturating_sub(self.baseline);
        let now = Instant::now();
        let (cpu, thread_cpu) = match thread_cpu_seconds(SERVER_THREAD) {
            Some(secs) => {
                let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                let pct = match st.last {
                    Some((prev, at)) if now > at => {
                        (secs - prev).max(0.0) / now.duration_since(at).as_secs_f64() * 100.0
                    }
                    _ => 0.0,
                };
                st.last = Some((secs, now));
                (pct as f32, true)
            }
            None => (process_cpu, false),
        };
        Stats { cpu: (cpu / self.cores as f32).clamp(0.0, 100.0), mem, thread_cpu }
    }
}

/// Total CPU time (user + system, seconds) of this process's threads called `name`; `None` where threads cannot be
/// read (not Linux/Android).
#[cfg(any(target_os = "linux", target_os = "android"))]
fn thread_cpu_seconds(name: &str) -> Option<f64> {
    // SAFETY: sysconf only reads a configuration value.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let tasks = std::fs::read_dir("/proc/self/task").ok()?;
    let ticks: u64 = tasks
        .flatten()
        .filter(|t| std::fs::read_to_string(t.path().join("comm")).is_ok_and(|c| c.trim_end() == name))
        .filter_map(|t| std::fs::read_to_string(t.path().join("stat")).ok())
        .filter_map(|s| stat_ticks(&s))
        .sum();
    Some(ticks as f64 / hz)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn thread_cpu_seconds(_name: &str) -> Option<f64> {
    None
}

/// utime + stime (fields 14 and 15) of a `/proc/.../stat` line; the name in parentheses may contain spaces.
#[cfg_attr(not(any(target_os = "linux", target_os = "android")), allow(dead_code))]
fn stat_ticks(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // after the name the state is field 3, so utime (14) and stime (15) are at 11 and 12 here
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burn() {
        let mut x = 0u64;
        for i in 0..30_000_000u64 {
            x = x.wrapping_add(i * i);
        }
        std::hint::black_box(x);
    }

    #[test]
    fn measures_the_server_thread_not_the_rest_of_the_app() {
        let m = Meter::default();
        let worker = std::thread::Builder::new()
            .name(SERVER_THREAD.into())
            .spawn(move || {
                m.sample();
                burn();
                let s = m.sample();
                // memory allocated after the baseline counts
                let v = vec![1u8; 32 << 20];
                std::hint::black_box(&v);
                (s, m.sample())
            })
            .unwrap();
        burn(); // CPU on another thread: not the server's
        let (busy, after_alloc) = worker.join().unwrap();
        assert!(busy.thread_cpu);
        assert!(busy.cpu > 0.0 && busy.cpu <= 100.0, "{busy:?}");
        assert!(after_alloc.mem >= 16 << 20, "{after_alloc:?}");
    }

    #[test]
    fn without_a_server_thread_or_pid_reads_zero() {
        let m = Meter::new(None);
        let s = m.sample();
        assert_eq!((s.cpu, s.mem), (0.0, 0));
        assert_eq!(thread_cpu_seconds("no-such-thread"), Some(0.0));
    }

    #[test]
    fn reads_cpu_ticks_from_a_stat_line() {
        assert_eq!(stat_ticks("12 (bh server) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0"), Some(300));
        assert_eq!(stat_ticks("garbage"), None);
        assert_eq!(stat_ticks("1 (x) S 1"), None);
    }
}
