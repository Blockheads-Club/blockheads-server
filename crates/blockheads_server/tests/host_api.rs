//! Embedding API: world management, start/stop and restart inside one process.

use blockheads_server::host::{self, HostConfig, Status, WorldSelect};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wait_for(h: &host::ServerHandle, want: Status, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if h.status() == want {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn world_management_start_stop_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("TheBlockheads");
    let d = Some(data.as_path());

    assert!(host::list_worlds(d).is_empty());
    let w = host::create_world(d, "Alpha", 104, false).expect("create");
    assert_eq!(w.name, "ALPHA");
    assert_eq!(host::list_worlds(d), vec![w.clone()]);

    for round in 0..2 {
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let cfg = HostConfig {
            data_dir: Some(data.clone()),
            world: WorldSelect::Load(w.id.clone()),
            port: Some(15300 + round),
            ..HostConfig::default()
        };
        let mut h = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
        assert!(wait_for(&h, Status::Running, 60), "round {round}: {:?}\n{:#?}", h.status(), lines.lock().unwrap());
        let end = h.stop();
        assert_eq!(end, Status::Stopped(0), "{:#?}", lines.lock().unwrap());
        assert!(lines.lock().unwrap().iter().any(|l| l.contains("World load complete.")));
    }

    assert!(host::delete_world(d, &w.id));
    assert!(!host::delete_world(d, &w.id));
    assert!(host::list_worlds(d).is_empty());
}

/// Regression: commands configured in `on_ready_commands` run on the server thread without any UI polling.
#[test]
fn on_ready_commands_run_without_polling() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("TheBlockheads");
    let d = Some(data.as_path());
    let w = host::create_world(d, "Gamma", 104, false).expect("create");
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data.clone()),
        world: WorldSelect::Load(w.id.clone()),
        port: Some(15320),
        on_ready_commands: vec!["/set-password hunter2".into()],
        ..HostConfig::default()
    };
    let mut h = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
    assert!(wait_for(&h, Status::Running, 60), "{:?}", h.status());
    let end = Instant::now() + Duration::from_secs(20);
    while Instant::now() < end && !lines.lock().unwrap().iter().any(|l| l.contains("Password set.")) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(lines.lock().unwrap().iter().any(|l| l.contains("Password set.")), "{:#?}", lines.lock().unwrap());
    assert_eq!(h.stop(), Status::Stopped(0));
}
