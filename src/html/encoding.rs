//! HTML byte-stream encoding sniffing. Transport metadata is the final response's
//! Content-Type value; local files have no transport metadata.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252, X_USER_DEFINED};

use crate::Error;

const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
const PRESCAN_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingSource {
    Bom,
    Transport,
    Meta,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedHtml {
    /// Newlines are normalized by the tokenizer before tokenization.
    pub text: String,
    pub encoding: &'static str,
    pub source: EncodingSource,
    pub had_decoding_errors: bool,
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn ascii_lower(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn label(bytes: &[u8]) -> Option<&'static Encoding> {
    Encoding::for_label(bytes)
}

fn transport_encoding(content_type: &str) -> Option<&'static Encoding> {
    // A Content-Type parameter can be quoted; a semicolon within quotes is not
    // a parameter separator. An invalid or unsupported label is ignored.
    let mut quoted = false;
    let mut start = 0;
    let bytes = content_type.as_bytes();
    for end in 0..=bytes.len() {
        if end < bytes.len() && bytes[end] == b'"' {
            quoted = !quoted;
        }
        if end != bytes.len() && (bytes[end] != b';' || quoted) {
            continue;
        }
        if start != 0 {
            let part = bytes[start..end].trim_ascii();
            if let Some(eq) = part.iter().position(|byte| *byte == b'=')
                && part[..eq].trim_ascii().eq_ignore_ascii_case(b"charset")
            {
                let value = part[eq + 1..].trim_ascii();
                let value = value
                    .strip_prefix(b"\"")
                    .and_then(|v| v.strip_suffix(b"\""))
                    .unwrap_or(value);
                return label(value);
            }
        }
        start = end + 1;
    }
    None
}

fn meta_content_encoding(value: &[u8]) -> Option<&'static Encoding> {
    let mut pos = 0;
    while pos + 7 <= value.len() {
        if value[pos..pos + 7].eq_ignore_ascii_case(b"charset") {
            pos += 7;
            while value.get(pos).is_some_and(|byte| whitespace(*byte)) {
                pos += 1;
            }
            if value.get(pos) != Some(&b'=') {
                continue;
            }
            pos += 1;
            while value.get(pos).is_some_and(|byte| whitespace(*byte)) {
                pos += 1;
            }
            let quote = match value.get(pos) {
                Some(b'\'' | b'"') => {
                    let quote = value[pos];
                    pos += 1;
                    Some(quote)
                }
                _ => None,
            };
            let start = pos;
            while value.get(pos).is_some_and(|byte| {
                if let Some(quote) = quote {
                    *byte != quote
                } else {
                    !whitespace(*byte) && *byte != b';'
                }
            }) {
                pos += 1;
            }
            return label(&value[start..pos]);
        }
        pos += 1;
    }
    None
}

fn attribute(bytes: &[u8], pos: &mut usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let start = loop {
        while bytes
            .get(*pos)
            .is_some_and(|byte| whitespace(*byte) || *byte == b'/')
        {
            *pos += 1;
        }
        if !bytes.get(*pos).is_some_and(|byte| *byte != b'>') {
            return None;
        }
        let start = *pos;
        while bytes
            .get(*pos)
            .is_some_and(|byte| !whitespace(*byte) && !matches!(*byte, b'/' | b'>' | b'='))
        {
            *pos += 1;
        }
        if start != *pos {
            break start;
        }
        *pos += 1;
    };
    let name = ascii_lower(&bytes[start..*pos]);
    while bytes.get(*pos).is_some_and(|byte| whitespace(*byte)) {
        *pos += 1;
    }
    if bytes.get(*pos) != Some(&b'=') {
        return Some((name, Vec::new()));
    }
    *pos += 1;
    while bytes.get(*pos).is_some_and(|byte| whitespace(*byte)) {
        *pos += 1;
    }
    let quote = match bytes.get(*pos) {
        Some(b'\'' | b'"') => {
            let quote = bytes[*pos];
            *pos += 1;
            Some(quote)
        }
        _ => None,
    };
    let start = *pos;
    while bytes.get(*pos).is_some_and(|byte| {
        if let Some(quote) = quote {
            *byte != quote
        } else {
            !whitespace(*byte) && *byte != b'>'
        }
    }) {
        *pos += 1;
    }
    let value = ascii_lower(&bytes[start..*pos]);
    if quote.is_some() && bytes.get(*pos) == quote.as_ref() {
        *pos += 1;
    }
    Some((name, value))
}

