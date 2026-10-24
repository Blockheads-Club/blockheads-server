//! `+[NSPropertyListSerialization propertyListWithData:options:format:error:]`: format detection, the XML
//! parser, the minimal OpenStep parser and the number grammars.
//!

use super::binary;
use super::value::{ClassTag, Origin, PlistArray, PlistData, PlistDict, PlistStr, PlistValue};
use crate::foundation::exception::ns_raise;
use crate::foundation::gsdict::GsDict;
use crate::foundation::nslog::nslog;

/// Detected serialisation format.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlistFormat {
    OpenStep = 1,
    Xml = 100,
    Binary = 200,
    None = 0,
}

/// Result of a read.
#[derive(Debug)]
pub struct ReadResult {
    pub value: Option<PlistValue>,
    pub format: PlistFormat,
    /// `NSError localizedDescription`; never logged by the helpers.
    pub error: Option<String>,
}

const XML_FAIL: &str = "failed to parse as XML property list";

fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `<integer>` text grammar: whitespace, sign, decimal digits, saturating.
pub fn parse_ns_long_long(s: &str) -> i64 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_c_space(b[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut v: i128 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = (v * 10 + (b[i] - b'0') as i128).min(1i128 << 70);
        i += 1;
    }
    if neg {
        v = -v;
    }
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// `<real>` text grammar (not correctly rounded on purpose).
pub fn parse_ns_double(s: &str) -> f64 {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n && is_c_space(b[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < n && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut x = 0.0f64;
    let (mut nd, mut nf) = (0usize, 0i64);
    while i < n && b[i].is_ascii_digit() {
        x = x * 10.0 + (b[i] - b'0') as f64;
        i += 1;
        nd += 1;
    }
    if i < n && b[i] == b'.' {
        i += 1;
        while i < n && b[i].is_ascii_digit() {
            x = x * 10.0 + (b[i] - b'0') as f64;
            i += 1;
            nd += 1;
            nf += 1;
        }
    }
    if nd == 0 {
        return 0.0;
    }
    let mut e: i64 = 0;
    if i < n && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        let mut eneg = false;
        if i < n && (b[i] == b'+' || b[i] == b'-') {
            eneg = b[i] == b'-';
            i += 1;
        }
        let mut ed = 0;
        while i < n && b[i].is_ascii_digit() {
            e = (e * 10 + (b[i] - b'0') as i64).min(1_000_000);
            i += 1;
            ed += 1;
        }
        if ed == 0 {
            return 0.0;
        }
        if eneg {
            e = -e;
        }
    }
    let ee = e - nf;
    let v = if x == 0.0 {
        0.0
    } else if ee == 0 {
        x
    } else {
        // Correctly rounded 10^E (glibc 2.19 pow); "1eE" parses with infinity/zero saturation.
        x * format!("1e{}", ee).parse::<f64>().unwrap_or(f64::NAN)
    };
    if neg {
        -v
    } else {
        v
    }
}

// ---------------------------------------------------------------------------------------------
// Entity / escape decoding
// ---------------------------------------------------------------------------------------------

/// Whether a Latin-1 `<string>` is stored as `GSUnicodeInlineString` instead of `GSCInlineString`.
///
/// Note: says "entity/escape/TAB in the source text", but the
/// goldens contradict a plain entity rule: `abc&amp;def` and `tab<TAB>here` are Unicode (in5_x.plist.out) while
/// `a&amp;b&lt;&#65;&#x42;` (a_xml.plist.out), `&unknown;` (str_ent) and `\U` escapes (m_esc) stay C strings. The
/// rule fitted to every observed case: after the first predefined/numeric entity or TAB, a later run of at least two
/// plain characters follows (the parser then appends multi-character chunks to a mutable buffer).
fn xml_string_split_storage(raw: &str) -> bool {
    let b = raw.as_bytes();
    let mut seen_special = false;
    let mut run = 0usize;
    let mut i = 0;
    while i < b.len() {
        let special_len = if b[i] == b'\t' {
            Some(1)
        } else if b[i] == b'&' {
            raw[i + 1..].find(';').filter(|r| *r <= 40).and_then(|rel| {
                let name = &raw[i + 1..i + 1 + rel];
                let known = matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
                    || name.strip_prefix('#').is_some_and(|n| {
                        match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                            Some(h) => u64::from_str_radix(h, 16).is_ok(),
                            None => n.parse::<u64>().is_ok(),
                        }
                    });
                known.then_some(rel + 2)
            })
        } else {
            None
        };
        match special_len {
            Some(l) => {
                seen_special = true;
                run = 0;
                i += l;
            }
            None => {
                // count characters, not UTF-8 continuation bytes
                if seen_special && b[i] & 0xC0 != 0x80 {
                    run += 1;
                    if run >= 2 {
                        return true;
                    }
                }
                i += 1;
            }
        }
    }
    false
}

/// Decode XML entities of `raw` into UTF-16 units. Unknown entities log and yield their name.
fn decode_entities(raw: &str) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            if let Some(rel) = raw[i + 1..].find(';').filter(|r| *r <= 40) {
                let name = &raw[i + 1..i + 1 + rel];
                let simple = match name {
                    "amp" => Some('&' as u16),
                    "lt" => Some('<' as u16),
                    "gt" => Some('>' as u16),
                    "quot" => Some('"' as u16),
                    "apos" => Some('\'' as u16),
                    _ => None,
                };
                if let Some(u) = simple {
                    out.push(u);
                } else if let Some(num) = name.strip_prefix('#') {
                    let v = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                        u64::from_str_radix(h, 16).ok()
                    } else {
                        num.parse::<u64>().ok()
                    };
                    match v {
                        // value truncated to a 16-bit unit (observed)
                        Some(v) => out.push(v as u16),
                        None => {
                            nslog(&format!("NSXMLParser: unrecognized entity: &{};", name));
                            out.extend(name.encode_utf16());
                        }
                    }
                } else {
                    nslog(&format!("NSXMLParser: unrecognized entity: &{};", name));
                    out.extend(name.encode_utf16());
                }
                i += rel + 2;
                continue;
            }
        }
        let ch = raw[i..].chars().next().unwrap();
        let mut buf = [0u16; 2];
        out.extend_from_slice(ch.encode_utf16(&mut buf));
        i += ch.len_utf8();
    }
    out
}

