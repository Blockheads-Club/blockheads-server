//! Replays of the private getopt corpus against the real libc (skipped without fixtures).

use super::*;

fn dir() -> String {
    crate::testenv::modules("app_cli_getopt")
}

/// Same output format as ``.
fn run(corpus: &[u8]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut optopt = 0;
    let text = corpus;
    let mut lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
    lines.pop();
    let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    for (n, line) in lines.iter().enumerate() {
        let args: Vec<Vec<u8>> =
            if line.is_empty() { vec![] } else { line.split(|&b| b == b'\t').map(|s| s.to_vec()).collect() };
        let mut argv = vec![b"/opt/blockheads/blockheads_server171".to_vec()];
        argv.extend(args.iter().cloned());
        let mut hdr = format!("### {}:", n);
        for a in &args {
            hdr.push_str(&format!(" [{}]", lossy(a)));
        }
        out.push(hdr);
        let mut g = Getopt::new(argv);
        g.set_optopt(optopt);
        loop {
            let mut diag: Vec<String> = Vec::new();
            let o = g.next(&mut |m| diag.push(lossy(m)));
            out.extend(diag);
            if o == -1 {
                break;
            }
            let ch = if o > 32 && o < 127 { (o as u8 as char).to_string() } else { ".".to_string() };
            let oa = g.optarg().map(lossy).unwrap_or_else(|| "(null)".to_string());
            out.push(format!("opt={}('{}') optarg={} optopt={} optind={}", o, ch, oa, g.optopt(), g.optind()));
            if o == b'h' as i32 || o == b'?' as i32 {
                break;
            }
        }
        optopt = g.optopt();
        let mut e = format!("end optind={} rest:", g.optind());
        for a in &g.argv()[1..] {
            e.push_str(&format!(" [{}]", lossy(a)));
        }
        out.push(e);
    }
    out.join("\n")
}

fn check(tsv: &str, expected: &str) {
    let (Ok(corpus), Ok(exp)) =
        (std::fs::read(format!("{}/{tsv}", dir())), std::fs::read_to_string(format!("{}/{expected}", dir())))
    else {
        eprintln!("SKIP: private getopt corpus unavailable (set BH_PRIVATE_DOCS)");
        return;
    };
    let got = run(&corpus);
    let (g, e): (Vec<&str>, Vec<&str>) = (got.split('\n').collect(), exp.trim_end_matches('\n').split('\n').collect());
    for (i, (a, b)) in g.iter().zip(e.iter()).enumerate() {
        assert_eq!(a, b, "{tsv} line {i}");
    }
    assert_eq!(g.len(), e.len(), "{tsv} line count");
}

/// 220 hand vectors against the real libc.
#[test]
fn corpus() {
    check("corpus.tsv", "corpus_expected.txt");
}

/// 3000 fuzz vectors against the real libc.
#[test]
fn fuzz() {
    check("fuzz.tsv", "fuzz_expected.txt");
}