fn meta_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let bytes = &bytes[..bytes.len().min(PRESCAN_BYTES)];
    if bytes.starts_with(b"<\0?\0x\0") {
        return Some(UTF_16LE);
    }
    if bytes.starts_with(b"\0<\0?\0x") {
        return Some(UTF_16BE);
    }
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos..].starts_with(b"<!--") {
            if let Some(end) = bytes[pos + 4..].windows(3).position(|w| w == b"-->") {
                pos += end + 7;
                continue;
            }
            break;
        }
        if bytes[pos..].len() >= 6
            && bytes[pos] == b'<'
            && bytes[pos + 1..pos + 5].eq_ignore_ascii_case(b"meta")
            && (whitespace(bytes[pos + 5]) || bytes[pos + 5] == b'/')
        {
            let mut cursor = pos + 5;
            let mut seen = Vec::new();
            let mut pragma = false;
            let mut charset = None;
            let mut need_pragma = None;
            while let Some((name, value)) = attribute(bytes, &mut cursor) {
                if seen.contains(&name) {
                    continue;
                }
                seen.push(name.clone());
                match name.as_slice() {
                    b"http-equiv" if value == b"content-type" => pragma = true,
                    b"content" if charset.is_none() => {
                        if let Some(enc) = meta_content_encoding(&value) {
                            charset = Some(enc);
                            need_pragma = Some(true);
                        }
                    }
                    b"charset" => {
                        charset = label(&value);
                        need_pragma = Some(false);
                    }
                    _ => {}
                }
            }
            // A declaration truncated by the prescan boundary is not usable.
            if bytes.get(cursor) != Some(&b'>') {
                break;
            }
            if need_pragma.is_some_and(|needed| !needed || pragma)
                && let Some(enc) = charset
            {
                return Some(if enc == UTF_16LE || enc == UTF_16BE {
                    UTF_8
                } else if enc == X_USER_DEFINED {
                    WINDOWS_1252
                } else {
                    enc
                });
            }
            pos = cursor.saturating_add(1);
            continue;
        }
        if bytes[pos] == b'<' {
            let mut cursor = pos + 1;
            if bytes.get(cursor) == Some(&b'/') {
                cursor += 1;
            }
            if bytes.get(cursor).is_some_and(u8::is_ascii_alphabetic) {
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| !whitespace(*byte) && *byte != b'>')
                {
                    cursor += 1;
                }
                while attribute(bytes, &mut cursor).is_some() {}
                if bytes.get(cursor) != Some(&b'>') {
                    break;
                }
                pos = cursor + 1;
                continue;
            }
            if matches!(bytes.get(cursor), Some(b'!' | b'?' | b'/')) {
                if let Some(end) = bytes[pos..].iter().position(|byte| *byte == b'>') {
                    pos += end + 1;
                    continue;
                }
                break;
            }
        }
        pos += 1;
    }
    // The HTML prescan also recognizes an XML declaration at the byte-stream start.
    if bytes.starts_with(b"<?xml") {
        let end = bytes.iter().position(|byte| *byte == b'>')?;
        let declaration = &bytes[..end];
        for pos in 0..declaration.len().saturating_sub(8) {
            if declaration[pos..].starts_with(b"encoding") {
                let mut cursor = pos + 8;
                while declaration.get(cursor).is_some_and(|b| *b <= b' ') {
                    cursor += 1;
                }
                if declaration.get(cursor) != Some(&b'=') {
                    continue;
                }
                cursor += 1;
                while declaration.get(cursor).is_some_and(|b| *b <= b' ') {
                    cursor += 1;
                }
                let quote = *declaration.get(cursor)?;
                if !matches!(quote, b'\'' | b'"') {
                    continue;
                }
                cursor += 1;
                let start = cursor;
                while declaration.get(cursor).is_some_and(|b| *b != quote) {
                    cursor += 1;
                }
                if let Some(enc) = label(&declaration[start..cursor]) {
                    return Some(if enc == UTF_16LE || enc == UTF_16BE {
                        UTF_8
                    } else {
                        enc
                    });
                }
            }
        }
    }
    None
}

/// Decode HTML bytes using BOM, final response Content-Type, meta prescan, then UTF-8.
/// Pass `None` for local files. Invalid byte sequences become U+FFFD.
pub fn decode_html_bytes(
    bytes: &[u8],
    transport_content_type: Option<&str>,
) -> Result<DecodedHtml, Error> {
    if bytes.len() > MAX_HTML_BYTES {
        return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
    }
    let (encoding, source) = if let Some((encoding, _)) = Encoding::for_bom(bytes) {
        (encoding, EncodingSource::Bom)
    } else if let Some(encoding) = transport_content_type.and_then(transport_encoding) {
        (encoding, EncodingSource::Transport)
    } else if let Some(encoding) = meta_encoding(bytes) {
        (encoding, EncodingSource::Meta)
    } else {
        (UTF_8, EncodingSource::Default)
    };
    let (decoded, had_decoding_errors) = encoding.decode_with_bom_removal(bytes);
    Ok(DecodedHtml {
        text: decoded.into_owned(),
        encoding: encoding.name(),
        source,
        had_decoding_errors,
    })
}
