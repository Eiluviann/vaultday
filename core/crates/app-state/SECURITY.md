# app-state: threat notes for v1

What this crate protects, what it does not, and what is still open. The
crate holds everything Vaultday keeps that is not in the vault: the
per-workspace document (day order, time boxes, sessions, rollover history,
ritual answers, notification state, id counters) and the app-level document
(connections, sources, feature enablement, settings, security fields).

## Protected in v1

- **App data is encrypted at rest.** Every change file and snapshot is
  sealed with ChaCha20-Poly1305 under a 256-bit data key with a fresh random
  96-bit nonce per file. A file is unreadable without the key: a reader
  holding another key, or none, sees only the header below.
- **Integrity of every file.** The header (format, kind, schema version,
  device id, sequence, nonce) is authenticated as associated data, so a file
  cannot be moved to another device's folder or renumbered without failing
  its tag. A file that fails is treated as "not arrived yet", never applied.
- **Signed app-level state.** The app document decides where data may go
  (connections, sources, feature switches, security fields), so each device's
  copy is also signed with that device's Ed25519 key over the sealed bytes.
  Loading verifies the signature against the device public keys published in
  the secret store before decrypting; a snapshot that fails, or whose device
  has no published key, is refused and reported, and none of its content is
    merged. A synced or edited file can therefore never open a connection.
  Merging is monotonic: replaying an old signed snapshot cannot undo a later
  setting. A device whose own snapshot was refused keeps it aside as
  `.<device-id>.bin.refused` before it writes a new one.
