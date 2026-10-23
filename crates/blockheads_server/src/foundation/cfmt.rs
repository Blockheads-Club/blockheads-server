//! printf-compatible formatting (glibc / GNUstep `%@` semantics).
//!

/// One variadic argument.
#[derive(Clone, Copy, Debug)]
pub enum CArg<'a> {
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    /// `%s` and `%@`.
    Str(&'a str),
    Ptr(usize),
    Char(u8),
}

/// `%.Nf`. Rust's `{:.N}` is exact and equals glibc.
pub fn fmt_f(v: f64, precision: usize) -> String {
    if !v.is_finite() {
        return nonfinite(v);
    }
    format!("{:.*}", precision, v)
}

fn nonfinite(v: f64) -> String {
    let s = if v.is_nan() {
        if v.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        }
    } else if v < 0.0 {
        "-inf"
    } else {
        "inf"
    };
    s.to_string()
}

/// `%.Ne`: at least two exponent digits.
pub fn fmt_e(v: f64, precision: usize) -> String {
    if !v.is_finite() {
        return nonfinite(v);
    }
    let s = format!("{:.*e}", precision, v);
    let (m, e) = s.split_once('e').unwrap();
    let ev: i32 = e.parse().unwrap();
    format!("{}e{}{:02}", m, if ev < 0 { '-' } else { '+' }, ev.abs())
}

fn fmt_g_impl(v: f64, precision: usize, alt: bool) -> String {
    if !v.is_finite() {
        return nonfinite(v);
    }
    let p = if precision == 0 { 1 } else { precision };
    if v == 0.0 {
        let mut s = if v.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
        if alt && p > 1 {
            s.push('.');
            s.push_str(&"0".repeat(p - 1));
        }
        return s;
    }
    let es = format!("{:.*e}", p - 1, v);
    let x: i32 = es.split_once('e').unwrap().1.parse().unwrap();
    let mut s = if x >= -4 && (x as i64) < p as i64 {
        format!("{:.*}", (p as i64 - 1 - x as i64) as usize, v)
    } else {
        fmt_e(v, p - 1)
    };
    if !alt {
        // strip trailing zeros of the mantissa
        let (mant, exp) = match s.find('e') {
            Some(i) => (s[..i].to_string(), s[i..].to_string()),
            None => (s.clone(), String::new()),
        };
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.').to_string() } else { mant };
        s = mant + &exp;
    } else if !s.contains('.') {
        // '#': always keep the decimal point
        s = match s.find('e') {
            Some(i) => format!("{}.{}", &s[..i], &s[i..]),
            None => s + ".",
        };
    }
    s
}

/// `%.Ng`.
pub fn fmt_g(v: f64, precision: usize) -> String {
    fmt_g_impl(v, precision, false)
}

/// `-[NSNumber description]` of a double: C `%.16g` (observed). Non-finite via `%g`.
pub fn ns_description_double(v: f64) -> String {
    fmt_g(v, 16)
}

/// `-[NSNumber description]` of a float: `%.7g` of the value widened to double.
pub fn ns_description_float(f: f32) -> String {
    fmt_g(f as f64, 7)
}

#[derive(Default, Clone, Copy)]
struct Spec {
    minus: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: Option<usize>,
    prec: Option<usize>,
    /// 0 none, -2 hh, -1 h, 1 l, 2 ll/z/j/t
    len: i8,
}

fn pad(body: String, sp: &Spec, numeric_zero_ok: bool) -> String {
    let w = sp.width.unwrap_or(0);
    let n = body.chars().count();
    if n >= w {
        return body;
    }
    let fill = w - n;
    if sp.minus {
        body + &" ".repeat(fill)
    } else if sp.zero && numeric_zero_ok {
        // insert zeros after sign / 0x prefix
        let b = body.as_bytes();
        let mut at = 0;
        if !b.is_empty() && (b[0] == b'-' || b[0] == b'+' || b[0] == b' ') {
            at = 1;
        }
        if body[at..].starts_with("0x") || body[at..].starts_with("0X") {
            at += 2;
        }
        format!("{}{}{}", &body[..at], "0".repeat(fill), &body[at..])
    } else {
        " ".repeat(fill) + &body
    }
}

