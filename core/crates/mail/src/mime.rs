//! Just enough MIME to show a message: RFC 2047 encoded words in subjects
//! and names, quoted-printable and base64 bodies, a few charsets, and a
//! plain-text excerpt from text or HTML. Latin-1, Windows-1252 and
//! ISO-8859-15 decode through small tables; charsets other than those, UTF-8
//! and ASCII decode lossily as UTF-8.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// How long an excerpt is, in characters.
pub const EXCERPT_CHARS: usize = 200;

/// Decodes RFC 2047 encoded words (`=?utf-8?B?…?=`, `=?utf-8?Q?…?=`) in a
/// header value; whitespace between two encoded words is dropped.
#[must_use]
pub fn decode_header(value: &[u8]) -> String {
    let text = String::from_utf8_lossy(value);
    let mut decoded = String::new();
    let mut rest = text.as_ref();
    let mut previous_was_word = false;
    while let Some(start) = rest.find("=?") {
        let (before, candidate) = rest.split_at(start);
        match encoded_word(candidate) {
            Some((word, consumed)) => {
                if !(previous_was_word && before.trim().is_empty()) {
                    decoded.push_str(before);
                }
                decoded.push_str(&word);
                rest = &candidate[consumed..];
                previous_was_word = true;
            }
            None => {
                decoded.push_str(before);
                decoded.push_str("=?");
                rest = &candidate[2..];
                previous_was_word = false;
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

/// Decodes a body part by its transfer encoding and charset.
#[must_use]
pub fn decode_body(raw: &[u8], transfer_encoding: &str, charset: &str) -> String {
    let bytes = match transfer_encoding.to_ascii_lowercase().as_str() {
        "base64" => decode_base64_lenient(raw),
        "quoted-printable" => decode_quoted_printable(raw),
        _ => raw.to_vec(),
    };
    decode_charset(&bytes, charset)
}

/// Plain text from HTML: tags, scripts and styles dropped, common entities decoded.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let tag = &rest[start..];
        let lower: String = tag.chars().take(8).collect::<String>().to_ascii_lowercase();
        let skip_to = if lower.starts_with("<script") {
            find_ignoring_case(tag, "</script>").map(|end| end + "</script>".len())
        } else if lower.starts_with("<style") {
            find_ignoring_case(tag, "</style>").map(|end| end + "</style>".len())
        } else {
            tag.find('>').map(|end| end + 1)
        };
        let Some(skip_to) = skip_to else {
            rest = "";
            break;
        };
        if lower.starts_with("<br") || lower.starts_with("<p") || lower.starts_with("</p") {
            text.push('\n');
        } else {
            text.push(' ');
        }
        rest = &tag[skip_to..];
    }
    text.push_str(rest);
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// The first [`EXCERPT_CHARS`] characters of a message's own words: quoted
/// lines (`>`) and a trailing signature (`-- `) are left out, and runs of
/// whitespace become one space.
#[must_use]
pub fn excerpt(text: &str) -> String {
    let own: Vec<&str> = text
        .lines()
        .take_while(|line| *line != "-- ")
        .filter(|line| !line.trim_start().starts_with('>'))
        .collect();
    let collapsed = own
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut cut: String = collapsed.chars().take(EXCERPT_CHARS).collect();
    if collapsed.chars().count() > EXCERPT_CHARS {
        cut.push('…');
    }
    cut
}

fn encoded_word(candidate: &str) -> Option<(String, usize)> {
    let inner = candidate.strip_prefix("=?")?;
    let (charset, inner) = inner.split_once('?')?;
    let (encoding, inner) = inner.split_once('?')?;
    let end = inner.find("?=")?;
    let payload = &inner[..end];
    if payload.contains(' ') {
        return None;
    }
    let bytes = match encoding.to_ascii_lowercase().as_str() {
        "b" => STANDARD.decode(payload.trim()).ok()?,
        "q" => decode_quoted_printable(payload.replace('_', " ").as_bytes()),
        _ => return None,
    };
    let consumed = 2 + charset.len() + 1 + encoding.len() + 1 + end + 2;
    Some((decode_charset(&bytes, charset), consumed))
}

fn decode_charset(bytes: &[u8], charset: &str) -> String {
    let charset = charset.to_ascii_lowercase();
    let charset = charset.split('*').next().unwrap_or_default();
    match charset {
        // Mail labelled Latin-1 is nearly always Windows-1252 in practice
        // (curly quotes, dashes, the euro sign in 0x80–0x9F), and the
        // WHATWG encoding standard maps the label to it for that reason.
        "iso-8859-1" | "latin1" | "windows-1252" | "cp1252" => {
            bytes.iter().map(|&byte| windows_1252_char(byte)).collect()
        }
        "iso-8859-15" | "latin9" => bytes.iter().map(|&byte| latin9_char(byte)).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// What Windows-1252 assigns to 0x80–0x9F; `None` for its five holes.
const WINDOWS_1252_HIGH: [Option<char>; 32] = [
    Some('\u{20AC}'),
    None,
    Some('\u{201A}'),
    Some('\u{0192}'),
    Some('\u{201E}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02C6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017D}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02DC}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203A}'),
    Some('\u{0153}'),
    None,
    Some('\u{017E}'),
    Some('\u{0178}'),
];

/// One Windows-1252 byte as a character; its holes keep the C1 control the
/// byte names in Latin-1, as the WHATWG decoder does.
fn windows_1252_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => WINDOWS_1252_HIGH
            .get(usize::from(byte - 0x80))
            .copied()
            .flatten()
            .unwrap_or(char::from(byte)),
        _ => char::from(byte),
    }
}

/// One ISO-8859-15 byte as a character: Latin-1 with eight places changed.
fn latin9_char(byte: u8) -> char {
    match byte {
        0xA4 => '\u{20AC}',
        0xA6 => '\u{0160}',
        0xA8 => '\u{0161}',
        0xB4 => '\u{017D}',
        0xB8 => '\u{017E}',
        0xBC => '\u{0152}',
        0xBD => '\u{0153}',
        0xBE => '\u{0178}',
        _ => char::from(byte),
    }
}

/// Base64 that may be cut mid-quantum by a partial fetch: whitespace is
/// ignored and a trailing partial quantum dropped.
fn decode_base64_lenient(raw: &[u8]) -> Vec<u8> {
    let clean: Vec<u8> = raw
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let usable = clean.len() - clean.len() % 4;
    STANDARD.decode(&clean[..usable]).unwrap_or_default()
}

fn decode_quoted_printable(raw: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        let byte = raw[index];
        if byte != b'=' {
            decoded.push(byte);
            index += 1;
            continue;
        }
        let next = raw.get(index + 1..index + 3);
        match next {
            Some([b'\r', b'\n']) => index += 3,
            Some([b'\n', _]) => index += 2,
            Some(&[high, low]) if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() => {
                let value = (hex_value(high) << 4) | hex_value(low);
                decoded.push(value);
                index += 3;
            }
            _ => {
                if raw.get(index + 1) == Some(&b'\n') {
                    index += 2;
                } else {
                    decoded.push(byte);
                    index += 1;
                }
            }
        }
    }
    decoded
}

const fn hex_value(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        b'A'..=b'F' => digit - b'A' + 10,
        _ => 0,
    }
}

fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_words_decode_and_join() {
        assert_eq!(
            decode_header(b"=?UTF-8?B?U3ludGhldGljIHN1YmplY3Q=?="),
            "Synthetic subject"
        );
        assert_eq!(
            decode_header(b"=?utf-8?Q?Caf=C3=A9_order?= =?utf-8?Q?_ready?="),
            "Café order ready"
        );
        assert_eq!(
            decode_header(b"Re: =?iso-8859-1?Q?R=E9union?= today"),
            "Re: Réunion today"
        );
        assert_eq!(decode_header(b"plain =? not a word"), "plain =? not a word");
    }

    #[test]
    fn bodies_decode_by_transfer_encoding() {
        assert_eq!(
            decode_body(
                b"Line one=\r\n continues =E2=9C=93",
                "quoted-printable",
                "utf-8"
            ),
            "Line one continues ✓"
        );
        assert_eq!(
            decode_body(b"U3ludGhldGlj\r\nIGJvZHk=", "BASE64", "utf-8"),
            "Synthetic body"
        );
        assert_eq!(
            decode_body(b"U3ludGhldGljIGJvZ", "base64", "utf-8"),
            "Synthetic bo",
            "a partial fetch cut mid-quantum keeps what decodes"
        );
    }

    #[test]
    fn windows_1252_and_latin9_decode_through_their_tables() {
        let bytes = [
            0x93, b'H', b'i', 0x94, b' ', 0x96, b' ', 0x80, b'5', b' ', 0xE9,
        ];
        assert_eq!(
            decode_charset(&bytes, "windows-1252"),
            "\u{201C}Hi\u{201D} \u{2013} \u{20AC}5 \u{e9}"
        );
        assert_eq!(
            decode_charset(&bytes, "ISO-8859-1"),
            decode_charset(&bytes, "cp1252")
        );
        assert_eq!(
            decode_charset(&[0x81, 0x8D], "windows-1252"),
            "\u{81}\u{8D}"
        );
        assert_eq!(
            decode_charset(&[0xA4, 0xBD, 0xE9], "iso-8859-15"),
            "\u{20AC}\u{153}\u{e9}"
        );
        assert_eq!(decode_charset("caf\u{e9}".as_bytes(), "utf-8"), "caf\u{e9}");
    }

    #[test]
    fn excerpts_skip_quotes_signatures_and_markup() {
        let text = "Hi,\n\nThe synthetic report is ready.\n> old quoted line\n-- \nSignature";
        assert_eq!(excerpt(text), "Hi, The synthetic report is ready.");
        let html =
            "<html><style>p{}</style><p>Hello&nbsp;<b>there</b></p><script>x()</script></html>";
        assert_eq!(excerpt(&html_to_text(html)), "Hello there");
        let long = "word ".repeat(100);
        assert!(excerpt(&long).ends_with('…'));
        assert_eq!(excerpt(&long).chars().count(), EXCERPT_CHARS + 1);
    }
}
