//! The `SecretStore` port as a foreign trait: the Mac app implements
//! [`SecretStorePort`] over the Keychain, and [`ForeignSecretStore`] adapts
//! it to [`app_state::SecretStore`] for the core.
//!
//! Key material crosses the bridge as plain bytes; on this side it goes
//! straight into [`SecretBytes`], which is wiped on drop. The bridge's own
//! buffers are freed but not wiped (see `app-state/SECURITY.md`).

use std::fmt;
use std::sync::Arc;

use app_state::{
    Availability, SecretBytes, SecretId as CoreSecretId, SecretItem as CoreSecretItem,
    SecretKind as CoreSecretKind, SecretStore, SecretStoreError,
};

/// What a Keychain item holds; mirrors [`app_state::SecretKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Enum)]
pub enum SecretKind {
    /// A workspace's symmetric data key, named by the workspace id.
    WorkspaceDataKey,
    /// The app-level document's key, named `app`.
    AppDataKey,
    /// This device's Ed25519 signing key seed, named by the device id.
    DeviceSigningKey,
    /// A device's published Ed25519 public key, named by its device id.
    DevicePublicKey,
    /// An AI provider's API key, named by the key's id; this device only.
    ProviderApiKey,
    /// The localhost API's bearer token, named `local-api`; this device only.
    LocalApiToken,
    /// A mail account's app password, named by its connection id; this device only.
    MailPassword,
    /// A mail account's OAuth refresh token, named by its connection id; this device only.
    MailRefreshToken,
}

/// Names one item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct SecretId {
    /// What the item holds.
    pub kind: SecretKind,
    /// The workspace id, device id or `app`.
    pub name: String,
}

/// One item and its key material.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct SecretItem {
    /// Which item this is.
    pub id: SecretId,
    /// Whether it travels to the user's other devices.
    pub is_synchronisable: bool,
    /// The key material.
    pub value: Vec<u8>,
}

/// The platform store failed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SecretStoreFailure {
    /// What the platform reported (an `OSStatus` and its message on Apple).
    #[error("{message}")]
    Platform {
        /// The platform's message; never key material.
        message: String,
    },
}

/// Where keys are kept, implemented by the platform. The contract is
/// [`app_state::SecretStore`]'s: `get` answers `None` for a missing item,
/// `add` never replaces one, `delete` of a missing item succeeds, `list`
/// answers the ids of one kind sorted by name.
#[uniffi::export(with_foreign)]
pub trait SecretStorePort: Send + Sync {
    /// Reads one item.
    ///
    /// # Errors
    /// The platform store failed.
    fn get(&self, id: SecretId) -> Result<Option<SecretItem>, SecretStoreFailure>;

    /// Stores an item, replacing one with the same id.
    ///
    /// # Errors
    /// The platform store failed.
    fn put(&self, item: SecretItem) -> Result<(), SecretStoreFailure>;

    /// Stores an item only when none with its id exists; returns whether it did.
    ///
    /// # Errors
    /// The platform store failed.
    fn add(&self, item: SecretItem) -> Result<bool, SecretStoreFailure>;

    /// Deletes an item everywhere it synchronises to.
    ///
    /// # Errors
    /// The platform store failed.
    fn delete(&self, id: SecretId) -> Result<(), SecretStoreFailure>;

    /// The ids of every item of one kind visible on this device.
    ///
    /// # Errors
    /// The platform store failed.
    fn list(&self, kind: SecretKind) -> Result<Vec<SecretId>, SecretStoreFailure>;
}

/// [`app_state::SecretStore`] over a platform [`SecretStorePort`].
#[derive(Clone)]
pub struct ForeignSecretStore(Arc<dyn SecretStorePort>);

/// Whether items of `kind` synchronise; the core decides, the platform obeys.
#[uniffi::export]
#[must_use]
pub fn secret_kind_is_synchronisable(kind: SecretKind) -> bool {
    CoreSecretKind::from(kind).availability() == Availability::Synchronisable
}

/// The stable label of `kind`, for platform service names.
#[uniffi::export]
#[must_use]
pub fn secret_kind_label(kind: SecretKind) -> String {
    CoreSecretKind::from(kind).label().to_owned()
}

impl ForeignSecretStore {
    /// Adapts `port`.
    #[must_use]
    pub fn new(port: Arc<dyn SecretStorePort>) -> ForeignSecretStore {
        ForeignSecretStore(port)
    }
}

impl fmt::Debug for ForeignSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ForeignSecretStore(..)")
    }
}

impl SecretStore for ForeignSecretStore {
    fn get(&self, id: &CoreSecretId) -> Result<Option<CoreSecretItem>, SecretStoreError> {
        let item = self.0.get(id.clone().into()).map_err(store_error)?;
        Ok(item.map(Into::into))
    }

    fn put(&self, item: CoreSecretItem) -> Result<(), SecretStoreError> {
        self.0.put(item.into()).map_err(store_error)
    }

    fn add(&self, item: CoreSecretItem) -> Result<bool, SecretStoreError> {
        self.0.add(item.into()).map_err(store_error)
    }

