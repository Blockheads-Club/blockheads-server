//! Comparison of normalised packet traces (strict sequence or per-type queues).
//!

use std::collections::BTreeMap;

use super::json_escape;
use super::normalize::NormMsg;

/// Comparison mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    /// same sequence
    Strict,
    /// per (client, type byte) ordered queues; tolerant of order between types
    PerType,
}

/// One message-level difference.
#[derive(Clone, Debug)]
pub struct MsgDiff {
    pub index_a: Option<usize>,
    pub index_b: Option<usize>,
    pub ty: u8,
    pub reason: String,
    pub a_text: String,
    pub b_text: String,
}

/// Result of a trace comparison.
#[derive(Clone, Debug)]
pub struct TraceDiffReport {
    pub mode: DiffMode,
    pub count_a: usize,
    pub count_b: usize,
    pub strict_equal: bool,
    pub per_type_equal: bool,
    /// Strict failed but PerType is equal: a pure send-order difference
    pub reordered_only: bool,
    pub diffs: Vec<MsgDiff>,
}

fn same(a: &NormMsg, b: &NormMsg) -> bool {
    a.ty == b.ty && a.channel == b.channel && a.client == b.client && a.sections == b.sections
}

/// Why two messages differ (first difference).
fn reason(a: &NormMsg, b: &NormMsg) -> String {
    if a.ty != b.ty {
        return format!("type differs: ref 0x{:02x} ours 0x{:02x}", a.ty, b.ty);
    }
    if a.client != b.client {
        return format!("client differs: {} vs {}", a.client, b.client);
    }
    if a.channel != b.channel {
        return format!("channel differs: {} vs {}", a.channel, b.channel);
    }
    for i in 0..a.sections.len().max(b.sections.len()) {
        match (a.sections.get(i), b.sections.get(i)) {
            (Some((na, da)), Some((nb, db))) => {
                if na != nb {
                    return format!("section #{} name differs: {} vs {}", i, na, nb);
                }
                if da != db {
                    if na.ends_with(".plist") {
                        let (ta, tb) = (String::from_utf8_lossy(da), String::from_utf8_lossy(db));
                        let (la, lb): (Vec<&str>, Vec<&str>) = (ta.split('\n').collect(), tb.split('\n').collect());
                        for j in 0..la.len().max(lb.len()) {
                            if la.get(j) != lb.get(j) {
                                return format!(
                                    "section {} differs at line {}: ref `{}` ours `{}`",
                                    na,
                                    j + 1,
                                    la.get(j).unwrap_or(&"<none>"),
                                    lb.get(j).unwrap_or(&"<none>")
                                );
                            }
                        }
                    }
                    let at = da.iter().zip(db.iter()).position(|(x, y)| x != y).unwrap_or(da.len().min(db.len()));
                    return format!("section {} differs at byte {} (len ref {} ours {})", na, at, da.len(), db.len());
                }
            }
            (Some((n, _)), None) => return format!("section {} only in ref", n),
            (None, other) => return format!("section {} only in ours", other.map_or("", |(n, _)| n.as_str())),
        }
    }
    "identical".into()
}

fn mk(ia: Option<usize>, ib: Option<usize>, a: Option<&NormMsg>, b: Option<&NormMsg>, why: String) -> MsgDiff {
    MsgDiff {
        index_a: ia,
        index_b: ib,
        ty: a.or(b).map(|m| m.ty).unwrap_or(0),
        reason: why,
        a_text: a.map(|m| m.text.clone()).unwrap_or_default(),
        b_text: b.map(|m| m.text.clone()).unwrap_or_default(),
    }
}

fn strict_diffs(a: &[NormMsg], b: &[NormMsg]) -> Vec<MsgDiff> {
    let mut d = Vec::new();
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => {
                if !same(x, y) {
                    d.push(mk(Some(i), Some(i), Some(x), Some(y), reason(x, y)));
                }
            }
            (Some(x), None) => d.push(mk(Some(i), None, Some(x), None, "only in ref".into())),
            (None, y) => d.extend(y.map(|y| mk(None, Some(i), None, Some(y), "only in ours".into()))),
        }
    }
    d
}

