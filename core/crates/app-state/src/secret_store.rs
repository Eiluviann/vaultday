//! The `SecretStore` port: where key material lives. The Keychain
//! implementation (synchronisable items through iCloud Keychain, Secure
//! Enclave wrapping) lands in Swift with the Mac app; tests and CI use
//! [`InMemorySecretStore`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use thiserror::Error;
use zeroize::Zeroizing;

/// What an item holds. Each kind has one fixed [`Availability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKind {
    /// A workspace's symmetric data key, named by the workspace id.
    WorkspaceDataKey,
    /// The symmetric key of the app-level document, named `app`.
    AppDataKey,
    /// This device's Ed25519 signing key seed, named by the device id.
    DeviceSigningKey,
    /// A device's Ed25519 public key, named by its device id; how other
    /// devices trust that device's signed app state.
    DevicePublicKey,
    /// An AI provider's API key, named by the key's id. It stays on this
    /// device: never synchronised, never written to any file.
    ProviderApiKey,
    /// The bearer token of the app's localhost API, named `local-api`. It
    /// stays on this device; the app also writes it, mode 0600, into its App
    /// Group container for the command line.
    LocalApiToken,
}

/// Whether an item travels to the user's other devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Availability {
    /// Synchronised to the user's other devices (iCloud Keychain on Apple).
    Synchronisable,
    /// Kept on this device only, never synchronised and never restored onto another device.
    DeviceOnly,
}

/// Names one item: its kind plus a name within the kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretId {
    /// What the item holds.
    pub kind: SecretKind,
    /// The workspace id, device id or `app`.
    pub name: String,
}

/// Secret bytes, wiped from memory when dropped and never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretBytes(Zeroizing<Vec<u8>>);

/// One stored item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretItem {
    /// Which item this is.
    pub id: SecretId,
    /// Whether it synchronises.
    pub availability: Availability,
    /// The key material.
    pub value: SecretBytes,
}

/// The store failed; the message comes from the platform.
#[derive(Debug, Error)]
#[error("secret store: {message}")]
pub struct SecretStoreError {
    /// What the platform reported.
    pub message: String,
}

/// Where keys are kept. Implementations are shared between threads and keep
/// their own locking.
pub trait SecretStore: Send + Sync {
    /// Reads one item, `None` when it does not exist (or has not synchronised yet).
    ///
    /// # Errors
    /// The platform store failed.
    fn get(&self, id: &SecretId) -> Result<Option<SecretItem>, SecretStoreError>;

    /// Stores an item, replacing one with the same id.
    ///
    /// # Errors
    /// The platform store failed.
    fn put(&self, item: SecretItem) -> Result<(), SecretStoreError>;

    /// Stores an item only when none with its id exists (`SecItemAdd` on
    /// Apple). Returns whether it was stored. Keys are minted through this,
    /// so a key is never overwritten.
    ///
    /// # Errors
    /// The platform store failed.
    fn add(&self, item: SecretItem) -> Result<bool, SecretStoreError>;

    /// Deletes an item, and with it every synchronised copy; deleting one that
    /// does not exist is not an error.
    ///
    /// # Errors
    /// The platform store failed.
    fn delete(&self, id: &SecretId) -> Result<(), SecretStoreError>;

    /// The ids of every item of one kind visible on this device, sorted.
    ///
    /// # Errors
    /// The platform store failed.
    fn list(&self, kind: SecretKind) -> Result<Vec<SecretId>, SecretStoreError>;
}

/// A [`SecretStore`] in memory that models synchronisation: stores made with
/// [`InMemorySecretStore::for_another_device`] share every synchronisable
/// item, while each keeps its own device-only items.
#[derive(Debug, Clone, Default)]
pub struct InMemorySecretStore {
    synchronised: Arc<Mutex<Items>>,
    device_only: Arc<Mutex<Items>>,
}

type Items = BTreeMap<SecretId, SecretItem>;

impl SecretKind {
    /// Where items of this kind live.
    #[must_use]
    pub const fn availability(self) -> Availability {
        match self {
            SecretKind::WorkspaceDataKey | SecretKind::AppDataKey | SecretKind::DevicePublicKey => {
                Availability::Synchronisable
            }
            SecretKind::DeviceSigningKey
            | SecretKind::ProviderApiKey
            | SecretKind::LocalApiToken => Availability::DeviceOnly,
        }
    }

    /// A stable label for the kind, for platform service names and messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            SecretKind::WorkspaceDataKey => "workspace-data-key",
            SecretKind::AppDataKey => "app-data-key",
            SecretKind::DeviceSigningKey => "device-signing-key",
            SecretKind::DevicePublicKey => "device-public-key",
            SecretKind::ProviderApiKey => "provider-api-key",
            SecretKind::LocalApiToken => "local-api-token",
        }
    }
}

impl SecretId {
    /// An id of `kind` named `name`.
    pub fn new(kind: SecretKind, name: impl Into<String>) -> SecretId {
        SecretId {
            kind,
            name: name.into(),
        }
    }
}

impl fmt::Display for SecretId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.kind.label(), self.name)
    }
}

