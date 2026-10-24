//! Our server in-process (`blockheads_server::host`) behind the [`ServerHandle`] interface, so the scenario
//! scripts run against the real production wiring without docker or a separately built binary.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use blockheads_server::host::{self, HostConfig, Status, WorldSelect};

use super::refenv::{RunSpec, ServerHandle};
use super::{free_udp_port, strip_nslog_prefix, HarnessError, Result, TempDir};

/// Our server on its own thread (stopped, with a save, on drop).
pub struct HostServer {
    name: String,
    h: Option<host::ServerHandle>,
    port: u16,
    data: PathBuf,
    log: Arc<Mutex<String>>,
    tmp: Option<TempDir>,
    exit: Option<i32>,
}

/// The subset of the CLI the scenarios use, mapped onto [`HostConfig`].
pub fn host_config(args: &[String]) -> Result<HostConfig> {
    let mut cfg = HostConfig { save_delay: 1, ..HostConfig::default() };
    let mut name = None;
    let mut width = 1;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or_else(|| HarnessError::Usage(format!("{} needs a value", a)));
        match a.as_str() {
            "--new" => name = Some(val()?),
            "--load" => cfg.world = WorldSelect::Load(val()?),
            "--world_id" => cfg.world_id = Some(val()?),
            "--custom-rules" => cfg.custom_rules_json = Some(val()?),
            "--save_delay" => cfg.save_delay = val()?.parse().map_err(|_| HarnessError::Usage("save_delay".into()))?,
            "--max_players" => {
                cfg.max_players = val()?.parse().map_err(|_| HarnessError::Usage("max_players".into()))?
            }
            "--owner" => cfg.owner_name = Some(val()?),
            "--world_width" => {
                width = match val()?.as_str() {
                    "1" => 1,
                    "4" => 4,
                    "16" => 16,
                    "1/4" | "104" => 104,
                    "1/16" | "116" => 116,
                    w => return Err(HarnessError::Usage(format!("world_width {}", w))),
                }
            }
            other => return Err(HarnessError::Usage(format!("host_config: unsupported arg {}", other))),
        }
    }
    if let Some(n) = name {
        cfg.world = WorldSelect::New { name: n, width_multiplier: width };
    }
    Ok(cfg)
}

impl HostServer {
    /// Starts our server in-process; `spec.data_dir` (a data root like the reference's `DATA_DIR`) is reused.
    pub fn start(spec: &RunSpec) -> Result<HostServer> {
        let (data, tmp) = match &spec.data_dir {
            Some(d) => (d.clone(), None),
            None => {
                let t = TempDir::new(&spec.tag)?;
                (t.path().join("data"), Some(t))
            }
        };
        let app = data.join("Library/ApplicationSupport/TheBlockheads");
        std::fs::create_dir_all(&app)?;
        let port = free_udp_port();
        let mut cfg = host_config(&spec.args)?;
        cfg.data_dir = Some(app);
        cfg.port = Some(port);
        let log = Arc::new(Mutex::new(String::new()));
        let l2 = log.clone();
        let sink: host::LogCallback = Arc::new(move |s: &str| {
            let mut g = l2.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            g.push_str(s);
            g.push('\n');
        });
        let h = host::start(cfg, sink)?;
        Ok(HostServer { name: format!("host-{}", spec.tag), h: Some(h), port, data, log, tmp, exit: None })
    }

    fn db_stamp(&self, world_id: &str) -> Option<(SystemTime, u64)> {
        let m = std::fs::metadata(self.save_dir(world_id).join("world_db/data.mdb")).ok()?;
        Some((m.modified().ok()?, m.len()))
    }
}