/// Queue discriminator inside one type byte: the object type of object batches (0x07/0x08/... are one queue per `objType`,
/// so the 45/14 creation order is a map-order difference,), nothing for other messages.
fn subkey(m: &NormMsg) -> Vec<u8> {
    match m.sections.first() {
        Some((n, b)) if n == "objType" => b.clone(),
        _ => vec![],
    }
}

fn per_type_diffs(a: &[NormMsg], b: &[NormMsg]) -> Vec<MsgDiff> {
    let group = |m: &[NormMsg]| {
        let mut g: BTreeMap<(String, u8, Vec<u8>), Vec<usize>> = BTreeMap::new();
        for (i, x) in m.iter().enumerate() {
            g.entry((x.client.clone(), x.ty, subkey(x))).or_default().push(i);
        }
        g
    };
    let (ga, gb) = (group(a), group(b));
    let mut keys: Vec<&(String, u8, Vec<u8>)> = ga.keys().chain(gb.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut d = Vec::new();
    let empty = Vec::new();
    for k in keys {
        let (qa, qb) = (ga.get(k).unwrap_or(&empty), gb.get(k).unwrap_or(&empty));
        for j in 0..qa.len().max(qb.len()) {
            match (qa.get(j), qb.get(j)) {
                (Some(&i), Some(&l)) => {
                    if !same(&a[i], &b[l]) {
                        d.push(mk(
                            Some(i),
                            Some(l),
                            Some(&a[i]),
                            Some(&b[l]),
                            format!("{} #{} of its type: {}", k.0, j, reason(&a[i], &b[l])),
                        ));
                    }
                }
                (Some(&i), None) => d.push(mk(Some(i), None, Some(&a[i]), None, "only in ref".into())),
                (None, l) => d.extend(l.map(|&l| mk(None, Some(l), None, Some(&b[l]), "only in ours".into()))),
            }
        }
    }
    d.sort_by_key(|x| (x.index_a.unwrap_or(usize::MAX), x.index_b.unwrap_or(usize::MAX)));
    d
}

/// Compare reference `a` with ours `b`.
pub fn diff_traces(a: &[NormMsg], b: &[NormMsg], mode: DiffMode) -> TraceDiffReport {
    let strict = strict_diffs(a, b);
    let per = per_type_diffs(a, b);
    let strict_equal = strict.is_empty();
    let per_type_equal = per.is_empty();
    TraceDiffReport {
        mode,
        count_a: a.len(),
        count_b: b.len(),
        strict_equal,
        per_type_equal,
        reordered_only: !strict_equal && per_type_equal,
        diffs: if mode == DiffMode::Strict { strict } else { per },
    }
}

impl TraceDiffReport {
    /// Clean = the requested mode is equal AND something was compared on both sides.
    pub fn is_clean(&self) -> bool {
        self.count_a > 0
            && self.count_b > 0
            && if self.mode == DiffMode::Strict { self.strict_equal } else { self.per_type_equal }
    }
    pub fn render(&self) -> String {
        let mut s = format!(
            "messages: ref={} ours={} mode={:?} strict_equal={} per_type_equal={} reordered_only={}\n",
            self.count_a, self.count_b, self.mode, self.strict_equal, self.per_type_equal, self.reordered_only
        );
        if self.count_a == 0 || self.count_b == 0 {
            s.push_str("WARNING: empty trace on at least one side, nothing was compared\n");
        }
        if self.reordered_only {
            s.push_str(
                "NOTE: only the SEND ORDER differs (same messages per type); fix the iteration order, do not mask it\n",
            );
        }
        for d in &self.diffs {
            s.push_str(&format!(
                "diff ref#{} ours#{} type 0x{:02x}: {}\n",
                d.index_a.map_or("-".into(), |x| x.to_string()),
                d.index_b.map_or("-".into(), |x| x.to_string()),
                d.ty,
                d.reason
            ));
            if !d.a_text.is_empty() {
                s.push_str(&format!("  ref : {}\n", d.a_text.replace('\n', "\n        ")));
            }
            if !d.b_text.is_empty() {
                s.push_str(&format!("  ours: {}\n", d.b_text.replace('\n', "\n        ")));
            }
        }
        s.push_str(if self.is_clean() { "RESULT: identical\n" } else { "RESULT: DIFFERENT\n" });
        s
    }
    pub fn render_json(&self) -> String {
        let mut s = format!(
            "{{\"count_a\":{},\"count_b\":{},\"strict_equal\":{},\"per_type_equal\":{},\"reordered_only\":{},\"clean\":{},\"diffs\":[",
            self.count_a,
            self.count_b,
            self.strict_equal,
            self.per_type_equal,
            self.reordered_only,
            self.is_clean()
        );
        for (i, d) in self.diffs.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{{\"type\":{},\"reason\":\"{}\"}}", d.ty, json_escape(&d.reason)));
        }
        s.push_str("]}");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::super::normalize::*;
    use super::*;

    fn batch(obj_type: u8, tag: u8) -> TraceEvent {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><array><data>AAEC{}</data></array></plist>",
            ["AA==", "AQ==", "Ag=="][(tag % 3) as usize]
        );
        let mut b = vec![0x07, obj_type];
        b.extend(blockheads_server::foundation::gzip::gzip_deflate(xml.as_bytes()));
        TraceEvent { t_ms: 0.0, dir: Dir::Rx, channel: 0, reliable: true, client: "C".into(), bytes: b }
    }
    fn simple(t: u8, x: u8) -> TraceEvent {
        TraceEvent { t_ms: 0.0, dir: Dir::Rx, channel: 0, reliable: true, client: "C".into(), bytes: vec![t, x] }
    }
    fn norm(ev: Vec<TraceEvent>) -> Vec<NormMsg> {
        normalize_trace(&ev, &NormalizeOptions::default())
    }

    /// S11: swapping the 0x07(45)/0x07(14) pair is `reordered_only`.
    #[test]
    fn s11_reordered_only() {
        let a = norm(vec![simple(0x04, 1), batch(45, 0), batch(14, 1), simple(0x27, 0)]);
        let b = norm(vec![simple(0x04, 1), batch(14, 1), batch(45, 0), simple(0x27, 0)]);
        let r = diff_traces(&a, &b, DiffMode::Strict);
        assert!(!r.strict_equal && r.per_type_equal && r.reordered_only);
        assert!(!r.is_clean());
        assert!(diff_traces(&a, &b, DiffMode::PerType).is_clean());
        // two messages of the same type and object type keep their order in PerType mode
        let c = norm(vec![simple(0x3a, 1), simple(0x3a, 2)]);
        let d = norm(vec![simple(0x3a, 2), simple(0x3a, 1)]);
        assert!(!diff_traces(&c, &d, DiffMode::PerType).per_type_equal);
        assert!(diff_traces(&a, &a, DiffMode::Strict).is_clean());
        assert!(!diff_traces(&[], &[], DiffMode::Strict).is_clean());
        let t = diff_traces(&a, &a[..3], DiffMode::Strict);
        assert!(t.diffs[0].reason.contains("only in ref"));
        assert!(!t.render().is_empty());
    }

    fn msg(ty: u8, client: &str, channel: u8, sections: &[(&str, &str)]) -> NormMsg {
        NormMsg {
            ty,
            channel,
            client: client.into(),
            sections: sections.iter().map(|(n, d)| (n.to_string(), d.as_bytes().to_vec())).collect(),
            text: format!("text of {ty:#x}"),
            t_ms: 0.0,
        }
    }

    #[test]
    fn reasons_name_the_first_difference() {
        let base = msg(1, "A", 0, &[("body", "abc")]);
        assert!(reason(&base, &msg(2, "A", 0, &[("body", "abc")])).starts_with("type differs: ref 0x01 ours 0x02"));
        assert_eq!(reason(&base, &msg(1, "B", 0, &[("body", "abc")])), "client differs: A vs B");
        assert_eq!(reason(&base, &msg(1, "A", 3, &[("body", "abc")])), "channel differs: 0 vs 3");
        assert!(reason(&base, &msg(1, "A", 0, &[("other", "abc")])).contains("section #0 name differs: body vs other"));
        assert!(reason(&base, &msg(1, "A", 0, &[("body", "abX")]))
            .contains("section body differs at byte 2 (len ref 3 ours 3)"));
        assert!(reason(&base, &msg(1, "A", 0, &[("body", "ab")])).contains("at byte 2 (len ref 3 ours 2)"));
        assert_eq!(reason(&base, &base), "identical");
        let pl = msg(1, "A", 0, &[("chat.plist", "l1\nl2\nl3")]);
        let r = reason(&pl, &msg(1, "A", 0, &[("chat.plist", "l1\nXX\nl3")]));
        assert!(r.contains("line 2: ref `l2` ours `XX`"), "{r}");
        let r = reason(&pl, &msg(1, "A", 0, &[("chat.plist", "l1\nl2")]));
        assert!(r.contains("line 3: ref `l3` ours `<none>`"), "{r}");
        let r = reason(&msg(1, "A", 0, &[("chat.plist", "l1")]), &msg(1, "A", 0, &[("chat.plist", "l1\nl2")]));
        assert!(r.contains("line 2: ref `<none>` ours `l2`"), "{r}");
        assert_eq!(reason(&base, &msg(1, "A", 0, &[("body", "abc"), ("extra", "x")])), "section extra only in ours");
        assert_eq!(reason(&msg(1, "A", 0, &[("body", "abc"), ("extra", "x")]), &base), "section extra only in ref");
    }

    #[test]
    fn strict_and_per_type_modes_report_missing_messages() {
        let a = vec![msg(1, "A", 0, &[("body", "1")]), msg(2, "A", 0, &[("body", "2")])];
        let b =
            vec![msg(1, "A", 0, &[("body", "1")]), msg(2, "A", 0, &[("body", "X")]), msg(3, "A", 0, &[("body", "3")])];
        for mode in [DiffMode::Strict, DiffMode::PerType] {
            let r = diff_traces(&a, &b, mode);
            assert_eq!(r.diffs.len(), 2);
            assert_eq!(
                (r.diffs[1].index_a, r.diffs[1].index_b, r.diffs[1].reason.as_str()),
                (None, Some(2), "only in ours")
            );
            assert_eq!(r.diffs[1].b_text, "text of 0x3");
            let rev = diff_traces(&b, &a, mode);
            assert_eq!((rev.diffs[1].index_a, rev.diffs[1].index_b), (Some(2), None));
            assert_eq!(rev.diffs[1].reason, "only in ref");
        }
        assert_eq!(
            diff_traces(&a, &b, DiffMode::PerType).diffs[0].reason,
            "A #0 of its type: section body differs at byte 0 (len ref 1 ours 1)"
        );
    }

    #[test]
    fn reports_render_text_and_json() {
        let a = vec![msg(1, "A", 0, &[("chat.plist", "x\ny")]), msg(2, "A", 0, &[("body", "2")])];
        let swapped = vec![a[1].clone(), a[0].clone()];
        let r = diff_traces(&a, &swapped, DiffMode::Strict);
        assert!(r.reordered_only);
        let text = r.render();
        assert!(
            text.contains("NOTE: only the SEND ORDER differs")
                && text.contains("ref : ")
                && text.contains("ours: text of")
        );
        let j = r.render_json();
        assert!(j.starts_with("{\"count_a\":2,\"count_b\":2,\"strict_equal\":false,\"per_type_equal\":true,\"reordered_only\":true,\"clean\":false,\"diffs\":[{\"type\":1,"));
        assert!(j.contains("},{\"type\":2,") && j.ends_with("]}"));
        let empty = diff_traces(&[], &a, DiffMode::PerType);
        assert!(empty.render().contains("WARNING: empty trace on at least one side"));
        assert!(!empty.is_clean());
        assert!(diff_traces(&a, &a, DiffMode::PerType).render_json().contains("\"diffs\":[]"));
        assert_eq!(subkey(&msg(7, "A", 0, &[("objType", "\u{9}")])), vec![9]);
    }
}