fn hex4(u: &[u16]) -> Option<u16> {
    if u.len() < 4 {
        return None;
    }
    let mut v = 0u32;
    for &c in &u[..4] {
        let d = char::from_u32(c as u32)?.to_digit(16)?;
        v = v * 16 + d;
    }
    Some(v as u16)
}

/// `\U` escape decoding including the GNUstep surrogate bug.
fn decode_u_escapes(u: &[u16]) -> Vec<u16> {
    let n = u.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if u[i] == b'\\' as u16 && i + 1 < n && u[i + 1] == b'U' as u16 {
            if let Some(v) = hex4(&u[i + 2..]) {
                if (0xD800..=0xDFFF).contains(&v) {
                    i += 6;
                    if i >= n {
                        ns_raise("NSRangeException", "Range location + length too great");
                    }
                    if i + 1 < n && u[i] == b'\\' as u16 && u[i + 1] == b'U' as u16 {
                        let k = 6.min(n - i);
                        out.extend_from_slice(&u[i..i + k]);
                        i += k;
                    }
                } else {
                    out.push(v);
                    i += 6;
                }
                continue;
            }
        }
        out.push(u[i]);
        i += 1;
    }
    out
}

/// Entities then `\U` escapes, to a Rust string (lone surrogates become U+FFFD).
/// Note: `PlistStr` is a Rust `String`, so a lone UTF-16 surrogate (`&#xD800;`) cannot be represented.
fn decode_text(raw: &str) -> String {
    let u = decode_u_escapes(&decode_entities(raw));
    String::from_utf16_lossy(&u)
}

// ---------------------------------------------------------------------------------------------
// Dates / data
// ---------------------------------------------------------------------------------------------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> Cur<'a> {
    fn num(&mut self, min: usize, max: usize) -> Option<i64> {
        let st = self.i;
        let mut v: i64 = 0;
        while self.i < self.b.len() && self.i - st < max && self.b[self.i].is_ascii_digit() {
            v = v * 10 + (self.b[self.i] - b'0') as i64;
            self.i += 1;
        }
        if self.i - st < min {
            None
        } else {
            Some(v)
        }
    }
    fn lit(&mut self, c: u8) -> Option<()> {
        if self.i < self.b.len() && self.b[self.i] == c {
            self.i += 1;
            Some(())
        } else {
            None
        }
    }
}