impl ServerHandle for HostServer {
    fn name(&self) -> &str {
        &self.name
    }
    fn port(&self) -> u16 {
        self.port
    }
    fn saves_dir(&self) -> PathBuf {
        self.data.join("Library/ApplicationSupport/TheBlockheads/saves")
    }
    fn log(&self) -> String {
        self.log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
    fn is_running(&mut self) -> bool {
        let r = self.h.as_ref().is_some_and(|h| h.is_alive());
        if !r {
            let l = self.log();
            let t: Vec<&str> = l.lines().rev().take(40).collect();
            eprintln!("DIAG not alive: {:?}\n{}", self.h.as_ref().map(|h| h.status()), t.into_iter().rev().collect::<Vec<_>>().join("\n"));
        }
        r
    }
    fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        loop {
            if self.log().lines().any(|l| strip_nslog_prefix(l).contains(needle)) {
                return Ok(());
            }
            if !self.is_running() || Instant::now() >= end {
                let st = self.h.as_ref().map(|h| h.status());
                return Err(HarnessError::Timeout(format!("{}: no '{}' ({:?})", self.name, needle, st)));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn wait_for_saves(&mut self, world_id: &str, n: u32, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        let mut last = self.db_stamp(world_id);
        let mut seen = 0;
        while seen < n {
            std::thread::sleep(Duration::from_millis(50));
            let now = self.db_stamp(world_id);
            if now != last {
                seen += now.is_some() as u32;
                last = now;
            }
            if seen < n && (!self.is_running() || Instant::now() >= end) {
                return Err(HarnessError::Timeout(format!("{}: saves {}/{}", self.name, seen, n)));
            }
        }
        Ok(())
    }
    /// Stops the server. Unlike SIGINT on the reference this also saves; callers wait for saves first anyway.
    fn stop_sigint(&mut self, _grace: Duration) -> Result<Option<i32>> {
        if let Some(mut h) = self.h.take() {
            self.exit = Some(if let Status::Stopped(c) = h.stop() { c } else { -1 });
        }
        Ok(self.exit)
    }
    fn data_dir(&self) -> PathBuf {
        self.data.clone()
    }
    fn keep_data(&mut self) -> PathBuf {
        if let Some(t) = self.tmp.take() {
            let _ = t.keep();
        }
        self.data.clone()
    }
    fn send_console(&mut self, line: &str) -> Result<()> {
        match &self.h {
            Some(h) => {
                h.send_command(line);
                Ok(())
            }
            None => Err(HarnessError::Usage(format!("{}: stopped", self.name))),
        }
    }
}

impl Drop for HostServer {
    fn drop(&mut self) {
        if let Some(mut h) = self.h.take() {
            let _ = h.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn maps_cli_args() {
        let c = host_config(&a(&[
            "--new",
            "x",
            "--world_id",
            "abc",
            "--world_width",
            "1/4",
            "--custom-rules",
            "{}",
            "--save_delay",
            "3",
            "--max_players",
            "7",
            "--owner",
            "O",
        ]))
        .unwrap();
        assert!(matches!(c.world, WorldSelect::New { ref name, width_multiplier: 104 } if name == "x"));
        assert_eq!(c.world_id.as_deref(), Some("abc"));
        assert_eq!(c.custom_rules_json.as_deref(), Some("{}"));
        assert_eq!((c.save_delay, c.max_players, c.owner_name.as_deref()), (3, 7, Some("O")));
        let c = host_config(&a(&["--load", "abc"])).unwrap();
        assert!(matches!(c.world, WorldSelect::Load(ref n) if n == "abc"));
        for w in ["1", "4", "16", "1/16"] {
            assert!(host_config(&a(&["--new", "x", "--world_width", w])).is_ok());
        }
        assert!(host_config(&a(&["--world_width", "3"])).is_err());
        assert!(host_config(&a(&["--bogus"])).is_err());
        assert!(host_config(&a(&["--new"])).is_err());
        assert!(host_config(&a(&["--save_delay", "x"])).is_err());
        assert!(host_config(&a(&["--max_players", "x"])).is_err());
    }

    /// The whole handle on a small new world: log waits, saves, console, stop, keep_data, drop while running.
    #[test]
    fn host_server_lifecycle() {
        let spec = |tag: &str| RunSpec {
            tag: tag.into(),
            args: a(&["--new", "x", "--world_id", "w1", "--world_width", "1/16"]),
            ..Default::default()
        };
        let mut s = HostServer::start(&spec("inproc")).unwrap();
        assert_eq!(s.name(), "host-inproc");
        assert!(s.port() > 0);
        s.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
        assert!(matches!(s.wait_for_log("never printed", Duration::from_millis(100)), Err(HarnessError::Timeout(_))));
        assert!(matches!(
            s.wait_for_saves("no-such-world", 1, Duration::from_millis(100)),
            Err(HarnessError::Timeout(_))
        ));
        s.send_console("/help").unwrap();
        // two clients on consecutive local ports, as the parity sessions run them
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", s.port()).parse().unwrap();
        let dir = TempDir::new("ports").unwrap();
        let base = crate::harness::free_udp_port_range(4);
        let ev = crate::harness::run_script_ports(
            "client A\nclient B\nuse A\ndisconnect\nuse B\ndisconnect\n",
            addr,
            dir.path(),
            Some(base),
        );
        assert!(ev.unwrap().iter().any(|e| e.client == "B"));
        let ev = crate::harness::run_script("client A\nuse A\ndisconnect\n", addr, dir.path()).unwrap();
        assert!(ev.iter().any(|e| e.client == "A"));
        assert!(matches!(crate::harness::run_script("not a verb\n", addr, dir.path()), Err(HarnessError::Parse(_))));
        s.wait_for_saves("w1", 1, Duration::from_secs(60)).unwrap();
        assert!(s.is_running());
        assert_eq!(s.stop_sigint(Duration::from_secs(1)).unwrap(), Some(0));
        assert_eq!(s.stop_sigint(Duration::from_secs(1)).unwrap(), Some(0), "already stopped");
        assert!(!s.is_running());
        assert!(matches!(s.send_console("/help"), Err(HarnessError::Usage(_))));
        let kept = s.keep_data();
        assert_eq!(kept, s.data_dir());
        drop(s);
        assert!(kept.exists());
        std::fs::remove_dir_all(kept.parent().unwrap()).unwrap();
        // dropped while running: stopped by Drop; a given data directory is reused as is
        let data = TempDir::new("given-data").unwrap();
        let mut given = spec("inproc-drop");
        given.data_dir = Some(data.path().join("data"));
        let mut s = HostServer::start(&given).unwrap();
        s.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
        drop(s);
    }
}
