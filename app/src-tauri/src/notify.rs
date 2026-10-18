//! Content of the Android foreground notification (and the crash alert), translated on the Rust side because the
//! WebView that normally owns the UI strings may be suspended while the screen is off.

use crate::core::Poll;
use tauri_plugin_hostservice::{AlertInfo, ServiceInfo};

struct Strings {
    running: &'static str,
    starting: &'static str,
    stopping: &'static str,
    loading: &'static str,
    players_of: &'static str,
    players: &'static str,
    stop: &'static str,
    copy: &'static str,
    console: &'static str,
    crashed: &'static str,
    crashed_body: &'static str,
    open_log: &'static str,
}

const EN: Strings = Strings {
    running: "Running",
    starting: "Starting",
    stopping: "Stopping",
    loading: "Loading {world}…",
    players_of: "{n} of {max} players online",
    players: "{n} players online",
    stop: "Stop server",
    copy: "Copy address",
    console: "Console",
    crashed: "Server stopped unexpectedly",
    crashed_body: "Tap to see why.",
    open_log: "Open log",
};

const TR: Strings = Strings {
    running: "Çalışıyor",
    starting: "Başlıyor",
    stopping: "Duruyor",
    loading: "{world} yükleniyor…",
    players_of: "{n}/{max} oyuncu çevrimiçi",
    players: "{n} oyuncu çevrimiçi",
    stop: "Sunucuyu durdur",
    copy: "Adresi kopyala",
    console: "Konsol",
    crashed: "Sunucu beklenmedik şekilde durdu",
    crashed_body: "Nedenini görmek için dokunun.",
    open_log: "Günlüğü aç",
};

fn strings(lang: &str) -> &'static Strings {
    if lang == "tr" {
        &TR
    } else {
        &EN
    }
}

/// The ongoing notification for the current poll result, or `None` when the server is not active.
pub fn service_info(lang: &str, p: &Poll, world: &str, address: Option<&str>, max_players: i32) -> Option<ServiceInfo> {
    let s = strings(lang);
    let loading = match p.state.as_str() {
        "starting" => true,
        "running" | "stopping" => false,
        _ => return None,
    };
    let state = match p.state.as_str() {
        "starting" => s.starting,
        "stopping" => s.stopping,
        _ => s.running,
    };
    let n = p.players.len();
    let (text, big_text) = if loading {
        (s.loading.replace("{world}", world), None)
    } else {
        let count = if max_players > 0 {
            s.players_of.replace("{n}", &n.to_string()).replace("{max}", &max_players.to_string())
        } else {
            s.players.replace("{n}", &n.to_string())
        };
        let text = match address {
            Some(a) => format!("{count} · {a}"),
            None => count,
        };
        let names = p.players.iter().map(|q| q.name.as_str()).collect::<Vec<_>>().join(", ");
        (text, if names.is_empty() { None } else { Some(names) })
    };
    Some(ServiceInfo {
        title: format!("{state} · {world}"),
        text,
        big_text,
        started_at: p.started_at,
        loading,
        address: address.map(str::to_string),
        stop_label: s.stop.into(),
        copy_label: s.copy.into(),
        open_label: s.console.into(),
    })
}

pub fn crash_alert(lang: &str) -> AlertInfo {
    let s = strings(lang);
    AlertInfo { title: s.crashed.into(), text: s.crashed_body.into(), open_label: s.open_log.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Player;

    fn poll(state: &str, players: Vec<Player>) -> Poll {
        Poll {
            state: state.into(),
            detail: None,
            world_id: None,
            port: None,
            lines: vec![],
            times: vec![],
            next: 0,
            players,
            started_at: Some(5),
        }
    }

    fn player(n: &str) -> Player {
        Player { name: n.into(), ip: "1.1.1.1".into(), id: n.into(), joined_at: 1 }
    }

    #[test]
    fn running_lists_players_and_address() {
        let p = poll("running", vec![player("MIRA"), player("TOBY")]);
        let i = service_info("en", &p, "Sunny", Some("10.0.0.2:15151"), 16).unwrap();
        assert_eq!(i.title, "Running · Sunny");
        assert_eq!(i.text, "2 of 16 players online · 10.0.0.2:15151");
        assert_eq!(i.big_text.as_deref(), Some("MIRA, TOBY"));
        assert_eq!(i.started_at, Some(5));
        assert!(!i.loading);
        assert_eq!(i.address.as_deref(), Some("10.0.0.2:15151"));
    }

    #[test]
    fn unlimited_and_no_address_in_turkish() {
        let p = poll("running", vec![]);
        let i = service_info("tr", &p, "Ada", None, 0).unwrap();
        assert_eq!(i.title, "Çalışıyor · Ada");
        assert_eq!(i.text, "0 oyuncu çevrimiçi");
        assert!(i.big_text.is_none() && i.address.is_none());
        let i = service_info("tr", &p, "Ada", None, 8).unwrap();
        assert_eq!(i.text, "0/8 oyuncu çevrimiçi");
    }

    #[test]
    fn starting_stopping_and_inactive() {
        let i = service_info("en", &poll("starting", vec![]), "Sunny", None, 0).unwrap();
        assert!(i.loading);
        assert_eq!(i.text, "Loading Sunny…");
        assert_eq!(i.title, "Starting · Sunny");
        let i = service_info("en", &poll("stopping", vec![]), "Sunny", None, 0).unwrap();
        assert_eq!(i.title, "Stopping · Sunny");
        assert!(service_info("en", &poll("stopped", vec![]), "x", None, 0).is_none());
        assert!(service_info("en", &poll("failed", vec![]), "x", None, 0).is_none());
    }

    #[test]
    fn alert_is_translated() {
        assert_eq!(crash_alert("en").open_label, "Open log");
        assert_eq!(crash_alert("tr").open_label, "Günlüğü aç");
    }
}