/// `<date>`: `YYYY-MM-DDTHH:MM:SSZ` or `YYYY-MM-DD HH:MM:SS +ZZZZ`, else nil. Returns reference-date seconds.
fn parse_date(s: &str) -> Option<f64> {
    let mut c = Cur { b: s.as_bytes(), i: 0 };
    let y = c.num(4, 4)?;
    c.lit(b'-')?;
    let mo = c.num(2, 2)?;
    c.lit(b'-')?;
    let d = c.num(2, 2)?;
    let mut off = 0i64;
    let sep = c.b.get(c.i).copied()?;
    c.i += 1;
    let (h, mi, se);
    if sep == b'T' {
        h = c.num(2, 2)?;
        c.lit(b':')?;
        mi = c.num(2, 2)?;
        c.lit(b':')?;
        se = c.num(2, 2)?;
        c.lit(b'Z')?;
    } else if sep == b' ' {
        h = c.num(2, 2)?;
        c.lit(b':')?;
        mi = c.num(2, 2)?;
        c.lit(b':')?;
        se = c.num(2, 2)?;
        c.lit(b' ')?;
        let sign = match c.b.get(c.i).copied()? {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        c.i += 1;
        let hh = c.num(2, 2)?;
        let mm = c.num(2, 2)?;
        off = sign * (hh * 3600 + mm * 60);
    } else {
        return None;
    }
    if c.i != c.b.len() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 59 {
        return None;
    }
    let unix = days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se - off;
    Some((unix - 978307200) as f64)
}

/// Lenient base64: invalid characters (incl. whitespace) ignored, missing padding fine.
fn decode_base64_lenient(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &c in s.as_bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// XML parser
// ---------------------------------------------------------------------------------------------

struct XmlParser<'a> {
    s: &'a str,
    i: usize,
    tag_cont: ClassTag,
    tag_leaf: ClassTag,
}

type Fail = ();

impl<'a> XmlParser<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    /// Skip whitespace, comments, processing instructions and DOCTYPE; stop at an element tag or EOF.
    fn skip_misc(&mut self) -> Result<(), Fail> {
        loop {
            let r = self.rest();
            let t = r.trim_start_matches(|c: char| c.is_ascii_whitespace());
            self.i += r.len() - t.len();
            let r = self.rest();
            if r.starts_with("<!--") {
                match r.find("-->") {
                    Some(e) => self.i += e + 3,
                    None => return Err(()),
                }
            } else if r.starts_with("<?") {
                match r.find("?>") {
                    Some(e) => self.i += e + 2,
                    None => return Err(()),
                }
            } else if r.starts_with("<!") && !r.starts_with("<![CDATA[") {
                let mut depth = 0;
                let mut end = None;
                for (k, ch) in r.char_indices() {
                    match ch {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        '>' if depth <= 0 => {
                            end = Some(k);
                            break;
                        }
                        _ => {}
                    }
                }
                match end {
                    Some(e) => self.i += e + 1,
                    None => return Err(()),
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Like `skip_misc` but also skips character data / CDATA between child elements of a container.
    fn skip_to_tag(&mut self) -> Result<(), Fail> {
        loop {
            self.skip_misc()?;
            let r = self.rest();
            if r.is_empty() {
                return Err(());
            }
            if r.starts_with("<![CDATA[") {
                match r.find("]]>") {
                    Some(e) => self.i += e + 3,
                    None => return Err(()),
                }
            } else if r.starts_with('<') {
                return Ok(());
            } else {
                match r.find('<') {
                    Some(e) => self.i += e,
                    None => return Err(()),
                }
            }
        }
    }

    /// Parse `<name attrs>` / `<name attrs/>` at the cursor. Returns (name, self_closing).
    fn start_tag(&mut self) -> Result<(String, bool), Fail> {
        let r = self.rest();
        debug_assert!(r.starts_with('<'));
        let b = r.as_bytes();
        let mut k = 1;
        while k < b.len() && !b[k].is_ascii_whitespace() && b[k] != b'/' && b[k] != b'>' {
            k += 1;
        }
        let name = r[1..k].to_string();
        let mut quote: Option<u8> = None;
        while k < b.len() {
            let c = b[k];
            match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    }
                }
                None => {
                    if c == b'"' || c == b'\'' {
                        quote = Some(c);
                    } else if c == b'>' {
                        let sc = k > 0 && b[k - 1] == b'/';
                        self.i += k + 1;
                        return Ok((name, sc));
                    }
                }
            }
            k += 1;
        }
        Err(())
    }

    /// Cursor at `</`: parse the end tag and return its name.
    fn end_tag(&mut self) -> Result<String, Fail> {
        let r = self.rest();
        match r.find('>') {
            Some(e) => {
                let name = r[2..e].trim().to_string();
                self.i += e + 1;
                Ok(name)
            }
            None => Err(()),
        }
    }

    /// Character data up to the matching end tag `</name>` (no child elements allowed).
    fn leaf_text(&mut self, name: &str) -> Result<String, Fail> {
        let mut text = String::new();
        loop {
            let r = self.rest();
            if r.is_empty() {
                return Err(());
            }
            match r.find('<') {
                None => return Err(()),
                Some(e) => {
                    text.push_str(&r[..e]);
                    self.i += e;
                }
            }
            let r = self.rest();
            if r.starts_with("<![CDATA[") {
                // CDATA content is dropped (observed).
                match r.find("]]>") {
                    Some(e) => self.i += e + 3,
                    None => return Err(()),
                }
            } else if r.starts_with("<!--") {
                match r.find("-->") {
                    Some(e) => self.i += e + 3,
                    None => return Err(()),
                }
            } else if r.starts_with("</") {
                let en = self.end_tag()?;
                return if en == name { Ok(text) } else { Err(()) };
            } else {
                let (child, _) = self.start_tag()?;
                nslog(&format!("unrecognized tag <{}>", child));
                return Err(());
            }
        }
    }

    /// Parse the value element whose start tag (`name`) has just been consumed.
    /// `Ok(None)` is a nil result (bad date).
    fn value(&mut self, name: &str, self_closing: bool) -> Result<Option<PlistValue>, Fail> {
        match name {
            "dict" => {
                let mut map = GsDict::plist_parsed();
                if !self_closing {
                    let mut pending: Option<String> = None;
                    loop {
                        self.skip_to_tag()?;
                        if self.rest().starts_with("</") {
                            let en = self.end_tag()?;
                            if en != "dict" {
                                return Err(());
                            }
                            break;
                        }
                        let (child, sc) = self.start_tag()?;
                        if child == "key" {
                            let k = if sc { String::new() } else { decode_text(&self.leaf_text("key")?) };
                            pending = Some(k);
                            continue;
                        }
                        let v = self.value(&child, sc)?;
                        let Some(k) = pending.take() else { return Err(()) };
                        match v {
                            Some(v) => {
                                map.insert(k, v);
                            }
                            None => ns_raise(
                                "NSInvalidArgumentException",
                                &format!("Tried to add nil value for key '{}' to dictionary", k),
                            ),
                        }
                    }
                }
                Ok(Some(PlistValue::Dict(PlistDict { map, tag: self.tag_cont })))
            }
            "array" => {
                let mut items = Vec::new();
                if !self_closing {
                    loop {
                        self.skip_to_tag()?;
                        if self.rest().starts_with("</") {
                            let en = self.end_tag()?;
                            if en != "array" {
                                return Err(());
                            }
                            break;
                        }
                        let (child, sc) = self.start_tag()?;
                        match self.value(&child, sc)? {
                            Some(v) => items.push(v),
                            None => ns_raise("NSInvalidArgumentException", "Tried to add nil to array"),
                        }
                    }
                }
                Ok(Some(PlistValue::Array(PlistArray { items, tag: self.tag_cont })))
            }
            "string" | "integer" | "real" | "date" | "data" | "true" | "false" => {
                let raw = if self_closing { String::new() } else { self.leaf_text(name)? };
                Ok(match name {
                    "string" => {
                        let u = decode_entities(&raw);
                        let s = String::from_utf16_lossy(&decode_u_escapes(&u));
                        let mut tag = self.tag_leaf;
                        tag.alt = xml_string_split_storage(&raw);
                        Some(PlistValue::String(PlistStr { s, tag }))
                    }
                    "integer" => Some(PlistValue::Int(parse_ns_long_long(&raw))),
                    "real" => Some(PlistValue::Real { value: parse_ns_double(&raw), is_float: false }),
                    "true" => Some(PlistValue::Bool(true)),
                    "false" => Some(PlistValue::Bool(false)),
                    "date" => parse_date(&decode_text(&raw)).map(PlistValue::Date),
                    _ => {
                        let mut tag = self.tag_leaf;
                        tag.alt = !raw.trim_matches(|c: char| c.is_ascii_whitespace()).is_empty();
                        Some(PlistValue::Data(PlistData { bytes: decode_base64_lenient(&raw), tag }))
                    }
                })
            }
            other => {
                nslog(&format!("unrecognized tag <{}>", other));
                Err(())
            }
        }
    }

    /// Whole document. `Ok(None)` = no root value.
    fn document(&mut self) -> Result<Option<PlistValue>, Fail> {
        // Leading whitespace before the declaration makes libxml-style parsing fail (observed).
        if self.s.as_bytes().first().is_none_or(|c| c.is_ascii_whitespace()) {
            return Err(());
        }
        self.skip_misc()?;
        if self.rest().is_empty() {
            return Ok(None);
        }
        if !self.rest().starts_with('<') {
            return Err(());
        }
        let (root, sc) = self.start_tag()?;
        if root != "plist" {
            return Err(());
        }
        if sc {
            return Ok(None);
        }
        self.skip_to_tag()?;
        if self.rest().starts_with("</") {
            let en = self.end_tag()?;
            return if en == "plist" { Ok(None) } else { Err(()) };
        }
        let (child, csc) = self.start_tag()?;
        // a nil root (bad date) is just "no value"
        self.value(&child, csc)
    }
}

/// XML text from raw bytes honouring the declared encoding (UTF-8 or ISO-8859-1).
fn xml_text(d: &[u8]) -> String {
    let head_len = d.len().min(200);
    let head = String::from_utf8_lossy(&d[..head_len]).to_ascii_lowercase();
    let latin1 = head
        .find("encoding")
        .map(|p| {
            let t = &head[p..];
            t.contains("iso-8859-1") || t.contains("latin1") || t.contains("iso8859-1")
        })
        .unwrap_or(false)
        && head.find("?>").is_some_and(|e| head.find("encoding").unwrap() < e);
    if latin1 {
        d.iter().map(|&b| b as char).collect()
    } else {
        String::from_utf8_lossy(d).into_owned()
    }
}

fn parse_xml(d: &[u8], opts: u32) -> ReadResult {
    let text = xml_text(d);
    let tag_cont = ClassTag { origin: Origin::Xml, mutable: opts != 0, alt: false };
    let tag_leaf = ClassTag { origin: Origin::Xml, mutable: opts == 2, alt: false };
    let mut p = XmlParser { s: &text, i: 0, tag_cont, tag_leaf };
    match p.document() {
        Ok(v) => ReadResult { value: v, format: PlistFormat::Xml, error: None },
        Err(()) => ReadResult { value: None, format: PlistFormat::Xml, error: Some(XML_FAIL.to_string()) },
    }
}

// ---------------------------------------------------------------------------------------------
// OpenStep (minimal,)
// ---------------------------------------------------------------------------------------------

struct OsParser {
    u: Vec<u16>,
    i: usize,
    opts: u32,
    /// Set when the input ended inside `<hex data>` (GNUstep: "unexpected end of string when parsing data").
    eos_in_data: bool,
}

impl OsParser {
    fn skip_ws(&mut self) {
        loop {
            while self.i < self.u.len() && matches!(self.u[self.i], 0x20 | 0x09 | 0x0a | 0x0d) {
                self.i += 1;
            }
            if self.i + 1 < self.u.len() && self.u[self.i] == b'/' as u16 && self.u[self.i + 1] == b'/' as u16 {
                while self.i < self.u.len() && self.u[self.i] != 0x0a {
                    self.i += 1;
                }
            } else if self.i + 1 < self.u.len() && self.u[self.i] == b'/' as u16 && self.u[self.i + 1] == b'*' as u16 {
                self.i += 2;
                while self.i + 1 < self.u.len() && !(self.u[self.i] == b'*' as u16 && self.u[self.i + 1] == b'/' as u16)
                {
                    self.i += 1;
                }
                self.i = (self.i + 2).min(self.u.len());
            } else {
                return;
            }
        }
    }

    fn mk_string(&self, units: &[u16]) -> PlistValue {
        PlistValue::String(PlistStr {
            s: String::from_utf16_lossy(units),
            tag: ClassTag { origin: Origin::OpenStep, mutable: true, alt: false },
        })
    }

    fn is_bare(c: u16) -> bool {
        c < 0x80 && ((c as u8).is_ascii_alphanumeric() || b"_$+/:.-".contains(&(c as u8)))
    }

    fn value(&mut self) -> Option<PlistValue> {
        self.skip_ws();
        let c = *self.u.get(self.i)?;
        let ctag = ClassTag { origin: Origin::OpenStep, mutable: self.opts != 0, alt: false };
        match c {
            0x7b => {
                self.i += 1;
                // GNUstep builds an empty mutable dictionary (capacity 0) and inserts in document order; the
                // immutable variant is made by class swizzling, so the order is the same for every option
                // (observed: plist_golden/xml_read_expected/c_openstep.plist.out).
                let mut map = GsDict::new();
                loop {
                    self.skip_ws();
                    match self.u.get(self.i) {
                        Some(0x7d) => {
                            self.i += 1;
                            break;
                        }
                        None => return None,
                        _ => {}
                    }
                    let k = self.value()?;
                    let PlistValue::String(k) = k else { return None };
                    self.skip_ws();
                    let v = if self.u.get(self.i) == Some(&(b'=' as u16)) {
                        self.i += 1;
                        self.value()?
                    } else {
                        self.mk_string(&k.s.encode_utf16().collect::<Vec<_>>())
                    };
                    self.skip_ws();
                    if self.u.get(self.i) == Some(&(b';' as u16)) {
                        self.i += 1;
                    } else {
                        return None;
                    }
                    map.insert(k.s.clone(), v);
                }
                Some(PlistValue::Dict(PlistDict { map, tag: ctag }))
            }
            0x28 => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    match self.u.get(self.i) {
                        Some(0x29) => {
                            self.i += 1;
                            break;
                        }
                        None => return None,
                        _ => {}
                    }
                    items.push(self.value()?);
                    self.skip_ws();
                    match self.u.get(self.i) {
                        Some(0x2c) => self.i += 1,
                        Some(0x29) => {}
                        _ => return None,
                    }
                }
                Some(PlistValue::Array(PlistArray { items, tag: ctag }))
            }
            0x3c => {
                self.i += 1;
                let mut nib: Vec<u8> = Vec::new();
                loop {
                    let Some(&c) = self.u.get(self.i) else {
                        self.eos_in_data = true;
                        return None;
                    };
                    self.i += 1;
                    if c == b'>' as u16 {
                        break;
                    }
                    if c == 0x20 {
                        continue;
                    }
                    let d = char::from_u32(c as u32)?.to_digit(16)?;
                    nib.push(d as u8);
                }
                if nib.len() % 2 != 0 {
                    return None;
                }
                let bytes = nib.chunks(2).map(|p| p[0] * 16 + p[1]).collect();
                Some(PlistValue::Data(PlistData {
                    bytes,
                    tag: ClassTag { origin: Origin::OpenStep, mutable: true, alt: false },
                }))
            }
            0x22 => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    let c = *self.u.get(self.i)?;
                    self.i += 1;
                    if c == 0x22 {
                        break;
                    }
                    if c == b'\\' as u16 {
                        let e = *self.u.get(self.i)?;
                        self.i += 1;
                        match e as u8 {
                            b'n' if e < 0x80 => out.push(10),
                            b't' if e < 0x80 => out.push(9),
                            b'r' if e < 0x80 => out.push(13),
                            b'U' if e < 0x80 => {
                                let v = hex4(&self.u[self.i.min(self.u.len())..])?;
                                self.i += 4;
                                out.push(v);
                            }
                            b'0'..=b'7' if e < 0x80 => {
                                let mut v = (e - b'0' as u16) as u32;
                                let mut cnt = 1;
                                while cnt < 3 {
                                    match self.u.get(self.i) {
                                        Some(&d) if (b'0' as u16..=b'7' as u16).contains(&d) => {
                                            v = v * 8 + (d - b'0' as u16) as u32;
                                            self.i += 1;
                                            cnt += 1;
                                        }
                                        _ => break,
                                    }
                                }
                                out.push(v as u16);
                            }
                            _ => out.push(e),
                        }
                    } else {
                        out.push(c);
                    }
                }
                Some(self.mk_string(&out))
            }
            c if Self::is_bare(c) => {
                let st = self.i;
                while self.i < self.u.len() && Self::is_bare(self.u[self.i]) {
                    self.i += 1;
                }
                let units = self.u[st..self.i].to_vec();
                Some(self.mk_string(&units))
            }
            _ => None,
        }
    }
}

