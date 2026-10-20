//! Cloud credit, privacy string and the no-credit boot.
//!

use crate::protocol::messages;
use crate::server::bhserver::BHServer;

impl BHServer {
    /// `-isCloudMatch`: `[match isCloudMatch]`, NO for a nil match.
    pub fn is_cloud_match(&self) -> bool {
        self.node.match_().map_or(false, |m| m.is_cloud_match())
    }

    /// `-credit` (float32): 0.0 unless a cloud match with a server match.
    pub fn credit(&self) -> f32 {
        if !self.is_cloud_match() {
            return 0.0;
        }
        match self.node.match_() {
            Some(m) => m.as_server_match().map_or(0.0, |s| s.credit()),
            None => 0.0,
        }
    }

    /// `-updateCredit:`: no callers in the binary.
    pub fn update_credit(&self, dt: f32) {
        if self.is_cloud_match() {
            if let Some(m) = self.node.match_() {
                if let Some(s) = m.as_server_match() {
                    let c = s.credit();
                    s.set_credit(c - dt);
                }
            }
        }
    }

    /// `-privacyString`: cloud + BHNetServerMatch: 3 "private", 2 "searchable", else "public".
    pub fn privacy_string(&self) -> Option<String> {
        if !self.is_cloud_match() {
            return None;
        }
        let m = self.node.match_()?;
        let s = m.as_server_match()?;
        Some(
            match s.privacy() {
                3 => "private",
                2 => "searchable",
                _ => "public",
            }
            .to_string(),
        )
    }

    /// `-bootAllClientsDueToNoCredit`: reason 13 + plist, disconnect 4.0 s later.
    pub fn boot_all_clients_due_to_no_credit(&self) {
        let ids = self.connected_clients.borrow().clone();
        for id in ids {
            let world_id = self.save_id.clone().unwrap_or_default();
            let world_name = self.world().and_then(|w| w.world_name()).unwrap_or_default();
            self.send_network_data_to_peers_reliable(
                messages::rejection_no_credit(&world_id, &world_name),
                &[id.clone()],
                true,
            );
            let srv = self.rc();
            let target = id.clone();
            self.perform_after_delay(
                4.0,
                Box::new(move || srv.delayed_disconnect_player_due_to_no_credit_with_id(&target)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::server::bhserver::test_support::*;

    #[test]
    fn non_cloud_has_zero_credit_and_no_privacy_string() {
        let h = Harness::new("credit_plain");
        assert!(!h.srv.is_cloud_match());
        assert_eq!(h.srv.credit(), 0.0);
        assert_eq!(h.srv.privacy_string(), None);
        h.srv.update_credit(1.0);
    }

    #[test]
    fn no_credit_boot_sends_reason_13_and_delays_four_seconds() {
        let h = Harness::new("credit_boot");
        let pid = h.join("Alice");
        h.m.clear();
        h.delayed.borrow_mut().clear();
        h.srv.boot_all_clients_due_to_no_credit();
        let pk = h.m.packets_to(&pid);
        assert_eq!(&pk[0][..2], &[0x26, 0x0d]);
        assert_eq!(h.delayed.borrow()[0].0, 4.0);
        h.run_delayed();
        assert_eq!(h.m.disconnected.borrow().as_slice(), &[pid.clone()]);
        let calls = h.w.calls.borrow().clone();
        assert!(calls.contains(&format!("client_disconnected {} false", pid)));
    }
}
