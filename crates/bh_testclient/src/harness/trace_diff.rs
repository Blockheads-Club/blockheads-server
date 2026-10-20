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
            (None, Some((n, _))) => return format!("section {} only in ours", n),
            (None, None) => {}
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
            (None, Some(y)) => d.push(mk(None, Some(i), None, Some(y), "only in ours".into())),
            (None, None) => {}
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
                (None, Some(&l)) => d.push(mk(None, Some(l), None, Some(&b[l]), "only in ours".into())),
                (None, None) => {}
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
}