fn parse_openstep(d: &[u8], opts: u32) -> ReadResult {
    if d == [0u8] {
        ns_raise("NSRangeException", "in getCharacters:range:, range { 1, 1 } extends beyond size (1)");
    }
    let text = String::from_utf8_lossy(d);
    let u: Vec<u16> = text.encode_utf16().collect();
    let fail = |off: usize, msg: &str| ReadResult {
        value: None,
        format: PlistFormat::OpenStep,
        error: Some(format!("Parse failed at line 1 (char {}) - {}", off, msg)),
    };
    let mut p = OsParser { u, i: 0, opts, eos_in_data: false };
    p.skip_ws();
    let first = p.u.get(p.i).copied();
    // A first character that starts no value parses as an empty unquoted string, so the failure is reported as
    // extra data at that character (observed: a UTF-8 BOM before `<?xml`, bom.plist.out).
    if let Some(c) = first {
        if !OsParser::is_bare(c) && !matches!(c, 0x7b | 0x28 | 0x3c | 0x22) {
            return fail(p.i + 1, "extra data after parsed string");
        }
    }
    let v = p.value();
    let Some(v) = v else {
        if p.eos_in_data {
            return fail(p.u.len() + 1, "unexpected end of string when parsing data");
        }
        // The first character starts a value (checked above), so a failing value always consumed at least that
        // character: the offset is never 0 and every failure is reported this way.
        return fail(p.i + 1, "unexpected character (wanted '>')");
    };
    p.skip_ws();
    if p.i < p.u.len() {
        // reported at the first non-blank character after the value, 1-based (observed: d_garbage, z_ver).
        return fail(p.i + 1, "extra data after parsed string");
    }
    // The top-level parse always runs into the end of the text, which GNUstep records as an error while still
    // returning the value (observed for strings and dictionaries: l_str, c_openstep).
    ReadResult {
        value: Some(v),
        format: PlistFormat::OpenStep,
        error: Some(format!("Parse failed at line 1 (char {}) - reached end of string", p.i + 1)),
    }
}

