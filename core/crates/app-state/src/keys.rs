//! The key hierarchy: one symmetric data key per workspace, one for the
//! app-level document, and one Ed25519 signing key per device whose public
//! half is published for the user's other devices. Every key lives in the
//! [`SecretStore`]; this module mints, loads and destroys them.

use std::collections::BTreeMap;
use std::fmt;

use domain::workspace::WorkspaceId;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use zeroize::Zeroizing;

use crate::device::DeviceId;
use crate::error::AppStateError;
use crate::secret_store::{SecretBytes, SecretId, SecretItem, SecretKind, SecretStore};

/// Length of a data key and of a signing key seed, in bytes.
pub const KEY_LEN: usize = 32;
/// The name of the app-level document's key.
const APP_KEY_NAME: &str = "app";

/// A 256-bit symmetric key for ChaCha20-Poly1305; wiped on drop, never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct DataKey(Zeroizing<[u8; KEY_LEN]>);

/// This device's identity: its id and its signing key.
pub struct DeviceIdentity {
    device_id: DeviceId,
    signing_key: SigningKey,
}

/// The public keys of every device the user's secret store vouches for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedDevices(BTreeMap<DeviceId, VerifyingKey>);

impl DataKey {
    /// A fresh random key.
    ///
    /// # Errors
    /// [`AppStateError::Randomness`] when the system has no randomness.
    pub fn generate() -> Result<DataKey, AppStateError> {
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        crate::random::fill(key.as_mut())?;
        Ok(DataKey(key))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    fn from_item(item: &SecretItem) -> Result<DataKey, AppStateError> {
        let key = zeroizing_key(item.value.expose())
            .ok_or_else(|| AppStateError::InvalidKeyMaterial(item.id.clone()))?;
        Ok(DataKey(key))
    }

    fn to_secret(&self) -> SecretBytes {
        SecretBytes::new(self.0.to_vec())
    }
}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(..)")
    }
}

/// Mints the data key of a new workspace and stores it synchronisable, so
/// the user's other devices can read the workspace's app state.
///
/// # Errors
/// [`AppStateError::KeyExists`] when the workspace already has a key (a
/// workspace is created once); otherwise store or randomness failures.
pub fn create_workspace_key(
    secrets: &dyn SecretStore,
    workspace: &WorkspaceId,
) -> Result<DataKey, AppStateError> {
    create_data_key(secrets, workspace_key_id(workspace))
}

/// The workspace's data key, `None` while it has not reached this device.
///
/// # Errors
/// Store failures, or [`AppStateError::InvalidKeyMaterial`] for a malformed item.
pub fn workspace_key(
    secrets: &dyn SecretStore,
    workspace: &WorkspaceId,
) -> Result<Option<DataKey>, AppStateError> {
    load_data_key(secrets, &workspace_key_id(workspace))
}

/// Destroys the workspace's data key on every device. The workspace's change
/// files stay where they are and can no longer be read by anyone.
///
/// # Errors
/// Store failures.
pub fn delete_workspace(
    secrets: &dyn SecretStore,
    workspace: &WorkspaceId,
) -> Result<(), AppStateError> {
    secrets.delete(&workspace_key_id(workspace))?;
    Ok(())
}

/// The app-level document's key, `None` while it has not reached this device.
///
/// # Errors
/// Store failures, or a malformed stored key.
pub fn app_key(secrets: &dyn SecretStore) -> Result<Option<DataKey>, AppStateError> {
    load_data_key(secrets, &app_key_id())
}

/// Mints the app-level document's key. Only the first device does this;
/// [`crate::signed::AppStateStore::open`] decides when.
///
/// # Errors
/// [`AppStateError::KeyExists`] when a key exists; store or randomness failures.
pub fn create_app_key(secrets: &dyn SecretStore) -> Result<DataKey, AppStateError> {
    create_data_key(secrets, app_key_id())
}

impl DeviceIdentity {
    /// This device's identity: the one stored device-only, or a new one with
    /// a random id when there is none (first launch, or a restore onto a new
    /// device, which never carries device-only items). The public key is
    /// published by [`crate::AppStateStore::open`] once this device holds the
    /// app key, never here: another device reads a published key as "the app
    /// key is on its way", so a device that never held it must not publish.
    ///
    /// # Errors
    /// Store or randomness failures, a malformed stored key, or
    /// [`AppStateError::SeveralDeviceKeys`].
    pub fn load_or_create(secrets: &dyn SecretStore) -> Result<DeviceIdentity, AppStateError> {
        let stored = secrets.list(SecretKind::DeviceSigningKey)?;
        let identity = match stored.as_slice() {
            [] => DeviceIdentity::mint(secrets)?,
            [id] => DeviceIdentity::load(secrets, id)?,
            several => return Err(AppStateError::SeveralDeviceKeys(several.len())),
        };
        Ok(identity)
    }