    fn delete(&self, id: &CoreSecretId) -> Result<(), SecretStoreError> {
        self.0.delete(id.clone().into()).map_err(store_error)
    }

    fn list(&self, kind: CoreSecretKind) -> Result<Vec<CoreSecretId>, SecretStoreError> {
        let mut ids: Vec<CoreSecretId> = self
            .0
            .list(kind.into())
            .map_err(store_error)?
            .into_iter()
            .map(Into::into)
            .filter(|id: &CoreSecretId| id.kind == kind)
            .collect();
        ids.sort();
        Ok(ids)
    }
}

impl fmt::Debug for SecretItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretItem")
            .field("id", &self.id)
            .field("is_synchronisable", &self.is_synchronisable)
            .field("value", &format_args!("{} bytes", self.value.len()))
            .finish()
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SecretStoreFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        SecretStoreFailure::Platform {
            message: error.reason,
        }
    }
}

impl From<SecretKind> for CoreSecretKind {
    fn from(kind: SecretKind) -> Self {
        match kind {
            SecretKind::WorkspaceDataKey => CoreSecretKind::WorkspaceDataKey,
            SecretKind::AppDataKey => CoreSecretKind::AppDataKey,
            SecretKind::DeviceSigningKey => CoreSecretKind::DeviceSigningKey,
            SecretKind::DevicePublicKey => CoreSecretKind::DevicePublicKey,
            SecretKind::ProviderApiKey => CoreSecretKind::ProviderApiKey,
            SecretKind::LocalApiToken => CoreSecretKind::LocalApiToken,
            SecretKind::MailPassword => CoreSecretKind::MailPassword,
            SecretKind::MailRefreshToken => CoreSecretKind::MailRefreshToken,
        }
    }
}

impl From<CoreSecretKind> for SecretKind {
    fn from(kind: CoreSecretKind) -> Self {
        match kind {
            CoreSecretKind::WorkspaceDataKey => SecretKind::WorkspaceDataKey,
            CoreSecretKind::AppDataKey => SecretKind::AppDataKey,
            CoreSecretKind::DeviceSigningKey => SecretKind::DeviceSigningKey,
            CoreSecretKind::DevicePublicKey => SecretKind::DevicePublicKey,
            CoreSecretKind::ProviderApiKey => SecretKind::ProviderApiKey,
            CoreSecretKind::LocalApiToken => SecretKind::LocalApiToken,
            CoreSecretKind::MailPassword => SecretKind::MailPassword,
            CoreSecretKind::MailRefreshToken => SecretKind::MailRefreshToken,
        }
    }
}

impl From<CoreSecretId> for SecretId {
    fn from(id: CoreSecretId) -> Self {
        SecretId {
            kind: id.kind.into(),
            name: id.name,
        }
    }
}

impl From<SecretId> for CoreSecretId {
    fn from(id: SecretId) -> Self {
        CoreSecretId::new(id.kind.into(), id.name)
    }
}

impl From<CoreSecretItem> for SecretItem {
    fn from(item: CoreSecretItem) -> Self {
        SecretItem {
            is_synchronisable: item.availability == Availability::Synchronisable,
            value: item.value.expose().to_vec(),
            id: item.id.into(),
        }
    }
}

// The availability always follows the kind: a platform that reports another
// one cannot move an item between partitions.
impl From<SecretItem> for CoreSecretItem {
    fn from(item: SecretItem) -> Self {
        let id = CoreSecretId::from(item.id);
        CoreSecretItem::new(id.kind, id.name, SecretBytes::new(item.value))
    }
}

