//! The app-level snapshot is signed by the device that wrote it; a changed
//! byte or an unknown signer is refused on load.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use app_state::{
    AppStateError, AppStateStore, Connection, ConnectionId, ConnectionKind, DeviceIdentity,
    InMemorySecretStore, KeyboardScheme, PlainSyncFiles, SecretBytes, SecretItem, SecretKind,
    SecretStore, SnapshotRefusal, SyncFiles,
};

fn open(dir: &Path, secrets: &InMemorySecretStore) -> (AppStateStore, app_state::AppStateLoad) {
    let identity = DeviceIdentity::load_or_create(secrets).unwrap();
    let files: Arc<dyn SyncFiles> = Arc::new(PlainSyncFiles);
    AppStateStore::open(files, dir, secrets, identity).unwrap()
}

fn mail_account() -> Connection {
    Connection {
        id: ConnectionId::new("con-mail"),
        kind: ConnectionKind::MailAccount,
        label: "Synthetic mail".to_owned(),
    }
}

/// The Mac writes a snapshot with a mail connection; returns its path.
fn mac_saves(dir: &Path, mac: &InMemorySecretStore) -> std::path::PathBuf {
    let (mut store, load) = open(dir, mac);
    assert_eq!(load.merged, 0);
    store
        .edit(|state| {
            state.put_connection(&mail_account())?;
            state.set_keyboard_scheme(KeyboardScheme::Vim)?;
            state.set_workload_threshold(Duration::from_secs(7 * 3600))
        })
        .unwrap();
    store.save().unwrap()
}

#[test]
fn a_signed_snapshot_reaches_the_other_device() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    let phone = mac.for_another_device();
    let path = mac_saves(temp.path(), &mac);
    assert!(
        !fs::read(&path)
            .unwrap()
            .windows(14)
            .any(|window| window == b"Synthetic mail"),
        "the snapshot is encrypted"
    );

    let (mut phone_store, load) = open(temp.path(), &phone);
    assert_eq!(load.merged, 1);
    assert!(load.refused.is_empty());
    assert_eq!(phone_store.state().connections(), [mail_account()]);
    assert_eq!(phone_store.state().keyboard_scheme(), KeyboardScheme::Vim);

    phone_store.save().unwrap();
    let (mut mac_store, load) = open(temp.path(), &mac);
    assert_eq!(load.merged, 2);
    assert_eq!(mac_store.heads(), phone_store.heads());
}

#[test]
fn tampering_with_one_byte_is_detected_and_the_snapshot_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    let phone = mac.for_another_device();
    let path = mac_saves(temp.path(), &mac);
    let original = fs::read(&path).unwrap();
    let mac_device = DeviceIdentity::load_or_create(&mac)
        .unwrap()
        .device_id()
        .clone();

    // A byte of the ciphertext, of the signature, and of the header's sequence.
    let sequence_byte = 4 + 1 + 1 + 2 + 1 + mac_device.as_str().len();
    for index in [original.len() / 2, original.len() - 1, sequence_byte] {
        let mut tampered = original.clone();
        tampered[index] ^= 0x01;
        fs::write(&path, &tampered).unwrap();
        let (phone_store, load) = open(temp.path(), &phone);
        assert_eq!(load.merged, 0, "byte {index}");
        assert_eq!(load.refused.len(), 1);
        assert_eq!(
            load.refused[0].reason,
            SnapshotRefusal::BadSignature(mac_device.clone())
        );
        assert!(
            load.refused[0].reason.to_string().contains("altered"),
            "{}",
            load.refused[0].reason
        );
        assert!(
            phone_store.state().connections().is_empty(),
            "nothing of it is merged"
        );
    }

    let mut truncated = original.clone();
    truncated.truncate(original.len() - 70);
    fs::write(&path, &truncated).unwrap();
    let (_, load) = open(temp.path(), &phone);
    assert!(matches!(
        load.refused[0].reason,
        SnapshotRefusal::BadSignature(_)
    ));

    fs::write(&path, &original).unwrap();
    let (phone_store, load) = open(temp.path(), &phone);
    assert_eq!(load.merged, 1);
    assert_eq!(phone_store.state().connections(), [mail_account()]);
}

