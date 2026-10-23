//! `NSData(CompressionMethods)`: gzipDeflate / gzipInflate (system zlib via flate2).
//!

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

/// `-[NSData gzipDeflate]`: deflateInit2(level -1, Z_DEFLATED, windowBits 31, memLevel 8, default strategy),
/// single `Z_FINISH` loop. Empty input returns empty.
pub fn gzip_deflate(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut c = Compress::new_gzip(Compression::new(6), 15);
    let mut out: Vec<u8> = Vec::with_capacity(0x4000);
    loop {
        if out.capacity() - out.len() < 0x400 {
            out.reserve(0x4000);
        }
        let in_pos = c.total_in() as usize;
        match c.compress_vec(&data[in_pos..], &mut out, FlushCompress::Finish) {
            // deflate's return code is ignored by the original
            Ok(Status::StreamEnd) | Err(_) => break,
            Ok(_) => continue,
        }
    }
    // The gzip header's OS byte is zlib's compile-time OS_CODE: 3 (Unix) in the original Linux server, but
    // 19 on macOS and 10 on Windows builds of the bundled zlib. It is not covered by any checksum, so pin it
    // to the original's value: the bytes on the wire and in saves are then identical on every host.
    if out.len() > 9 {
        out[9] = 3;
    }
    out
}

/// `-[NSData gzipInflate]`: windowBits 47 (auto gzip/zlib), first member only, trailing bytes ignored,
/// `None` on corrupt or truncated input, empty input returns `Some(empty)`.
pub fn gzip_inflate(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return Some(Vec::new());
    }
    let mut d = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        Decompress::new_gzip(15)
    } else {
        Decompress::new(true)
    };
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + (data.len() >> 1));
    loop {
        if out.capacity() == out.len() {
            out.reserve((data.len() >> 1).max(64));
        }
        let in_pos = d.total_in() as usize;
        let before_in = d.total_in();
        let before_out = d.total_out();
        match d.decompress_vec(&data[in_pos.min(data.len())..], &mut out, FlushDecompress::Sync) {
            Ok(Status::StreamEnd) => return Some(out),
            // No progress with room left in the output means the input ended early (zlib reports that as a buffer
            // error); the output is grown before every call, so it is never full here.
            Ok(Status::Ok | Status::BufError) => {
                if d.total_in() == before_in && d.total_out() == before_out && out.capacity() != out.len() {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    #[test]
    fn golden_hello() {
        assert_eq!(hex(&gzip_deflate(b"hello")), "1f8b0800000000000003cb48cdc9c9070086a6103605000000");
    }

    #[test]
    fn roundtrip() {
        assert!(gzip_deflate(&[]).is_empty());
        assert_eq!(gzip_inflate(&[]), Some(vec![]));
        let zeros = vec![0u8; 100_000];
        let g = gzip_deflate(&zeros);
        assert_eq!(&g[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]);
        assert_eq!(gzip_inflate(&g).unwrap(), zeros);
        let mut x = 12345u32;
        let rnd: Vec<u8> = (0..300_000)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 24) as u8
            })
            .collect();
        assert_eq!(gzip_inflate(&gzip_deflate(&rnd)).unwrap(), rnd);
    }

    #[test]
    fn identical_to_system_zlib() {
        use crate::foundation::md5::data_string_from_md5 as md5;
        let g = gzip_deflate(&vec![0u8; 100_000]);
        assert_eq!((md5(&g).as_str(), g.len()), ("781d85846e95592e564bead19c048178", 132));
        let mut x = 12345u32;
        let rnd: Vec<u8> = (0..300_000)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 24) as u8
            })
            .collect();
        let g = gzip_deflate(&rnd);
        assert_eq!((md5(&g).as_str(), g.len()), ("cc03e2f6b6fd5f3cb461315b63af81ac", 300113));
        let g = gzip_deflate(&"The quick brown fox jumps over the lazy dog. ".repeat(5000).into_bytes());
        assert_eq!((md5(&g).as_str(), g.len()), ("e6d040cca2c9dccab36cd87606d149a5", 733));
    }

    #[test]
    fn inflate_edge_cases() {
        let g = gzip_deflate(b"hello world hello world");
        assert_eq!(gzip_inflate(&g[..g.len() - 5]), None);
        let mut j = g.clone();
        j.extend_from_slice(b"junk");
        assert_eq!(gzip_inflate(&j).unwrap(), b"hello world hello world");
        let mut two = g.clone();
        two.extend_from_slice(&gzip_deflate(b"second"));
        assert_eq!(gzip_inflate(&two).unwrap(), b"hello world hello world");
        // zlib wrapped "hello"
        let z = [0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15];
        assert_eq!(gzip_inflate(&z).unwrap(), b"hello");
        assert_eq!(gzip_inflate(b"this is not compressed data"), None);
    }

    #[test]
    fn corrupt_and_truncated_streams() {
        let good = gzip_deflate(&vec![7u8; 5000]);
        assert_eq!(gzip_inflate(&good).unwrap(), vec![7u8; 5000]);
        for cut in 1..good.len() {
            assert!(gzip_inflate(&good[..cut]).is_none(), "cut at {cut}");
        }
        assert!(gzip_inflate(&good[..good.len() / 2]).is_none());
        assert!(gzip_inflate(&good[..good.len() - 4]).is_none());
        let mut bad = good.clone();
        bad[12] ^= 0xff;
        bad[13] ^= 0xff;
        assert!(gzip_inflate(&bad).is_none());
        assert!(gzip_inflate(b"not compressed at all").is_none());
        assert_eq!(gzip_inflate(&[]).unwrap(), Vec::<u8>::new());
    }
}
