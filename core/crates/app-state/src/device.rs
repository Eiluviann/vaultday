//! Device ids: random per install, the name of the device's sync folder and
//! the prefix of the Automerge actors its changes are written as.

use std::fmt;

use automerge::ActorId;

use crate::error::AppStateError;

/// Longest accepted device id, in bytes; the envelope stores the length in one byte.
pub const DEVICE_ID_MAX_LEN: usize = 64;
/// Random bytes behind a generated id; 128 bits keep two installs apart.
const GENERATED_ID_BYTES: usize = 16;

/// Which device wrote a change: the name of its sync folder and the prefix of
/// the Automerge actors it writes as.
///
/// Generated ids are random ([`DeviceId::generate`]), never derived from the
/// host name, and regenerated after a restore (see
/// [`crate::keys::DeviceIdentity::regenerate`]) so two installs never write
/// the same sequence numbers under one id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    /// Wraps a device id after checking it is a plain folder name.
    ///
    /// # Errors
    /// [`AppStateError::InvalidDeviceId`] for an empty id, one longer than
    /// [`DEVICE_ID_MAX_LEN`] or one with other characters than letters,
    /// digits, `-` and `_`.
    pub fn new(id: impl Into<String>) -> Result<DeviceId, AppStateError> {
        let id = id.into();
        let is_plain = !id.is_empty()
            && id.len() <= DEVICE_ID_MAX_LEN
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !is_plain {
            return Err(AppStateError::InvalidDeviceId(id));
        }
        Ok(DeviceId(id))
    }

    /// A fresh random id: 32 lowercase hex digits.
    ///
    /// # Errors
    /// [`AppStateError::Randomness`] when the system has no randomness.
    pub fn generate() -> Result<DeviceId, AppStateError> {
        let bytes: [u8; GENERATED_ID_BYTES] = crate::random::bytes()?;
        Ok(DeviceId(hex(&bytes)))
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A fresh Automerge actor for one run of this device: the id, a `.`
    /// and random hex. A new actor per run means a run that could not read
    /// some of its own earlier changes (evicted by the sync provider, or
    /// restored from a backup) never reuses their sequence numbers.
    pub(crate) fn run_actor(&self) -> Result<ActorId, AppStateError> {
        let run: [u8; 8] = crate::random::bytes()?;
        Ok(ActorId::from(
            format!("{}.{}", self.0, hex(&run)).as_bytes(),
        ))
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_ids_are_plain_folder_names() {
        assert!(DeviceId::new("mac-a_1").is_ok());
        for refused in ["", "../x", "a b", &"x".repeat(DEVICE_ID_MAX_LEN + 1)] {
            assert!(
                matches!(
                    DeviceId::new(refused),
                    Err(AppStateError::InvalidDeviceId(_))
                ),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn generated_ids_are_random_valid_ids() {
        let first = DeviceId::generate().unwrap();
        let second = DeviceId::generate().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.as_str().len(), 32);
        assert!(DeviceId::new(first.as_str()).is_ok());
    }
}