impl SecretBytes {
    /// Takes ownership of `bytes`; the buffer is wiped on drop.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> SecretBytes {
        SecretBytes(Zeroizing::new(bytes))
    }

    /// The bytes.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes({} bytes)", self.0.len())
    }
}

impl SecretItem {
    /// An item of `kind` named `name`, with the kind's availability.
    pub fn new(kind: SecretKind, name: impl Into<String>, value: SecretBytes) -> SecretItem {
        SecretItem {
            id: SecretId::new(kind, name),
            availability: kind.availability(),
            value,
        }
    }
}

impl InMemorySecretStore {
    /// An empty store for one device.
    #[must_use]
    pub fn new() -> InMemorySecretStore {
        InMemorySecretStore::default()
    }

    /// A store for a second device of the same user: it sees every
    /// synchronisable item of this one, now and later, and none of its
    /// device-only items.
    #[must_use]
    pub fn for_another_device(&self) -> InMemorySecretStore {
        InMemorySecretStore {
            synchronised: Arc::clone(&self.synchronised),
            device_only: Arc::default(),
        }
    }

    fn partition(&self, availability: Availability) -> &Mutex<Items> {
        match availability {
            Availability::Synchronisable => &self.synchronised,
            Availability::DeviceOnly => &self.device_only,
        }
    }
}

impl SecretStore for InMemorySecretStore {
    fn get(&self, id: &SecretId) -> Result<Option<SecretItem>, SecretStoreError> {
        for availability in [Availability::DeviceOnly, Availability::Synchronisable] {
            if let Some(item) = lock(self.partition(availability)).get(id) {
                return Ok(Some(item.clone()));
            }
        }
        Ok(None)
    }

    fn put(&self, item: SecretItem) -> Result<(), SecretStoreError> {
        let other = match item.availability {
            Availability::Synchronisable => Availability::DeviceOnly,
            Availability::DeviceOnly => Availability::Synchronisable,
        };
        lock(self.partition(other)).remove(&item.id);
        lock(self.partition(item.availability)).insert(item.id.clone(), item);
        Ok(())
    }

    fn add(&self, item: SecretItem) -> Result<bool, SecretStoreError> {
        // Both partitions are locked for the check and the insert, so two
        // threads adding the same id cannot both succeed.
        let mut synchronised = lock(&self.synchronised);
        let mut device_only = lock(&self.device_only);
        if synchronised.contains_key(&item.id) || device_only.contains_key(&item.id) {
            return Ok(false);
        }
        let partition = match item.availability {
            Availability::Synchronisable => &mut synchronised,
            Availability::DeviceOnly => &mut device_only,
        };
        partition.insert(item.id.clone(), item);
        Ok(true)
    }

    fn delete(&self, id: &SecretId) -> Result<(), SecretStoreError> {
        for availability in [Availability::DeviceOnly, Availability::Synchronisable] {
            lock(self.partition(availability)).remove(id);
        }
        Ok(())
    }

    fn list(&self, kind: SecretKind) -> Result<Vec<SecretId>, SecretStoreError> {
        let mut ids: Vec<SecretId> = [Availability::DeviceOnly, Availability::Synchronisable]
            .into_iter()
            .flat_map(|availability| {
                lock(self.partition(availability))
                    .keys()
                    .filter(|id| id.kind == kind)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        ids.sort();
        Ok(ids)
    }
}

// A panic while holding the lock leaves the map itself intact, so a poisoned
// lock is still safe to use.
fn lock(items: &Mutex<Items>) -> std::sync::MutexGuard<'_, Items> {
    items.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: SecretKind, name: &str, byte: u8) -> SecretItem {
        SecretItem::new(kind, name, SecretBytes::new(vec![byte; 4]))
    }

    #[test]
    fn synchronisable_items_reach_the_other_device_and_device_only_items_do_not() {
        let mac = InMemorySecretStore::new();
        let phone = mac.for_another_device();
        mac.put(item(SecretKind::WorkspaceDataKey, "personal", 1))
            .unwrap();
        mac.put(item(SecretKind::DeviceSigningKey, "mac", 2))
            .unwrap();

        let key = SecretId::new(SecretKind::WorkspaceDataKey, "personal");
        assert_eq!(phone.get(&key).unwrap().unwrap().value.expose(), [1; 4]);
        assert!(phone.list(SecretKind::DeviceSigningKey).unwrap().is_empty());
        assert_eq!(mac.list(SecretKind::DeviceSigningKey).unwrap().len(), 1);
        assert!(
            !phone
                .add(item(SecretKind::WorkspaceDataKey, "personal", 9))
                .unwrap()
        );
        assert_eq!(
            mac.get(&key).unwrap().unwrap().value.expose(),
            [1; 4],
            "add never overwrites"
        );

        phone.delete(&key).unwrap();
        assert!(
            mac.get(&key).unwrap().is_none(),
            "a delete reaches every device"
        );
        phone.delete(&key).unwrap();
    }

    #[test]
    fn secret_bytes_never_print_their_content() {
        let printed = format!("{:?}", item(SecretKind::AppDataKey, "app", 0xAB));
        assert!(printed.contains("SecretBytes(4 bytes)"), "{printed}");
        assert!(!printed.contains("171"), "{printed}");
    }
}