// ---------------------------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------------------------

/// `propertyListWithData:options:format:error:`. Raises exactly where GNUstep raises.
pub fn property_list_with_data(d: &[u8], opts: u32) -> ReadResult {
    if d.is_empty() {
        return ReadResult {
            value: None,
            format: PlistFormat::None,
            error: Some("empty data argument passed to method".to_string()),
        };
    }
    if d.len() >= 8 && &d[..8] == b"bplist00" {
        return match binary::read_bplist(d, opts) {
            Ok(v) => ReadResult { value: v, format: PlistFormat::Binary, error: None },
            Err(e) => ns_raise(&e.name, e.reason.as_deref().unwrap_or("")),
        };
    }
    let mut k = 0;
    while k < d.len() && is_c_space(d[k]) {
        k += 1;
    }
    if d.len() >= k + 5 && d[k..k + 5].eq_ignore_ascii_case(b"<?xml") {
        return parse_xml(&d[k..], opts).with_leading_ws(k > 0);
    }
    parse_openstep(d, opts)
}

impl ReadResult {
    fn with_leading_ws(mut self, ws: bool) -> Self {
        if ws {
            self.value = None;
            self.error = Some(XML_FAIL.to_string());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plist::value::PlistValue as V;

    fn read(s: &str, opts: u32) -> ReadResult {
        property_list_with_data(s.as_bytes(), opts)
    }
    fn wrap(body: &str) -> String {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"0.9\">\n{}\n</plist>", body)
    }

    #[test]
    fn long_long_rules() {
        assert_eq!(parse_ns_long_long("9223372036854775808"), i64::MAX);
        assert_eq!(parse_ns_long_long("99999999999999999999"), i64::MAX);
        assert_eq!(parse_ns_long_long("- 5"), 0);
        assert_eq!(parse_ns_long_long("+-5"), 0);
        assert_eq!(parse_ns_long_long("12abc"), 12);
        assert_eq!(parse_ns_long_long("1e2"), 1);
        assert_eq!(parse_ns_long_long("-7"), -7);
    }

    #[test]
    fn xml_basic_and_classes() {
        let r = read(&wrap("<array><integer>2147483647</integer><integer>2147483646</integer><string>a&amp;b&#65;&#x42;</string><date>2026-10-16T20:24:45Z</date><data>\nYWJj\nZA==</data><true/></array>"), 0);
        assert_eq!(r.format, PlistFormat::Xml);
        let V::Array(a) = r.value.unwrap() else { panic!() };
        assert_eq!(a.items[0].class_name(), "NSLongLongNumber");
        assert_eq!(a.items[1].class_name(), "NSIntNumber");
        assert_eq!(a.items[2].as_str(), Some("a&bAB"));
        let V::Date(d) = a.items[3] else { panic!() };
        assert_eq!(d + 978307200.0, 1792182285.0);
        let V::Data(ref d) = a.items[4] else { panic!() };
        assert_eq!(d.bytes, b"abcd");
        let r2 = read(&wrap("<array/>"), 2);
        assert_eq!(r2.value.unwrap().class_name(), "GSMutableArray");
    }

    #[test]
    fn xml_failures_and_exceptions() {
        use crate::foundation::exception::catch_ns;
        let r = read(&wrap("<array><integer>1</integer>"), 0);
        assert!(r.value.is_none());
        assert_eq!(r.error.as_deref(), Some(XML_FAIL));
        let e = catch_ns(|| read(&wrap("<array><date>bad</date></array>"), 0)).unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        assert_eq!(e.reason.as_deref(), Some("Tried to add nil to array"));
        let e = catch_ns(|| read(&wrap("<dict><key>a</key><date>bad</date></dict>"), 0)).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("Tried to add nil value for key 'a' to dictionary"));
        assert!(read(&wrap(""), 0).value.is_none());
        assert!(read(&wrap("<dict><key>a</key><string>x</string><string>y</string></dict>"), 0).value.is_none());
        let r = read(&wrap("<dict><key>a</key><key>b</key><string>v</string></dict>"), 0);
        let v = r.value.unwrap();
        assert_eq!(v.object_for_key("b").and_then(|x| x.as_str()), Some("v"));
        assert!(v.object_for_key("a").is_none());
        // leading whitespace before the declaration
        let r = read(&format!("  {}", wrap("<array/>")), 0);
        assert!(r.value.is_none());
        assert_eq!(r.format, PlistFormat::Xml);
        // CDATA dropped
        let r = read(&wrap("<array><string><![CDATA[x<y]]></string></array>"), 0);
        let V::Array(a) = r.value.unwrap() else { panic!() };
        assert_eq!(a.items[0].as_str(), Some(""));
        // empty
        assert_eq!(property_list_with_data(b"", 0).error.as_deref(), Some("empty data argument passed to method"));
    }

