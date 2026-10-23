//! The encrypted file format shared by change files, workspace snapshots and
//! app snapshots: a plaintext header, authenticated as associated data, then
//! the ChaCha20-Poly1305 ciphertext under a data key with a fresh random
//! nonce per file.
//!
//! ```text
//! magic "VDAY" | format u8 | kind u8 | schema u16 BE | device id length u8 |
//! device id | sequence u64 BE | nonce (12) | ciphertext and tag (16)
//! ```
//!
//! Everything before the ciphertext, nonce included, is the associated data,
//! so a reader trusts the device id and sequence only after the tag checks.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::Zeroizing;

use crate::device::{DEVICE_ID_MAX_LEN, DeviceId};
use crate::error::AppStateError;
use crate::keys::DataKey;

const MAGIC: &[u8; 4] = b"VDAY";
/// Version of this byte layout.
const FORMAT_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// What a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvelopeKind {
    /// One burst of a device's changes to a workspace document.
    ChangeBatch,
    /// A device's compaction of a workspace document.
    WorkspaceSnapshot,
    /// A device's copy of the app-level document.
    AppSnapshot,
}

/// The authenticated header of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) kind: EnvelopeKind,
    /// Version of the document schema the plaintext was written with.
    pub(crate) schema: u16,
    /// The device that wrote the file.
    pub(crate) device: DeviceId,
    /// The batch number, or for a snapshot the last batch number it covers.
    pub(crate) sequence: u64,
}

/// Why a file could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenError {
    /// The bytes do not start with a whole, known header.
    Malformed,
    /// A format version this build does not read.
    NewerFormat,
    /// The tag does not verify: wrong key, truncated, or altered.
    Undecryptable,
}

impl EnvelopeKind {
    const fn code(self) -> u8 {
        match self {
            EnvelopeKind::ChangeBatch => 1,
            EnvelopeKind::WorkspaceSnapshot => 2,
            EnvelopeKind::AppSnapshot => 3,
        }
    }

    const fn from_code(code: u8) -> Option<EnvelopeKind> {
        match code {
            1 => Some(EnvelopeKind::ChangeBatch),
            2 => Some(EnvelopeKind::WorkspaceSnapshot),
            3 => Some(EnvelopeKind::AppSnapshot),
            _ => None,
        }
    }
}

/// Encrypts `plaintext` under `key` with a fresh nonce, behind `header`.
pub(crate) fn seal(
    key: &DataKey,
    header: &Header,
    plaintext: &[u8],
) -> Result<Vec<u8>, AppStateError> {
    let nonce: [u8; NONCE_LEN] = crate::random::bytes()?;
    let mut bytes = encode_header(header);
    bytes.extend_from_slice(&nonce);
    let ciphertext = cipher(key)
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &bytes,
            },
        )
        // ChaCha20-Poly1305 fails only for messages beyond 256 GiB.
        .map_err(|_| AppStateError::TooLargeToEncrypt(plaintext.len()))?;
    bytes.extend_from_slice(&ciphertext);
    Ok(bytes)
}

/// Reads the header without authenticating it, to route a file before it is opened.
pub(crate) fn peek_header(bytes: &[u8]) -> Result<Header, OpenError> {
    decode_header(bytes).map(|(header, _)| header)
}

/// Authenticates and decrypts a file.
pub(crate) fn open(key: &DataKey, bytes: &[u8]) -> Result<(Header, Zeroizing<Vec<u8>>), OpenError> {
    let (header, header_len) = decode_header(bytes)?;
    let nonce_end = header_len + NONCE_LEN;
    if bytes.len() < nonce_end + TAG_LEN {
        return Err(OpenError::Undecryptable);
    }
    let (associated, ciphertext) = bytes.split_at(nonce_end);
    let plaintext = cipher(key)
        .decrypt(
            Nonce::from_slice(&associated[header_len..]),
            Payload {
                msg: ciphertext,
                aad: associated,
            },
        )
        .map_err(|_| OpenError::Undecryptable)?;
    Ok((header, Zeroizing::new(plaintext)))
}

fn cipher(key: &DataKey) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(key.as_bytes().into())
}