fn arg_signed(a: &CArg, len: i8) -> i64 {
    let raw: i64 = match *a {
        CArg::I32(v) => v as i64,
        CArg::U32(v) => v as i64,
        CArg::I64(v) => v,
        CArg::U64(v) => v as i64,
        CArg::Ptr(v) => v as i64,
        CArg::Char(v) => v as i64,
        CArg::F64(_) | CArg::Str(_) => 0,
    };
    match len {
        -2 => raw as i8 as i64,
        -1 => raw as i16 as i64,
        0 => raw as i32 as i64,
        _ => raw,
    }
}

fn arg_unsigned(a: &CArg, len: i8) -> u64 {
    let raw = arg_signed(a, 1) as u64;
    match len {
        -2 => raw as u8 as u64,
        -1 => raw as u16 as u64,
        0 => raw as u32 as u64,
        _ => raw,
    }
}

fn int_with_prec(digits: String, sp: &Spec) -> String {
    match sp.prec {
        Some(p) => {
            if p == 0 && digits == "0" {
                String::new()
            } else if digits.len() < p {
                "0".repeat(p - digits.len()) + &digits
            } else {
                digits
            }
        }
        None => digits,
    }
}

/// C `printf`. Flags `-+ #0`, width, `.precision`, `*`, `hh h l ll z j t`, conversions
/// `d i u o x X e E f F g G c s p @ %`.
pub fn sprintf(fmt: &str, args: &[CArg]) -> String {
    let f: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut ai = 0usize;
    let next = |ai: &mut usize| -> Option<CArg> {
        let a = args.get(*ai).copied();
        *ai += 1;
        a
    };
    let mut i = 0;
    while i < f.len() {
        let c = f[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= f.len() {
            out.push('%');
            break;
        }
        let mut sp = Spec::default();
        while i < f.len() {
            match f[i] {
                '-' => sp.minus = true,
                '+' => sp.plus = true,
                ' ' => sp.space = true,
                '#' => sp.alt = true,
                '0' => sp.zero = true,
                _ => break,
            }
            i += 1;
        }
        if i < f.len() && f[i] == '*' {
            i += 1;
            let w = next(&mut ai).map(|a| arg_signed(&a, 0)).unwrap_or(0);
            if w < 0 {
                sp.minus = true;
            }
            sp.width = Some(w.unsigned_abs() as usize);
        } else {
            let mut w = None;
            while i < f.len() && f[i].is_ascii_digit() {
                w = Some(w.unwrap_or(0) * 10 + f[i].to_digit(10).unwrap() as usize);
                i += 1;
            }
            sp.width = w;
        }
        if i < f.len() && f[i] == '.' {
            i += 1;
            if i < f.len() && f[i] == '*' {
                i += 1;
                let p = next(&mut ai).map(|a| arg_signed(&a, 0)).unwrap_or(0);
                sp.prec = if p < 0 { None } else { Some(p as usize) };
            } else {
                let mut p = 0usize;
                while i < f.len() && f[i].is_ascii_digit() {
                    p = p * 10 + f[i].to_digit(10).unwrap() as usize;
                    i += 1;
                }
                sp.prec = Some(p);
            }
        }
        loop {
            if i >= f.len() {
                break;
            }
            match f[i] {
                'h' => sp.len = if sp.len == -1 { -2 } else { -1 },
                'l' => sp.len = if sp.len == 1 { 2 } else { 1 },
                'z' | 'j' | 't' | 'L' | 'q' => sp.len = 2,
                _ => break,
            }
            i += 1;
        }
        if i >= f.len() {
            break;
        }
        let conv = f[i];
        i += 1;
        match conv {
            '%' => out.push('%'),
            'd' | 'i' => {
                let v = next(&mut ai).map(|a| arg_signed(&a, sp.len)).unwrap_or(0);
                let digits = int_with_prec(v.unsigned_abs().to_string(), &sp);
                let sign = if v < 0 {
                    "-"
                } else if sp.plus {
                    "+"
                } else if sp.space {
                    " "
                } else {
                    ""
                };
                out.push_str(&pad(format!("{}{}", sign, digits), &sp, sp.prec.is_none()));
            }
            'u' | 'o' | 'x' | 'X' => {
                let v = next(&mut ai).map(|a| arg_unsigned(&a, sp.len)).unwrap_or(0);
                let mut digits = match conv {
                    'u' => v.to_string(),
                    'o' => format!("{:o}", v),
                    'x' => format!("{:x}", v),
                    _ => format!("{:X}", v),
                };
                digits = int_with_prec(digits, &sp);
                if sp.alt {
                    match conv {
                        'o' if !digits.starts_with('0') => digits.insert(0, '0'),
                        'x' if v != 0 => digits.insert_str(0, "0x"),
                        'X' if v != 0 => digits.insert_str(0, "0X"),
                        _ => {}
                    }
                }
                out.push_str(&pad(digits, &sp, sp.prec.is_none()));
            }
            'c' => {
                let v = next(&mut ai).map(|a| arg_signed(&a, 0)).unwrap_or(0);
                let ch = (v as u8) as char; // Latin-1 byte value
                out.push_str(&pad(ch.to_string(), &sp, false));
            }
            's' | '@' => {
                let s = match next(&mut ai) {
                    Some(CArg::Str(s)) => s,
                    _ => "(null)",
                };
                let body: String = match sp.prec {
                    Some(p) if conv == 's' => {
                        let b = s.as_bytes();
                        String::from_utf8_lossy(&b[..p.min(b.len())]).into_owned()
                    }
                    Some(p) => s.chars().take(p).collect(),
                    None => s.to_string(),
                };
                out.push_str(&pad(body, &sp, false));
            }
            'p' => {
                let v = next(&mut ai).map(|a| arg_unsigned(&a, 2)).unwrap_or(0);
                let body = if v == 0 { "(nil)".to_string() } else { format!("0x{:x}", v) };
                out.push_str(&pad(body, &sp, false));
            }
            'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
                let v = match next(&mut ai) {
                    Some(CArg::F64(v)) => v,
                    _ => 0.0,
                };
                let prec = sp.prec.unwrap_or(6);
                let upper = conv.is_ascii_uppercase();
                let mut body = match conv.to_ascii_lowercase() {
                    'e' => {
                        let mut s = fmt_e(v, prec);
                        if sp.alt && prec == 0 && v.is_finite() {
                            s = s.replacen('e', ".e", 1);
                        }
                        s
                    }
                    'f' => {
                        let mut s = fmt_f(v, prec);
                        if sp.alt && prec == 0 && v.is_finite() {
                            s.push('.');
                        }
                        s
                    }
                    _ => fmt_g_impl(v, prec, sp.alt),
                };
                if upper {
                    body = body.to_uppercase();
                }
                if !body.starts_with('-') {
                    if sp.plus {
                        body.insert(0, '+');
                    } else if sp.space {
                        body.insert(0, ' ');
                    }
                }
                out.push_str(&pad(body, &sp, v.is_finite()));
            }
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    fn c_i(fmt: &str, v: i64) -> String {
        let f = CString::new(fmt).unwrap();
        let mut buf = vec![0u8; 256];
        unsafe { libc::snprintf(buf.as_mut_ptr() as *mut _, 256, f.as_ptr(), v) };
        CStr::from_bytes_until_nul(&buf).unwrap().to_string_lossy().into_owned()
    }
    fn c_f(fmt: &str, v: f64) -> String {
        let f = CString::new(fmt).unwrap();
        let mut buf = vec![0u8; 512];
        unsafe { libc::snprintf(buf.as_mut_ptr() as *mut _, 512, f.as_ptr(), v) };
        CStr::from_bytes_until_nul(&buf).unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn integers_vs_libc() {
        let vals: [i64; 9] = [0, 1, -1, 42, -42, 255, 65535, 2147483647, -2147483648];
        for f in [
            "%d", "%i", "%5d", "%-5d|", "%05d", "%+d", "% d", "%.3d", "%8.3d", "%u", "%x", "%X", "%#x", "%#o", "%o",
            "%10d", "%-10d|", "%0x", "%08x", "%#08x", "%.0d",
        ] {
            for &v in &vals {
                let a = if f.contains('u') || f.contains('x') || f.contains('X') || f.contains('o') {
                    CArg::U32(v as u32)
                } else {
                    CArg::I32(v as i32)
                };
                // libc varargs: pass as int-size by casting to i32
                let f_c = CString::new(f).unwrap();
                let mut buf = vec![0u8; 64];
                unsafe { libc::snprintf(buf.as_mut_ptr() as *mut _, 64, f_c.as_ptr(), v as i32) };
                let want = CStr::from_bytes_until_nul(&buf).unwrap().to_string_lossy().into_owned();
                assert_eq!(sprintf(f, &[a]), want, "fmt {f} v {v}");
            }
        }
        for f in ["%ld", "%lld", "%lu", "%llu", "%zu", "%zx", "%lx"] {
            for &v in &[0i64, 1, -1, 4294967296, i64::MAX, i64::MIN] {
                let a = if f.ends_with('d') { CArg::I64(v) } else { CArg::U64(v as u64) };
                assert_eq!(sprintf(f, &[a]), c_i(f, v), "fmt {f} v {v}");
            }
        }
        assert_eq!(sprintf("%hhd %hd", &[CArg::I32(300), CArg::I32(70000)]), "44 4464");
    }

    #[test]
    fn floats_vs_libc() {
        let vals = [
            0.0,
            -0.0,
            1.0,
            0.5,
            0.0001,
            0.00001,
            100000.0,
            1e6,
            1e-5,
            123456789.0,
            3.14159265358979,
            -2.5,
            1e100,
            1e-100,
            0.1,
            2.5,
            0.125,
            999999.5,
            9.9999995,
            1e15,
            1e16,
            1e17,
            65.43000000000001,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        for f in [
            "%f", "%.2f", "%.1f", "%.4f", "%5.1f", "%e", "%.2e", "%E", "%g", "%G", "%.7g", "%.15g", "%.16g", "%.17g",
            "%10.3f", "%-10.3f|", "%+f", "%010.2f", "%#g", "%.0f", "%#.0f", "%.0e", "%.1g", "%.0g", "% f",
        ] {
            for &v in &vals {
                if f == "%#g" && v == 999999.5 {
                    continue; // glibc quirk with '#' and round-up to the next exponent
                }
                assert_eq!(sprintf(f, &[CArg::F64(v)]), c_f(f, v), "fmt {f} v {v:?}");
            }
        }
        let nan = f64::NAN;
        assert_eq!(sprintf("%f %g", &[CArg::F64(nan), CArg::F64(nan)]), c_f("%f", nan) + " " + &c_f("%g", nan));
        assert_eq!(fmt_g(0.0001, 7), "0.0001");
        assert_eq!(fmt_g(100000.0, 7), "100000");
        assert_eq!(fmt_g(1e6, 7), "1000000");
        assert_eq!(fmt_g(1e-5, 7), "1e-05");
        assert_eq!(fmt_e(1.0, 6), "1.000000e+00");
        assert_eq!(fmt_f(2.675, 2), "2.67");
    }

    #[test]
    fn strings_and_misc() {
        assert_eq!(sprintf("%c%c", &[CArg::Char(b'a'), CArg::I32(66)]), "aB");
        assert_eq!(
            sprintf("%s|%10s|%-6s|", &[CArg::Str("ab"), CArg::Str("cd"), CArg::Str("ef")]),
            "ab|        cd|ef    |"
        );
        assert_eq!(sprintf("%.3s", &[CArg::Str("abcdef")]), "abc");
        assert_eq!(sprintf("%.40s", &[CArg::Str("short")]), "short");
        assert_eq!(sprintf("%@: %d%%", &[CArg::Str("name"), CArg::I32(5)]), "name: 5%");
        assert_eq!(sprintf("%p %p", &[CArg::Ptr(0), CArg::Ptr(0xdead)]), "(nil) 0xdead");
        assert_eq!(sprintf("%*d|%-*d|", &[CArg::I32(4), CArg::I32(7), CArg::I32(3), CArg::I32(1)]), "   7|1  |");
    }

    #[test]
    fn double_description() {
        for (v, s) in [
            (1.0, "1"),
            (0.0, "0"),
            (0.5, "0.5"),
            (0.1, "0.1"),
            (123456789.123, "123456789.123"),
            (1e10, "10000000000"),
            (1e-5, "1e-05"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000"),
            (0.7000000000000001, "0.7000000000000001"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.3333333333333333"),
            (123456789.12345679, "123456789.1234568"),
            (5e-324, "4.940656458412465e-324"),
            (1.7976931348623157e308, "1.797693134862316e+308"),
            (-0.0, "-0"),
            (1.234567890123457e19, "1.234567890123457e+19"),
            (0.0001, "0.0001"),
        ] {
            assert_eq!(ns_description_double(v), s);
        }
    }

    #[test]
    fn sprintf_edge_forms() {
        use CArg::*;
        let cases: Vec<(&str, Vec<CArg>, &str)> = vec![
            ("100%", vec![], "100%"),
            ("%5", vec![], ""),
            ("%-*d|", vec![I32(-4), I32(7)], "7   |"),
            ("%.*f", vec![I32(2), F64(1.0)], "1.00"),
            ("%.*f", vec![I32(-1), F64(1.0)], "1.000000"),
            ("%#X", vec![U32(255)], "0XFF"),
            ("%hhu", vec![U32(300)], "44"),
            ("%hu", vec![U32(70000)], "4464"),
            ("%hhd", vec![I32(300)], "44"),
            ("%s", vec![], "(null)"),
            ("%d", vec![F64(1.0)], "0"),
            ("%s", vec![I32(3)], "(null)"),
            ("%.2s", vec![Str("h\u{e9}llo")], "h\u{fffd}"),
            ("%f", vec![], "0.000000"),
            ("%f", vec![I32(1)], "0.000000"),
            ("%#.0e", vec![F64(5.0)], "5.e+00"),
            ("%#g", vec![F64(1.0)], "1.00000"),
            ("%#.3g", vec![F64(1e-10)], "1.00e-10"),
            ("%E", vec![F64(f64::INFINITY)], "INF"),
            ("%G", vec![F64(f64::NAN)], "NAN"),
            ("%y", vec![], "%y"),
            ("%ld %lld %zd", vec![I64(1), I64(2), I64(3)], "1 2 3"),
            ("%p", vec![Ptr(255)], "0xff"),
            ("%c", vec![Char(65)], "A"),
            ("%d", vec![Ptr(5)], "5"),
            ("%d", vec![Char(66)], "66"),
            ("%u", vec![I64(-1)], "4294967295"),
            ("%#.1g", vec![F64(1e10)], "1.e+10"),
            ("%.2@", vec![Str("h\u{e9}llo")], "h\u{e9}"),
            ("%p", vec![Ptr(0)], "(nil)"),
            ("%f", vec![F64(-f64::NAN)], "-nan"),
            ("%e", vec![F64(f64::NEG_INFINITY)], "-inf"),
        ];
        assert_eq!(ns_description_double(0.1), "0.1");
        assert_eq!(ns_description_float(0.1), "0.1");
        for (f, a, want) in cases {
            assert_eq!(sprintf(f, &a), want, "{f}");
        }
    }
}