    #[test]
    fn string_escapes() {
        use crate::foundation::exception::catch_ns;
        let get = |s: &str| -> Result<String, String> {
            match catch_ns(|| read(&wrap(&format!("<array><string>{}</string></array>", s)), 0)) {
                Ok(r) => {
                    let V::Array(a) = r.value.unwrap() else { panic!() };
                    Ok(a.items[0].as_str().unwrap().to_string())
                }
                Err(e) => Err(e.reason.unwrap()),
            }
        };
        assert_eq!(get("\\U0041").unwrap(), "A");
        assert_eq!(get("\\U12").unwrap(), "\\U12");
        assert_eq!(get("\\UD83D\\UDE00").unwrap(), "\\UDE00");
        assert_eq!(get("a\\UD83D\\UDE00b").unwrap(), "a\\UDE00b");
        assert_eq!(get("\\UD83Dx").unwrap(), "x");
        assert_eq!(get("\\UD83D").unwrap_err(), "Range location + length too great");
        assert_eq!(get("x\\UDE00").unwrap_err(), "Range location + length too great");
        assert_eq!(get("\\UD83D\\UDE00\\UD83D").unwrap_err(), "Range location + length too great");
        assert_eq!(get("\\UD83D\\UDE00\\UD83D\\UDE00").unwrap(), "\\UDE00\\UDE00");
        assert_eq!(get("\\UD83D\\UDE00\\UDE00x").unwrap(), "\\UDE00x");
        assert_eq!(get("&#x1F600;").unwrap(), "\u{F600}");
    }