fn store_error(failure: SecretStoreFailure) -> SecretStoreError {
    SecretStoreError {
        message: failure.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use app_state::{
        AppStateStore, DeviceIdentity, PlainSyncFiles, TrustedDevices, create_workspace_key,
        workspace_key,
    };
    use domain::workspace::WorkspaceId;

    use super::*;

    /// A port that keeps items in a map, as a platform store would.
    #[derive(Default)]
    struct MapPort(Mutex<BTreeMap<(SecretKind, String), SecretItem>>);

    impl MapPort {
        fn items(&self) -> std::sync::MutexGuard<'_, BTreeMap<(SecretKind, String), SecretItem>> {
            self.0.lock().unwrap()
        }
    }

    impl SecretStorePort for MapPort {
        fn get(&self, id: SecretId) -> Result<Option<SecretItem>, SecretStoreFailure> {
            Ok(self.items().get(&(id.kind, id.name)).cloned())
        }
        fn put(&self, item: SecretItem) -> Result<(), SecretStoreFailure> {
            self.items()
                .insert((item.id.kind, item.id.name.clone()), item);
            Ok(())
        }
        fn add(&self, item: SecretItem) -> Result<bool, SecretStoreFailure> {
            let key = (item.id.kind, item.id.name.clone());
            let mut items = self.items();
            if items.contains_key(&key) {
                return Ok(false);
            }
            items.insert(key, item);
            Ok(true)
        }
        fn delete(&self, id: SecretId) -> Result<(), SecretStoreFailure> {
            self.items().remove(&(id.kind, id.name));
            Ok(())
        }
        fn list(&self, kind: SecretKind) -> Result<Vec<SecretId>, SecretStoreFailure> {
            // Unsorted and with a foreign kind mixed in: the adapter fixes both.
            let mut ids: Vec<SecretId> =
                self.items().values().map(|item| item.id.clone()).collect();
            ids.reverse();
            Ok(ids
                .into_iter()
                .filter(|id| id.kind == kind || id.name == "stray")
                .collect())
        }
    }

    struct FailingPort;

    impl SecretStorePort for FailingPort {
        fn get(&self, _: SecretId) -> Result<Option<SecretItem>, SecretStoreFailure> {
            Err(locked())
        }
        fn put(&self, _: SecretItem) -> Result<(), SecretStoreFailure> {
            Err(locked())
        }
        fn add(&self, _: SecretItem) -> Result<bool, SecretStoreFailure> {
            Err(locked())
        }
        fn delete(&self, _: SecretId) -> Result<(), SecretStoreFailure> {
            Err(locked())
        }
        fn list(&self, _: SecretKind) -> Result<Vec<SecretId>, SecretStoreFailure> {
            Err(locked())
        }
    }

    fn locked() -> SecretStoreFailure {
        SecretStoreFailure::Platform {
            message: "errSecInteractionNotAllowed (-25308)".to_owned(),
        }
    }

    #[test]
    fn the_key_hierarchy_runs_over_a_foreign_store() {
        let port = Arc::new(MapPort::default());
        let store = ForeignSecretStore::new(port.clone());
        let workspace = WorkspaceId::new("mock-studio");
        let key = create_workspace_key(&store, &workspace).unwrap();
        assert_eq!(workspace_key(&store, &workspace).unwrap(), Some(key));

        let identity = DeviceIdentity::load_or_create(&store).unwrap();
        let again = DeviceIdentity::load_or_create(&store).unwrap();
        assert_eq!(identity.device_id(), again.device_id());
        // Opening the app state mints the app key and publishes the public key.
        let app_folder = tempfile::tempdir().unwrap();
        AppStateStore::open(Arc::new(PlainSyncFiles), app_folder.path(), &store, again).unwrap();
        let trusted = TrustedDevices::load(&store).unwrap();
        assert_eq!(
            trusted.key_of(identity.device_id()),
            Some(&identity.public_key())
        );

        let items = port.items();
        let signing = items
            .values()
            .find(|item| item.id.kind == SecretKind::DeviceSigningKey)
            .unwrap();
        assert!(
            !signing.is_synchronisable,
            "the signing key stays on the device"
        );
        assert!(
            items
                .values()
                .filter(|item| item.id.kind != SecretKind::DeviceSigningKey)
                .all(|item| item.is_synchronisable)
        );
    }

    #[test]
    fn list_is_sorted_and_keeps_to_its_kind() {
        let port = Arc::new(MapPort::default());
        for name in ["b", "a", "stray"] {
            let kind = if name == "stray" {
                SecretKind::AppDataKey
            } else {
                SecretKind::DevicePublicKey
            };
            port.put(SecretItem {
                id: SecretId {
                    kind,
                    name: name.to_owned(),
                },
                is_synchronisable: true,
                value: vec![0; 32],
            })
            .unwrap();
        }
        let store = ForeignSecretStore::new(port);
        let names: Vec<String> = store
            .list(CoreSecretKind::DevicePublicKey)
            .unwrap()
            .into_iter()
            .map(|id| id.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn a_platform_failure_reaches_the_core_as_a_store_error() {
        let store = ForeignSecretStore::new(Arc::new(FailingPort));
        let error = workspace_key(&store, &WorkspaceId::new("w")).unwrap_err();
        assert!(error.to_string().contains("-25308"), "{error}");
    }

    #[test]
    fn availability_follows_the_kind_and_items_never_print_their_value() {
        assert!(secret_kind_is_synchronisable(SecretKind::WorkspaceDataKey));
        assert!(!secret_kind_is_synchronisable(SecretKind::DeviceSigningKey));
        assert!(!secret_kind_is_synchronisable(SecretKind::ProviderApiKey));
        assert!(!secret_kind_is_synchronisable(SecretKind::LocalApiToken));
        assert_eq!(
            secret_kind_label(SecretKind::LocalApiToken),
            "local-api-token"
        );
        assert_eq!(
            secret_kind_label(SecretKind::ProviderApiKey),
            "provider-api-key"
        );
        assert_eq!(
            secret_kind_label(SecretKind::DevicePublicKey),
            "device-public-key"
        );

        let item = SecretItem {
            id: SecretId {
                kind: SecretKind::DeviceSigningKey,
                name: "dev".to_owned(),
            },
            is_synchronisable: true,
            value: vec![0xAB; 4],
        };
        let core = CoreSecretItem::from(item.clone());
        assert_eq!(core.availability, Availability::DeviceOnly);
        let printed = format!("{item:?}");
        assert!(
            printed.contains("4 bytes") && !printed.contains("171"),
            "{printed}"
        );
    }
}