#[test]
fn a_snapshot_from_a_device_without_a_published_key_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    open(temp.path(), &mac).0.save().unwrap();
    // A store that shares nothing with the Mac: its key is published nowhere
    // the Mac sees. Its snapshot is written elsewhere and copied in.
    let elsewhere = tempfile::tempdir().unwrap();
    let stranger = InMemorySecretStore::new();
    let written = mac_saves(elsewhere.path(), &stranger);
    let stranger_path = temp.path().join(written.file_name().unwrap());
    fs::copy(&written, &stranger_path).unwrap();

    let (mac_store, load) = open(temp.path(), &mac);
    assert_eq!(load.merged, 1, "the Mac's own snapshot");
    assert_eq!(load.refused.len(), 1);
    assert_eq!(load.refused[0].path, stranger_path);
    assert!(matches!(
        load.refused[0].reason,
        SnapshotRefusal::UnknownDevice(_)
    ));
    assert!(mac_store.state().connections().is_empty());
}

#[test]
fn a_regenerated_device_keeps_its_old_snapshot_trusted() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    mac_saves(temp.path(), &mac);
    let renewed = DeviceIdentity::regenerate(&mac).unwrap();
    let files: Arc<dyn SyncFiles> = Arc::new(PlainSyncFiles);
    let (store, load) = AppStateStore::open(files, temp.path(), &mac, renewed).unwrap();
    assert_eq!(load.merged, 1);
    assert_eq!(store.state().connections(), [mail_account()]);
}

#[test]
fn a_refused_own_snapshot_is_kept_aside_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    let path = mac_saves(temp.path(), &mac);
    let mut tampered = fs::read(&path).unwrap();
    let middle = tampered.len() / 2;
    tampered[middle] ^= 0x01;
    fs::write(&path, &tampered).unwrap();

    let (mut store, load) = open(temp.path(), &mac);
    assert_eq!(load.refused.len(), 1);
    store.save().unwrap();
    let aside = temp.path().join(format!(
        ".{}.refused",
        path.file_name().unwrap().to_string_lossy()
    ));
    assert_eq!(fs::read(&aside).unwrap(), tampered);
    let (_, load) = open(temp.path(), &mac);
    assert_eq!(
        (load.merged, load.refused.len()),
        (1, 0),
        "the set-aside file is never read"
    );
}

#[test]
fn a_device_that_sees_another_devices_key_but_not_the_app_key_waits() {
    // Another device's app folder is elsewhere (each Mac's App Group
    // container), so its snapshots are not here; its published public key
    // arrived through the keychain before the app key did.
    let temp = tempfile::tempdir().unwrap();
    let other = InMemorySecretStore::new();
    let other_identity = DeviceIdentity::load_or_create(&other).unwrap();
    let here = InMemorySecretStore::new();
    here.put(SecretItem::new(
        SecretKind::DevicePublicKey,
        other_identity.device_id().as_str(),
        SecretBytes::new(other_identity.public_key().to_bytes().to_vec()),
    ))
    .unwrap();

    let identity = DeviceIdentity::load_or_create(&here).unwrap();
    let refused = AppStateStore::open(Arc::new(PlainSyncFiles), temp.path(), &here, identity);
    assert!(matches!(refused, Err(AppStateError::KeyNotArrived(_))));
    assert!(
        here.list(SecretKind::AppDataKey).unwrap().is_empty(),
        "no second app key"
    );

    // Alone, the first device mints the key.
    let (store, _) = open(temp.path(), &InMemorySecretStore::new());
    drop(store);
}

#[test]
fn a_device_that_never_held_the_app_key_makes_no_other_device_wait() {
    // The Mac loaded its identity and stopped before its app state opened
    // (a Keychain or container failure). Had its public key been published,
    // the phone would wait for an app key the Mac never minted, and the Mac,
    // relaunched, would wait for the phone's: neither would ever mint.
    let mac = InMemorySecretStore::new();
    let phone = mac.for_another_device();
    let mac_identity = DeviceIdentity::load_or_create(&mac).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (store, _) = open(temp.path(), &phone);
    drop(store);
    assert_eq!(phone.list(SecretKind::AppDataKey).unwrap().len(), 1);

    // The phone published its key once it held the app key; the Mac opens
    // with the key that synchronised and publishes its own.
    let mac_folder = tempfile::tempdir().unwrap();
    let files: Arc<dyn SyncFiles> = Arc::new(PlainSyncFiles);
    AppStateStore::open(files, mac_folder.path(), &mac, mac_identity).unwrap();
    assert_eq!(phone.list(SecretKind::DevicePublicKey).unwrap().len(), 2);
}
