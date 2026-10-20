//! Curse-word filters.
//!
//! `removeCurseWordsFromBlockheadName:`.

use crate::foundation::nsstring;
use crate::plist::value::PlistValue;
use crate::protocol::messages;
use crate::server::bhserver::{new_mutable_dict, BHServer};

/// U+1F621 POUTING FACE (UTF-8 F0 9F 98 A1), the replacement text.
const ANGRY: &str = "\u{1F621}";

/// Per-UTF-16-unit case fold used by `NSCaseInsensitiveSearch`.
// Note: GNUstep folds with its own tables; only single-unit lowercase mappings are applied here.
fn fold_unit(u: u16) -> u16 {
    if u < 0x80 {
        return (u as u8).to_ascii_lowercase() as u16;
    }
    match char::from_u32(u as u32) {
        Some(c) => {
            let mut it = c.to_lowercase();
            match (it.next(), it.next()) {
                (Some(l), None) if (l as u32) < 0x10000 => l as u16,
                _ => u,
            }
        }
        None => u,
    }
}

/// All non-overlapping case-insensitive matches of `needle` in `hay`, left to right, as UTF-16 `(start, len)`.
/// An empty needle never matches.
fn find_all_ci(hay: &[u16], needle: &[u16]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let h: Vec<u16> = hay.iter().map(|u| fold_unit(*u)).collect();
    let n: Vec<u16> = needle.iter().map(|u| fold_unit(*u)).collect();
    let mut i = 0;
    while i + n.len() <= h.len() {
        if h[i..i + n.len()] == n[..] {
            out.push((i, n.len()));
            i += n.len();
        } else {
            i += 1;
        }
    }
    out
}

/// `rangeOfString:options:NSCaseInsensitiveSearch` found?
fn contains_ci(s: &str, needle: &str) -> bool {
    let h: Vec<u16> = s.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    // first-match existence equals non-empty find_all result
    !find_all_ci(&h, &n).is_empty()
}

/// `stringByReplacingOccurrencesOfString:withString:options:NSCaseInsensitiveSearch range:(0,len)`.
fn replace_ci(s: &str, needle: &str, with: &str) -> String {
    let h: Vec<u16> = s.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    let w: Vec<u16> = with.encode_utf16().collect();
    let matches = find_all_ci(&h, &n);
    if matches.is_empty() {
        return s.to_string();
    }
    let mut out: Vec<u16> = Vec::with_capacity(h.len());
    let mut pos = 0;
    for (st, len) in matches {
        out.extend_from_slice(&h[pos..st]);
        out.extend_from_slice(&w);
        pos = st + len;
    }
    out.extend_from_slice(&h[pos..]);
    String::from_utf16_lossy(&out)
}

impl BHServer {
    /// `-replaceCurseWordsForMessage:client:`. A nil message counts as found for every curse (bug-compatible).
    pub fn replace_curse_words_for_message_client(
        &self,
        message: Option<&str>,
        player_id: Option<&str>,
    ) -> Option<String> {
        let mut message: Option<String> = message.map(|s| s.to_string());
        let mut curse_found = false;
        let curses = self.curse_list.borrow().clone().unwrap_or_default();
        for curse in &curses {
            match &message {
                Some(m) => {
                    if contains_ci(m, curse) {
                        curse_found = true;
                        message = Some(replace_ci(m, curse, ANGRY));
                    }
                }
                None => {
                    curse_found = true;
                }
            }
        }
        if curse_found {
            let shown = message.as_deref().unwrap_or("(null)");
            let notice =
                format!("Cursing/swearing is not allowed on public servers. Your message was modified to {}.", shown);
            match player_id {
                None => {
                    if let Some(d) = self.delegate() {
                        let mut dict = new_mutable_dict();
                        dict.map.insert("message".to_string(), PlistValue::string(notice));
                        dict.map.insert("alias".to_string(), PlistValue::string("SERVER"));
                        d.chat_message_recieved_display_notification(dict, true);
                    }
                }
                Some(pid) => self.send_chat_message_send_to_clients(notice.as_str(), Some(&[pid.to_string()])),
            }
        }
        let _ = messages::PROTOCOL_VERSION;
        message
    }

    /// `-removeCurseWordsFromBlockheadName:`.
    pub fn remove_curse_words_from_blockhead_name(&self, name: Option<&str>) -> Option<String> {
        let mut result = name.map(|s| s.to_string());
        let curses = self.curse_list.borrow().clone().unwrap_or_default();
        for curse in &curses {
            if let Some(r) = &result {
                if contains_ci(r, curse) {
                    result = Some(replace_ci(r, curse, ""));
                }
            }
        }
        let _ = nsstring::length("");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::bhserver::test_support::*;

    fn with_curses(tag: &str, text: &str) -> Harness {
        let t = text.to_string();
        Harness::with_files(tag, move |_| {
            let _ = t;
        })
        .with_curse(text)
    }

    impl Harness {
        fn with_curse(self, text: &str) -> Harness {
            *self.srv.curse_list.borrow_mut() = Some(text.split('\n').map(|s| s.to_string()).collect());
            self
        }
    }

    #[test]
    fn replaces_case_insensitively_and_notifies() {
        let h = with_curses("prof_a", "bad\nWorse\n");
        let pid = h.join("Alice");
        h.m.clear();
        let out = h.srv.replace_curse_words_for_message_client(Some("you are Bad and worSE"), Some(&pid));
        assert_eq!(out.as_deref(), Some("you are \u{1F621} and \u{1F621}"));
        let pk = h.m.packets_to(&pid);
        assert_eq!(pk[0][0], 0x24);
        let msgs = h.d.messages();
        assert_eq!(msgs.last().unwrap(), "Cursing/swearing is not allowed on public servers. Your message was modified to you are \u{1F621} and \u{1F621}.");
    }

    #[test]
    fn empty_curse_never_matches_and_local_notice() {
        let h = with_curses("prof_b", "");
        assert_eq!(h.srv.replace_curse_words_for_message_client(Some("abc"), None).as_deref(), Some("abc"));
        assert!(h.d.messages().is_empty());
        let h = with_curses("prof_c", "\nb");
        let r = h.srv.replace_curse_words_for_message_client(Some("abc"), None);
        assert_eq!(r.as_deref(), Some("a\u{1F621}c"));
        assert_eq!(h.srv.remove_curse_words_from_blockhead_name(Some("abc")).as_deref(), Some("ac"));
        let (d, display) = h.d.chats.borrow().last().cloned().unwrap();
        assert!(display);
        assert_eq!(dict_str_of(&d, "alias").as_deref(), Some("SERVER"));
    }

    fn dict_str_of(d: &crate::plist::value::PlistDict, k: &str) -> Option<String> {
        crate::server::bhserver::dict_str(d, k)
    }

    #[test]
    fn nil_message_counts_as_found() {
        let h = with_curses("prof_d", "x");
        assert_eq!(h.srv.replace_curse_words_for_message_client(None, None), None);
        assert!(h.d.messages()[0].ends_with("modified to (null)."));
    }
}
