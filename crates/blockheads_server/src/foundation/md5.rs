//! MD5 categories (`NSData(MD5Addition)`, `NSString(MD5Addition)`).
//!

use ::md5::{Digest, Md5};

/// 16-byte MD5 digest.
pub fn md5_digest(data: &[u8]) -> [u8; 16] {
    let mut h = Md5::new();
    h.update(data);
    h.finalize().into()
}

/// `-[NSData stringFromMD5]`: 32 lowercase hex chars.
pub fn data_string_from_md5(data: &[u8]) -> String {
    md5_digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// `-[NSString stringFromMD5]`: MD5 of the UTF-8 bytes.
pub fn string_from_md5(s: &str) -> String {
    data_string_from_md5(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden() {
        assert_eq!(string_from_md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(string_from_md5("hello world"), "5eb63bbbe01eeed093cb22bb8f5acdc3");
        assert_eq!(string_from_md5("caf\u{e9}"), "07117fe4a1ebd544965dc19573183da2");
        assert_eq!(data_string_from_md5(&[]), "d41d8cd98f00b204e9800998ecf8427e");
    }
}