    #[test]
    fn openstep_and_detection() {
        let r = property_list_with_data(b"{ a = 1; b = \"x y\"; c = (1, 2.5); d = <6162>; }", 0);
        assert_eq!(r.format, PlistFormat::OpenStep);
        let v = r.value.unwrap();
        assert_eq!(v.object_for_key("b").unwrap().as_str(), Some("x y"));
        assert!(property_list_with_data(b"garbage here", 0).value.is_none());
        assert!(property_list_with_data(b"<plist version=\"0.9\"><string>top</string></plist>", 0).value.is_none());
        let r = property_list_with_data(b"junk", 0);
        assert_eq!(r.value.unwrap().as_str(), Some("junk"));
        assert!(property_list_with_data(b"\xef\xbb\xbf<?xml version=\"1.0\"?><plist/>", 0).value.is_none());
        let r = property_list_with_data(b"<?XML version=\"1.0\"?><plist><array/></plist>", 0);
        assert_eq!(r.format, PlistFormat::Xml);
        assert!(r.value.is_some());
    }
}

#[cfg(test)]
mod edge_cases {
    use super::*;
    use crate::foundation::exception::catch_ns;
    use crate::plist::value::PlistValue as V;

    fn xml(body: &str) -> ReadResult {
        property_list_with_data(format!("<?xml version=\"1.0\"?>{body}").as_bytes(), 0)
    }
    fn os(s: &str) -> ReadResult {
        property_list_with_data(s.as_bytes(), 0)
    }
    fn bad_xml(body: &str) {
        let r = xml(body);
        assert!(r.value.is_none(), "{body}");
        assert_eq!(r.error.as_deref(), Some(XML_FAIL), "{body}");
    }

    #[test]
    fn number_grammar_edges() {
        assert_eq!(parse_ns_long_long("  \t12"), 12);
        assert_eq!(parse_ns_double(" \n2.5"), 2.5);
        assert_eq!(parse_ns_double("abc"), 0.0);
        assert_eq!(parse_ns_double("1e2"), 100.0);
        assert_eq!(parse_ns_double("1E+2"), 100.0);
        assert_eq!(parse_ns_double("1e-2"), 0.01);
        assert_eq!(parse_ns_double("1e+"), 0.0);
        assert!(parse_ns_double("1e5000000").is_infinite());
        assert_eq!(parse_ns_double("-1.5e1"), -15.0);
        assert_eq!(parse_ns_double("0e0"), 0.0);
        let r = xml("<plist><array><dict/><data>+/8=!</data></array></plist>");
        let V::Array(a) = r.value.unwrap() else { panic!() };
        let V::Data(d) = &a.items[1] else { panic!() };
        assert_eq!(d.bytes, [0xfb, 0xff]);
        assert_eq!(decode_entities("a&b"), "a&b".encode_utf16().collect::<Vec<_>>());
        assert!(parse_date("2026X").is_none());
    }

    #[test]
    fn entity_edges() {
        let _ = xml("<plist><string>a\tbc&amp;de</string></plist>");
        assert_eq!(decode_entities("&#zz;x"), "#zzx".encode_utf16().collect::<Vec<_>>());
        assert_eq!(decode_entities("&bogus;"), "bogus".encode_utf16().collect::<Vec<_>>());
        assert!(xml_string_split_storage("a\tbcd"));
        let r = xml("<plist><string>q&#xZZ;&nope;</string></plist>");
        assert_eq!(r.value.unwrap().as_str(), Some("q#xZZnope"));
    }

    #[test]
    fn date_forms() {
        assert!(parse_date("2026-10-16 20:24:45 +0100").is_some());
        assert_eq!(parse_date("2026-10-16 20:24:45 +0100"), parse_date("2026-10-16T19:24:45Z"));
        assert!(parse_date("2026-10-16 20:24:45 -0230").is_some());
        assert!(parse_date("2026-10-16 20:24:45 *0100").is_none());
        assert!(parse_date("2026-10-16X20:24:45Z").is_none());
        assert!(parse_date("2026-13-16T20:24:45Z").is_none());
        assert!(parse_date("2026-10-16T20:24:45Zjunk").is_none());
    }

