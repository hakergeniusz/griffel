//! Text decoding/encoding for document load and save
//! (`05-file-formats-and-persistence.md` §1).
//!
//! Loading: UTF-8 first, then charset detection. Saving: the file's
//! original encoding, falling back to UTF-8 when a character cannot be
//! represented. CRLF bytes are preserved verbatim (no newline conversion).

use encoding_rs::Encoding;

/// Decoded document text plus the encoding label used to re-encode on save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: &'static str,
}

/// Decode raw file bytes. Returns an error when no encoding produced valid
/// text.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    // UTF-8 first (BOM handled: a UTF-8 BOM is stripped, other BOMs are
    // handled by their decoders).
    let (text, _, had_errors) = encoding_rs::UTF_8.decode(bytes);
    if !had_errors {
        return Ok(Decoded {
            text: strip_utf8_bom(&text).to_owned(),
            encoding: "UTF-8",
        });
    }

    // Charset detection on the raw bytes.
    let mut detector =
        chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    detector.feed(bytes, true);
    let enc = detector.guess(None, chardetng::Utf8Detection::Allow);
    let (text, _, had_errors) = enc.decode(bytes);
    if !had_errors {
        return Ok(Decoded {
            text: text.into_owned(),
            encoding: enc.name(),
        });
    }

    // Last resort: UTF-16 BOMs decode losslessly via encoding_rs BOM
    // sniffing above; anything else is rejected.
    Err("the file is not valid text in any detected encoding".into())
}

fn strip_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Encode `text` with `encoding`. If some characters cannot be represented
/// in that encoding, retries as UTF-8 and reports the switched label
/// (`06-edge-cases.md` §1). UTF-8 encoding cannot fail, so an error here is
/// only possible for a broken input encoding label.
pub fn encode(text: &str, encoding: &str) -> Result<(Vec<u8>, String), String> {
    let enc = Encoding::for_label(encoding.as_bytes())
        .ok_or_else(|| format!("unknown encoding: {encoding}"))?;
    let (bytes, _, had_errors) = enc.encode(text);
    if !had_errors {
        return Ok((bytes.into_owned(), encoding.to_owned()));
    }
    // Retry as UTF-8; encode into a UTF-8 byte string never fails.
    let (bytes, _, had_errors) = encoding_rs::UTF_8.encode(text);
    if had_errors {
        return Err("the text cannot be encoded as UTF-8".into());
    }
    Ok((bytes.into_owned(), "UTF-8".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_roundtrip() {
        let text = "héllo — ünïcode ✓\nline2\r\nline3";
        let (bytes, enc) = encode(text, "UTF-8").unwrap();
        assert_eq!(enc, "UTF-8");
        let d = decode(&bytes).unwrap();
        assert_eq!(d.text, text);
        assert_eq!(d.encoding, "UTF-8");
    }

    #[test]
    fn utf8_bom_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hello".as_bytes());
        let d = decode(&bytes).unwrap();
        assert_eq!(d.text, "hello");
    }

    #[test]
    fn latin1_detected_and_preserved() {
        // "héllo" in windows-1252: é = 0xE9.
        let bytes = b"h\xe9llo";
        let d = decode(bytes).unwrap();
        assert_eq!(d.text, "h\u{e9}llo");
        assert_ne!(d.encoding, "UTF-8");
        // Re-encode in the detected encoding round-trips.
        let (out, _) = encode(&d.text, d.encoding).unwrap();
        assert_eq!(out, bytes.to_vec());
    }

    #[test]
    fn latin1_save_falls_back_to_utf8() {
        // Em dash cannot be encoded in windows-1252... actually it can
        // (0x97). Use a character outside it: '☃'.
        let (bytes, effective) = encode("snow ☃", "windows-1252").unwrap();
        assert_eq!(effective, "UTF-8");
        let d = decode(&bytes).unwrap();
        assert_eq!(d.text, "snow ☃");
    }

    #[test]
    fn utf16le_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "héllo".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let d = decode(&bytes).unwrap();
        assert_eq!(d.text, "héllo");
    }

    #[test]
    fn invalid_bytes_rejected() {
        // A lone invalid UTF-8 byte that chardetng still cannot fix as text
        // in a multi-byte encoding without errors is rare; feed something
        // that decodes with errors in every candidate.
        let bytes = [0xFF, 0xFE, 0x00, 0xD8, 0x00]; // truncated UTF-16 surrogate
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn empty_file() {
        let d = decode(b"").unwrap();
        assert_eq!(d.text, "");
    }

    #[test]
    fn crlf_preserved() {
        let d = decode(b"a\r\nb").unwrap();
        assert_eq!(d.text, "a\r\nb");
    }
}