- **Key hierarchy.** One data key per workspace, minted on workspace
  creation and stored synchronisable; one data key for the app document,
  synchronisable; one Ed25519 signing key per device, stored device-only,
  with its public half published synchronisable under the device id once
  the device holds the app key (other devices read a published key as "the
  app key is on its way", so a device that never held it publishes none). Keys
  are minted through `SecretStore::add`, which never replaces an existing
  item, and the app key is minted only by a device that finds no snapshot in
  the app folder, an evicted iCloud placeholder of one included: a device
  whose keychain has not received the key yet gets
  `KeyNotArrived` instead of minting a second key that would orphan the
  others' snapshots. Key material is held in `zeroize` types and never
  printed. `delete_workspace` deletes the workspace key everywhere; the
  change files left in the vault are then unreadable by anyone.
- **Secrets stay out of the vault.** `settings_guard::secret_like_fields`
  finds fields of a `settings.json` whose key names a secret or whose value
  has the form of one; secrets belong in the secret store.
- **Volatile state stays out of the workspace.** `VolatileStateDir` refuses a
  folder inside any workspace, so indexes and caches never sync with the vault.

## Not protected in v1

- **The vault itself is plaintext.** Notes, task lines and `settings.json`
  are protected only by the disk (FileVault) and by the sync provider's own
  encryption. Anything that can read the vault folder can read them.
- **The documents in memory are plaintext** while the app runs.
- **Workspace change files are encrypted but not signed.** Any holder of a
  workspace key can write a change file that others apply; the key is shared
  by all of the user's devices by design. Only the app-level state is signed.
- **File names and sizes leak.** Device ids, file numbers, file sizes and
  write times are visible in the sync folder.
- **Deletion is not prevented.** Anyone with write access to the sync folder
  can delete files; other devices then miss those changes until the owner's
  next compaction rewrites them into a snapshot.
- **Security modes are off.** The fields (mode `off`, `enhanced`, `strict`,
  always-ask-before-sending) exist and are stored in the signed app state,
  but nothing enforces them until roadmap Phase 5. Every workspace reads as
  `off` until set.
- **Id blocks are guaranteed apart for every claim made after seeing the
  others'.** The workspace creator holds block 0; every other claim lands on
  a block chosen by a hash of the run's random actor. A block's next number
  is the highest any run wrote, and a run that has not read every file of
  its own folder mints only from blocks it claimed itself, so one device
  never mints an id twice. Two claims made while neither side has seen the
  other (two devices offline, or a run that missed its device's last claim)
  land on the same block with odds of 1 in 9 999; when the claims meet, the
  smaller device id keeps the block and the other moves on, but ids minted
  from that block before then can collide. This is the residual risk of
  minting offline. It assumes a device's own files are always listed in its
  folder (placeholders count); a restore from an older backup that keeps the
  device id breaks that, which is what `DeviceIdentity::regenerate` is for.
- **Concurrent creation is not prevented.** Two devices that create the
  same workspace, or mint the app key, before their keychains synchronise
  each keep a key; iCloud Keychain then keeps one of the two. A device does
  wait when it already sees another device's change files (workspace) or
  published public key (app key); it mints only when it sees neither. Nor
  does a device claim a new workspace id over a sync folder that holds
  another device's change files, or an evicted `workspace.id`, before that
  device's `workspace.id` arrived; it waits for the id. On the
  Mac the app document's snapshots live in each Mac's App Group container
  and do not sync between Macs yet (where they sync is decided in onboarding),
  so the published-key check is the only guard for the app key there. Onboarding
  creates a workspace on one device and the others join it.
- **One writer per device.** Two processes writing app state under one
  device id would number the same files; only the app writes app state, the
  CLI goes through the app's localhost API. Connection ids are chosen
  randomly by the app, so two devices never add one id for two connections.

## The Mac's Keychain store (PR 7a)

`apps/apple/VaultdayCore/Sources/VaultdayCore/Keychain/` implements the
port over the data protection keychain, every query naming its service,
account, access group and `kSecAttrSynchronizable`:

- A data key (workspace or app) is a synchronizable item
  (`kSecAttrAccessibleAfterFirstUnlock`): this is how the user's other Apple
  devices get it, protected by iCloud Keychain's end-to-end encryption. This
  Mac also keeps a device-only copy sealed by its Secure Enclave P-256 key
  (ECIES: ephemeral key agreement, HKDF-SHA256, AES-GCM with the item's
  address as associated data) and reads the key from that copy. The
  synchronised item is still in this Mac's keychain, as iCloud Keychain
  needs it to be, so the sealed copy adds a hardware-bound read path but does
  not remove the synchronised copy from the disk. The copy carries the
  synchronised item's modification date and is rebuilt when that changes, so
  a copy never outlives iCloud Keychain resolving a conflict for another
  device's key. A copy whose synchronised item is gone (the workspace was
  removed on any device, or the user signed out of iCloud Keychain without
  keeping its items) is deleted on read; that key is then gone on this Mac.
- The device signing key seed is a device-only item
  (`…AfterFirstUnlockThisDeviceOnly`), sealed by the Secure Enclave key, so
  it opens only on this Mac. One that no longer opens (a reset enclave) reads
  as missing and the core mints a new device identity. The public key is
  synchronizable and not sealed.
- **The workspace id** under which the data key is stored is a random
  `ws-…` id that the first device writes to `<workspace>/.vaultday/sync/workspace.id`,
  never the manifest's name, so two workspaces of one name keep their keys
  apart and a rename keeps its key. Removing a workspace destroys the key and
  moves its unreadable change files aside to `.vaultday/sync-removed-<time>`.
- A Mac without a Secure Enclave stores device-only items unsealed.
- Key bytes cross the UniFFI bridge in plain buffers, which are freed but
  not wiped; on the Rust side they go straight into `zeroize` types. The
  store never logs, and its errors carry `OSStatus` values only.

## File format

```text
magic "VDAY" | format u8 = 1 | kind u8 | schema u16 BE | device id length u8 |
device id | sequence u64 BE | nonce (12) | ciphertext and tag (16)
```

Kinds: 1 change batch (`<device-id>/NNNN.bin`), 2 workspace snapshot
(`<device-id>/snapshot-NNNN.bin`), 3 app snapshot (`<app dir>/<device-id>.bin`,
followed by a 64-byte Ed25519 signature over `"vaultday app snapshot v1\0"`
and the sealed bytes). Everything before the ciphertext is associated data.

## Sync folder rules

- Writes are temp-then-rename and flushed before the rename.
- A file that cannot be read, opened or parsed yet (an iCloud placeholder, a
  truncated or half-synced file, one under another key, one from a newer
  version) is skipped and retried on the next pull; it is never corruption.
- Conflict copies (`0007 2.bin`, `0007 (conflicted copy).bin`,
  `0007.sync-conflict-….bin`) are read as ordinary files; applying is
  idempotent.
- Each device writes and compacts only its own folder, and compacts only
  once it has read every file there. A snapshot covering `NNNN` holds the
  whole document, changes still waiting for their dependencies included, and
  is a tombstone for the device's files up to `NNNN`: the device never reuses
  those numbers, and deletes a file that reappears under one of them only
  when every change in it is already in the document. Snapshots are applied
  with `load_incremental`, which keeps those waiting changes; a load-and-merge
  would drop them.
- Device ids are random. A restore that carries the device-only items along
  calls `DeviceIdentity::regenerate`; every run also writes under a fresh
  Automerge actor, so a run that could not read some of its own earlier
  changes never reuses their sequence numbers.

## Open items

- **The real Keychain is tested by hand.** An unsigned test bundle has no
  Keychain entitlement (`SecItemAdd` answers -34018), so the Swift store's
  tests run over an in-memory backend that models iCloud Keychain; the
  `SecItem` queries are checked attribute by attribute. The two-Mac run over
  iCloud Keychain is the manual test in the PR 7a description.
- **Linux key transport.** How a Linux device receives a workspace key
  without iCloud Keychain is designed in the Electron phase, after v1.
- **One file port.** Change files go through the byte-level `SyncFiles` port
  because `vault_base::FileAccess` carries UTF-8 text only and was frozen
  while PR 5 was in review. Folding `read_bytes` and `write_atomic_bytes`
  into `FileAccess` gives the Mac app one port to implement.
- **Settings loader.** The app's writes of `settings.json` (the bindings'
  workspace form: the Configure sheet and a new workspace's note prefill) run
  `settings_guard::secret_like_fields` first, refusing or leaving out a
  value in a secret's form. `vault-base`'s loader, and so a hand-edited file
  or the CLI, does not call it yet.