    #[test]
    fn xml_comments_pis_and_doctype() {
        assert!(xml("<plist><!-- c --><array/></plist>").value.is_some());
        assert!(xml("<!DOCTYPE plist [ <!ENTITY a \"b\"> ]><plist><array/></plist>").value.is_some());
        bad_xml("<plist><!-- c");
        bad_xml("<plist><? x");
        bad_xml("<!DOCTYPE plist [ <!ENTITY a \"b\">");
        assert!(xml("").value.is_none());
        assert!(xml("").error.is_none());
        bad_xml("junk");
        bad_xml("<other/>");
        assert!(xml("<plist/>").value.is_none());
        assert!(xml("<plist/>").error.is_none());
        bad_xml("<plist>");
        bad_xml("<plist></notplist>");
        assert!(xml("<plist></plist>").value.is_none());
    }

    #[test]
    fn xml_container_edges() {
        let r = xml("<plist><dict><key>a</key><integer>1</integer><key>b</key><true/></dict></plist>");
        assert_eq!(r.value.unwrap().object_for_key("a").unwrap().long_long_value(), 1);
        assert!(xml("<plist><array><![CDATA[x]]><integer>1</integer>text</array></plist>").value.is_some());
        assert!(xml("<plist><array>text<integer>1</integer></array></plist>").value.is_some());
        bad_xml("<plist><array><![CDATA[x</array></plist>");
        bad_xml("<plist><array>text");
        bad_xml("<plist><array>");
        bad_xml("<plist><array><integer");
        bad_xml("<plist><array></array");
        bad_xml("<plist><array></dict></plist>");
        bad_xml("<plist><dict></array></plist>");
        bad_xml("<plist><dict><integer>1</integer></dict></plist>");
        bad_xml("<plist><array><unknown/></array></plist>");
        assert!(xml("<plist><dict><key/><string>v</string></dict></plist>").value.is_some());
    }

    #[test]
    fn xml_leaf_edges() {
        bad_xml("<plist><integer>");
        bad_xml("<plist><integer>12");
        bad_xml("<plist><string><![CDATA[x</string></plist>");
        assert_eq!(xml("<plist><string>a<![CDATA[x]]>b</string></plist>").value.unwrap().as_str(), Some("ab"));
        assert_eq!(xml("<plist><string>a<!-- c -->b</string></plist>").value.unwrap().as_str(), Some("ab"));
        bad_xml("<plist><string>a<!-- c</string></plist>");
        bad_xml("<plist><string><b/></string></plist>");
        bad_xml("<plist><string>x</integer></plist>");
    }

    #[test]
    fn xml_nil_values_raise() {
        let e = catch_ns(|| xml("<plist><array><date>nope</date></array></plist>")).unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        let e = catch_ns(|| xml("<plist><dict><key>k</key><date>nope</date></dict></plist>")).unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        assert!(xml("<plist><date>nope</date></plist>").value.is_none());
    }

    #[test]
    fn xml_declared_latin1_and_leading_space() {
        let mut d = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><plist><string>".to_vec();
        d.push(0xe9);
        d.extend_from_slice(b"</string></plist>");
        let r = property_list_with_data(&d, 0);
        assert_eq!(r.value.unwrap().as_str(), Some("\u{e9}"));
        let r = property_list_with_data(b" <?xml version=\"1.0\"?><plist/>", 0);
        assert_eq!(r.error.as_deref(), Some(XML_FAIL));
        // the document parser itself rejects leading whitespace and empty text
        let mk = |s| XmlParser {
            s,
            i: 0,
            tag_cont: ClassTag { origin: Origin::Xml, mutable: false, alt: false },
            tag_leaf: ClassTag { origin: Origin::Xml, mutable: false, alt: false },
        };
        assert!(mk(" <plist/>").document().is_err());
        assert!(mk("").document().is_err());
    }

    #[test]
    fn openstep_comments_and_containers() {
        assert!(os("// line\n{a = b;}").value.is_some());
        assert!(os("/* block */ (a, b)").value.is_some());
        assert!(os("/* open").value.is_none());
        let v = os("{lonely; b = c;}").value.unwrap();
        let V::Dict(d) = v else { panic!() };
        assert_eq!(d.map.len(), 2);
        assert!(os("{a = b}").value.is_none());
        assert!(os("{a = b;").value.is_none());
        assert!(os("{(x) = b;}").value.is_none());
        assert!(os("(a b)").value.is_none());
        assert!(os("(a, b").value.is_none());
        assert!(os("(a,)").value.is_some());
        assert!(os("(a,").value.is_none());
        assert!(os("{ = x;}").value.is_none());
    }

    #[test]
    fn openstep_data_and_strings() {
        assert!(os("<0a 0b>").value.is_some());
        assert!(os("<0a0>").value.is_none());
        let r = os("<0a0b");
        assert!(r.value.is_none());
        assert!(r.error.unwrap().contains("end of string when parsing data"));
        let r = os("\"a\\nb\\tc\\rd\\U0041\\101\\7\\q\"");
        assert_eq!(r.value.unwrap().as_str(), Some("a\nb\tc\rdAA\u{7}q"));
        assert!(os("\"trail\\").value.is_none());
        assert!(os("\"unterminated").value.is_none());
        let r = os("a b");
        assert!(r.error.unwrap().contains("extra data"));
        let r = os("{a = <}");
        assert!(r.error.unwrap().contains("wanted '>'"));
        let e = catch_ns(|| os("\0")).unwrap_err();
        assert_eq!(e.name, "NSRangeException");
    }

    #[test]
    fn corrupt_binary_raises() {
        let mut d = b"bplist00".to_vec();
        d.extend_from_slice(&[0u8; 32]);
        let e = catch_ns(|| property_list_with_data(&d, 0)).unwrap_err();
        assert_eq!(e.name, "NSGenericException");
    }
}

#[cfg(test)]
#[path = "xml_read_golden.rs"]
mod golden;