fn encode_header(header: &Header) -> Vec<u8> {
    let device = header.device.as_str().as_bytes();
    let mut bytes = Vec::with_capacity(MAGIC.len() + 13 + device.len() + NONCE_LEN);
    bytes.extend_from_slice(MAGIC);
    bytes.push(FORMAT_VERSION);
    bytes.push(header.kind.code());
    bytes.extend_from_slice(&header.schema.to_be_bytes());
    // `DeviceId` caps the length at DEVICE_ID_MAX_LEN, which fits a byte.
    bytes.push(u8::try_from(device.len()).unwrap_or(u8::MAX));
    bytes.extend_from_slice(device);
    bytes.extend_from_slice(&header.sequence.to_be_bytes());
    bytes
}

fn decode_header(bytes: &[u8]) -> Result<(Header, usize), OpenError> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(OpenError::Malformed);
    }
    if reader.byte()? != FORMAT_VERSION {
        return Err(OpenError::NewerFormat);
    }
    let kind = EnvelopeKind::from_code(reader.byte()?).ok_or(OpenError::Malformed)?;
    let schema = u16::from_be_bytes(reader.array()?);
    let device_len = usize::from(reader.byte()?);
    if device_len > DEVICE_ID_MAX_LEN {
        return Err(OpenError::Malformed);
    }
    let device = std::str::from_utf8(reader.take(device_len)?)
        .ok()
        .and_then(|id| DeviceId::new(id).ok())
        .ok_or(OpenError::Malformed)?;
    let sequence = u64::from_be_bytes(reader.array()?);
    let header = Header {
        kind,
        schema,
        device,
        sequence,
    };
    Ok((header, reader.position))
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], OpenError> {
        let end = self.position.checked_add(len).ok_or(OpenError::Malformed)?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or(OpenError::Malformed)?;
        self.position = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, OpenError> {
        Ok(self.array::<1>()?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], OpenError> {
        self.take(N)?.try_into().map_err(|_| OpenError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            kind: EnvelopeKind::ChangeBatch,
            schema: 1,
            device: DeviceId::new("mac-a").unwrap(),
            sequence: 7,
        }
    }

    #[test]
    fn a_sealed_file_opens_with_its_key_only() {
        let key = DataKey::generate().unwrap();
        let sealed = seal(&key, &header(), b"synthetic plan").unwrap();
        let (opened, plaintext) = open(&key, &sealed).unwrap();
        assert_eq!(opened, header());
        assert_eq!(plaintext.as_slice(), b"synthetic plan");
        assert_eq!(
            open(&DataKey::generate().unwrap(), &sealed).unwrap_err(),
            OpenError::Undecryptable
        );
        assert!(
            !sealed.windows(9).any(|window| window == b"synthetic"),
            "the plaintext never appears in the file"
        );
    }

    #[test]
    fn two_seals_of_the_same_bytes_use_different_nonces() {
        let key = DataKey::generate().unwrap();
        assert_ne!(
            seal(&key, &header(), b"same").unwrap(),
            seal(&key, &header(), b"same").unwrap()
        );
    }

    #[test]
    fn every_altered_or_truncated_byte_is_refused() {
        let key = DataKey::generate().unwrap();
        let sealed = seal(&key, &header(), b"plan").unwrap();
        for index in 0..sealed.len() {
            let mut altered = sealed.clone();
            altered[index] ^= 0x01;
            assert!(open(&key, &altered).is_err(), "byte {index}");
            assert!(open(&key, &sealed[..index]).is_err(), "cut at {index}");
        }
    }

    #[test]
    fn a_newer_format_is_told_apart_from_garbage() {
        let key = DataKey::generate().unwrap();
        let mut sealed = seal(&key, &header(), b"plan").unwrap();
        sealed[4] = FORMAT_VERSION + 1;
        assert_eq!(open(&key, &sealed).unwrap_err(), OpenError::NewerFormat);
        assert_eq!(open(&key, b"").unwrap_err(), OpenError::Malformed);
        assert_eq!(
            peek_header(&seal(&key, &header(), b"").unwrap()).unwrap(),
            header()
        );
    }
}