    /// The regenerate-after-restore hook: replaces this device's identity
    /// with a new random id and signing key. The platform calls it when it
    /// sees a restore whose device-only items came along (the same Mac
    /// restored from its own backup), so the restored install never reuses
    /// sequence numbers of the id it had before. The old public key stays
    /// published: snapshots the old id signed stay trusted.
    ///
    /// # Errors
    /// Store or randomness failures.
    pub fn regenerate(secrets: &dyn SecretStore) -> Result<DeviceIdentity, AppStateError> {
        for id in secrets.list(SecretKind::DeviceSigningKey)? {
            secrets.delete(&id)?;
        }
        let identity = DeviceIdentity::mint(secrets)?;
        identity.publish(secrets)?;
        Ok(identity)
    }

    /// This device's id.
    #[must_use]
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// The public half of the signing key.
    #[must_use]
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub(crate) fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    fn mint(secrets: &dyn SecretStore) -> Result<DeviceIdentity, AppStateError> {
        let device_id = DeviceId::generate()?;
        let mut seed = Zeroizing::new([0u8; KEY_LEN]);
        crate::random::fill(seed.as_mut())?;
        let signing_key = SigningKey::from_bytes(&seed);
        secrets.add(SecretItem::new(
            SecretKind::DeviceSigningKey,
            device_id.as_str(),
            SecretBytes::new(seed.to_vec()),
        ))?;
        Ok(DeviceIdentity {
            device_id,
            signing_key,
        })
    }

    fn load(secrets: &dyn SecretStore, id: &SecretId) -> Result<DeviceIdentity, AppStateError> {
        let invalid = || AppStateError::InvalidKeyMaterial(id.clone());
        let device_id = DeviceId::new(id.name.clone()).map_err(|_| invalid())?;
        let item = secrets.get(id)?.ok_or_else(invalid)?;
        let seed = zeroizing_key(item.value.expose()).ok_or_else(invalid)?;
        Ok(DeviceIdentity {
            device_id,
            signing_key: SigningKey::from_bytes(&seed),
        })
    }

    pub(crate) fn publish(&self, secrets: &dyn SecretStore) -> Result<(), AppStateError> {
        secrets.put(SecretItem::new(
            SecretKind::DevicePublicKey,
            self.device_id.as_str(),
            SecretBytes::new(self.public_key().to_bytes().to_vec()),
        ))?;
        Ok(())
    }
}

impl fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceIdentity")
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

impl TrustedDevices {
    /// Every published device public key. Items that are not a valid id or
    /// key are left out: they cannot vouch for anything.
    ///
    /// # Errors
    /// Store failures.
    pub fn load(secrets: &dyn SecretStore) -> Result<TrustedDevices, AppStateError> {
        let mut keys = BTreeMap::new();
        for id in secrets.list(SecretKind::DevicePublicKey)? {
            let Ok(device) = DeviceId::new(id.name.clone()) else {
                continue;
            };
            let Some(item) = secrets.get(&id)? else {
                continue;
            };
            let Ok(bytes) = <[u8; KEY_LEN]>::try_from(item.value.expose()) else {
                continue;
            };
            if let Ok(key) = VerifyingKey::from_bytes(&bytes) {
                keys.insert(device, key);
            }
        }
        Ok(TrustedDevices(keys))
    }

    /// The published key of `device`.
    #[must_use]
    pub fn key_of(&self, device: &DeviceId) -> Option<&VerifyingKey> {
        self.0.get(device)
    }

    /// The trusted device ids.
    pub fn devices(&self) -> impl Iterator<Item = &DeviceId> {
        self.0.keys()
    }
}

fn workspace_key_id(workspace: &WorkspaceId) -> SecretId {
    SecretId::new(SecretKind::WorkspaceDataKey, workspace.as_str())
}

pub(crate) fn app_key_id() -> SecretId {
    SecretId::new(SecretKind::AppDataKey, APP_KEY_NAME)
}

/// Mints a key through [`SecretStore::add`], so an existing key is never replaced.
fn create_data_key(secrets: &dyn SecretStore, id: SecretId) -> Result<DataKey, AppStateError> {
    let key = DataKey::generate()?;
    if !secrets.add(SecretItem::new(id.kind, id.name.clone(), key.to_secret()))? {
        return Err(AppStateError::KeyExists(id));
    }
    Ok(key)
}

/// Copies 32 bytes straight into a wiped-on-drop buffer; `None` for another length.
fn zeroizing_key(bytes: &[u8]) -> Option<Zeroizing<[u8; KEY_LEN]>> {
    if bytes.len() != KEY_LEN {
        return None;
    }
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    key.copy_from_slice(bytes);
    Some(key)
}

fn load_data_key(
    secrets: &dyn SecretStore,
    id: &SecretId,
) -> Result<Option<DataKey>, AppStateError> {
    secrets
        .get(id)?
        .map(|item| DataKey::from_item(&item))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_store::{Availability, InMemorySecretStore};

    #[test]
    fn a_workspace_key_is_minted_once_shared_and_destroyed_with_the_workspace() {
        let mac = InMemorySecretStore::new();
        let phone = mac.for_another_device();
        let workspace = WorkspaceId::new("personal");
        let key = create_workspace_key(&mac, &workspace).unwrap();
        assert!(matches!(
            create_workspace_key(&mac, &workspace),
            Err(AppStateError::KeyExists(_))
        ));
        assert_eq!(workspace_key(&phone, &workspace).unwrap(), Some(key));
        let stored = mac.get(&workspace_key_id(&workspace)).unwrap().unwrap();
        assert_eq!(stored.availability, Availability::Synchronisable);

        delete_workspace(&phone, &workspace).unwrap();
        assert_eq!(workspace_key(&mac, &workspace).unwrap(), None);
    }

    #[test]
    fn the_device_key_stays_on_the_device_and_its_public_half_is_published() {
        let mac = InMemorySecretStore::new();
        let phone = mac.for_another_device();
        let mac_identity = DeviceIdentity::load_or_create(&mac).unwrap();
        let again = DeviceIdentity::load_or_create(&mac).unwrap();
        assert_eq!(again.device_id(), mac_identity.device_id());
        assert_eq!(again.public_key(), mac_identity.public_key());
        assert!(
            TrustedDevices::load(&phone)
                .unwrap()
                .devices()
                .next()
                .is_none(),
            "published only once the device holds the app key"
        );
        mac_identity.publish(&mac).unwrap();

        let phone_identity = DeviceIdentity::load_or_create(&phone).unwrap();
        phone_identity.publish(&phone).unwrap();
        assert_ne!(phone_identity.device_id(), mac_identity.device_id());
        assert!(phone.list(SecretKind::DeviceSigningKey).unwrap().len() == 1);

        let trusted = TrustedDevices::load(&phone).unwrap();
        assert_eq!(
            trusted.key_of(mac_identity.device_id()),
            Some(&mac_identity.public_key())
        );
        assert_eq!(trusted.devices().count(), 2);
    }

    #[test]
    fn regenerating_after_a_restore_gives_a_new_id_and_keeps_the_old_key_trusted() {
        let mac = InMemorySecretStore::new();
        let before = DeviceIdentity::load_or_create(&mac).unwrap();
        // It held the app key before the restore, so it had published.
        before.publish(&mac).unwrap();
        let after = DeviceIdentity::regenerate(&mac).unwrap();
        assert_ne!(after.device_id(), before.device_id());
        assert_eq!(mac.list(SecretKind::DeviceSigningKey).unwrap().len(), 1);
        let trusted = TrustedDevices::load(&mac).unwrap();
        assert!(trusted.key_of(before.device_id()).is_some());
        assert!(trusted.key_of(after.device_id()).is_some());
    }

    #[test]
    fn keys_never_print() {
        let key = DataKey::generate().unwrap();
        assert_eq!(format!("{key:?}"), "DataKey(..)");
        let identity = DeviceIdentity::load_or_create(&InMemorySecretStore::new()).unwrap();
        assert!(!format!("{identity:?}").contains("signing_key"));
    }

    #[test]
    fn the_app_key_is_minted_once_and_never_replaced() {
        let store = InMemorySecretStore::new();
        assert_eq!(app_key(&store).unwrap(), None);
        let key = create_app_key(&store).unwrap();
        assert!(matches!(
            create_app_key(&store),
            Err(AppStateError::KeyExists(_))
        ));
        assert_eq!(app_key(&store.for_another_device()).unwrap(), Some(key));
    }
}
