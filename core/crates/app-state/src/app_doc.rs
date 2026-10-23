//! The app-level document: connections, sources, feature enablement, app
//! settings and the security fields. It is stored signed (see
//! [`crate::signed`]), because what it says decides where data may go: a
//! synced or edited note can never switch on a connection.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ChangeHash, ObjId, ObjType, ReadDoc, ScalarValue, Value};
use domain::registry::{Enablement, FeatureId};
use domain::workspace::WorkspaceId;
use jiff::civil::Time;

use crate::device::DeviceId;
use crate::doc::{
    KEY_SEPARATOR, composite_key, delete_if_present, get_bool, get_bytes, get_map, get_str,
    get_u64, keys, root_object, schema_document,
};
use crate::error::AppStateError;

mod onboarding;
mod preferences;

pub use onboarding::{
    AccountPart, FeatureScopeChoice, FeatureScopeMode, ONBOARDING_STEP_COUNT, OnboardingProgress,
    SyncChoice, SyncHome, SyncService,
};
pub use preferences::{
    AI_TIER_EVERYWHERE, AgentChanges, AiScopeChoice, AiTier, BreakPattern, DataRetention,
    ListPosition, OtherAppsAccess, OverrunWarning, ProviderKeyRecord, ProviderKeyScope,
    ProviderKind, ShortcutCaps, WorkingHours, WorkingHoursKind,
};

/// Version of this document's schema, carried in every snapshot's header.
pub const APP_SCHEMA_VERSION: u16 = 1;
/// The workload threshold until the user sets one: an eight-hour day.
pub const DEFAULT_WORKLOAD_THRESHOLD: Duration = Duration::from_secs(8 * 60 * 60);
const SCHEMA_ACTOR: &str = "vaultday:schema:app:1";

const CONNECTIONS: &str = "connections";
const SOURCES: &str = "sources";
const FEATURES: &str = "features";
const SETTINGS: &str = "settings";
const DEVICE_SETTINGS: &str = "device_settings";
const RITUAL_TIMES: &str = "ritual_times";
const SECURITY: &str = "security";
const MAPS: [&str; 7] = [
    CONNECTIONS,
    SOURCES,
    FEATURES,
    SETTINGS,
    DEVICE_SETTINGS,
    RITUAL_TIMES,
    SECURITY,
];
const APP_SCOPE: &str = "app";
const FOLDER_GRANT_BOOKMARK: &str = "folder_grant_bookmark";
const FOLDER_GRANT_PATH: &str = "folder_grant_path";
const WORKSPACE_SCOPE: &str = "workspace";

/// Identifies a connection, chosen by the app when it adds one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(String);

/// What a connection reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConnectionKind {
    /// An Obsidian vault.
    Vault,
    /// A plain folder of markdown.
    Folder,
    /// A calendar.
    Calendar,
    /// A reminders list.
    ReminderList,
    /// A mail account; its password or token lives in the secret store, never here.
    MailAccount,
}

/// One entry of the app's list of connections. Holds no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    /// The connection's id.
    pub id: ConnectionId,
    /// What it reaches.
    pub kind: ConnectionKind,
    /// What the user calls it.
    pub label: String,
}

/// A folder a workspace reads: a vault connection plus a path inside it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Source {
    /// The workspace the folder belongs to.
    pub workspace: WorkspaceId,
    /// The vault or folder connection.
    pub vault: ConnectionId,
    /// The path inside the vault, `/`-separated, empty for the vault root.
    pub subpath: String,
}

/// Access this device holds to a vault or folder connection: the platform's
/// grant (a security-scoped bookmark on the Mac) and the path it last
/// resolved to. A grant only works on the device that made it, so grants are
/// kept per device; it is not a secret, but it is what lets the sandboxed app
/// open the folder, which is why it lives in the signed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderGrant {
    /// The vault or folder connection the grant opens.
    pub connection: ConnectionId,
    /// The platform's opaque grant bytes.
    pub bookmark: Vec<u8>,
    /// The folder's path when the grant was made or last resolved.
    pub path: String,
}

/// Where a feature switch applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnablementScope {
    /// The whole app.
    App,
    /// One workspace.
    Workspace(WorkspaceId),
}

/// The keyboard scheme the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardScheme {
    /// The platform's standard keys.
    #[default]
    Standard,
    /// Vim motions.
    Vim,
}

/// A workspace's security mode. Stored now, enforced from roadmap Phase 5;
/// every workspace is [`SecurityMode::Off`] in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecurityMode {
    /// No mode.
    #[default]
    Off,
    /// Enhanced.
    Enhanced,
    /// Strict.
    Strict,
}

/// A workspace's security fields; inert in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspaceSecurity {
    /// The mode.
    pub mode: SecurityMode,
    /// Whether anything leaving the device asks first.
    pub always_ask_before_sending: bool,
}

/// The app-level document of one device.
#[derive(Debug)]
pub struct AppState {
    doc: AutoCommit,
    device: DeviceId,
    objects: Objects,
}

#[derive(Debug, Clone)]
struct Objects {
    connections: ObjId,
    sources: ObjId,
    features: ObjId,
    settings: ObjId,
    device_settings: ObjId,
    ritual_times: ObjId,
    security: ObjId,
}

impl ConnectionId {
    /// Wraps the id text.
    pub fn new(id: impl Into<String>) -> ConnectionId {
        ConnectionId(id.into())
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ConnectionKind {
    const ALL: [ConnectionKind; 5] = [
        ConnectionKind::Vault,
        ConnectionKind::Folder,
        ConnectionKind::Calendar,
        ConnectionKind::ReminderList,
        ConnectionKind::MailAccount,
    ];

    const fn label(self) -> &'static str {
        match self {
            ConnectionKind::Vault => "vault",
            ConnectionKind::Folder => "folder",
            ConnectionKind::Calendar => "calendar",
            ConnectionKind::ReminderList => "reminder-list",
            ConnectionKind::MailAccount => "mail-account",
        }
    }
}

impl SecurityMode {
    const fn label(self) -> &'static str {
        match self {
            SecurityMode::Off => "off",
            SecurityMode::Enhanced => "enhanced",
            SecurityMode::Strict => "strict",
        }
    }
}

impl AppState {
    /// A document holding only the schema change, writing as `device`.
    pub(crate) fn new(device: DeviceId) -> Result<AppState, AppStateError> {
        let doc = schema_document(SCHEMA_ACTOR, &MAPS, &[])?.with_actor(device.run_actor()?);
        let objects = Objects {
            connections: root_object(&doc, CONNECTIONS)?,
            sources: root_object(&doc, SOURCES)?,
            features: root_object(&doc, FEATURES)?,
            settings: root_object(&doc, SETTINGS)?,
            device_settings: root_object(&doc, DEVICE_SETTINGS)?,
            ritual_times: root_object(&doc, RITUAL_TIMES)?,
            security: root_object(&doc, SECURITY)?,
        };
        Ok(AppState {
            doc,
            device,
            objects,
        })
    }

    pub(crate) fn doc_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
    }

    /// The document heads: equal on two devices exactly when they converged.
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    // Connections and sources.

    /// Every connection, by id.
    #[must_use]
    pub fn connections(&self) -> Vec<Connection> {
        keys(&self.doc, &self.objects.connections)
            .into_iter()
            .filter_map(|id| self.connection(&ConnectionId::new(id)))
            .collect()
    }

    /// One connection.
    #[must_use]
    pub fn connection(&self, id: &ConnectionId) -> Option<Connection> {
        let record = get_map(&self.doc, &self.objects.connections, id.as_str())?;
        let kind_label = get_str(&self.doc, &record, "kind")?;
        let kind = ConnectionKind::ALL
            .into_iter()
            .find(|kind| kind.label() == kind_label)?;
        Some(Connection {
            id: id.clone(),
            kind,
            label: get_str(&self.doc, &record, "label").unwrap_or_default(),
        })
    }

    /// Adds a connection or replaces the one with the same id.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn put_connection(&mut self, connection: &Connection) -> Result<(), AppStateError> {
        let parent = self.objects.connections.clone();
        let record = match self.doc.get(&parent, connection.id.as_str())? {
            Some((Value::Object(ObjType::Map), record)) => record,
            _ => self
                .doc
                .put_object(&parent, connection.id.as_str(), ObjType::Map)?,
        };
        self.doc.put(&record, "kind", connection.kind.label())?;
        self.doc.put(&record, "label", connection.label.as_str())?;
        Ok(())
    }

    /// Removes a connection and every source that reads through it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_connection(&mut self, id: &ConnectionId) -> Result<(), AppStateError> {
        for source in self
            .all_sources()
            .into_iter()
            .filter(|source| &source.vault == id)
        {
            self.remove_source(&source)?;
        }
        delete_if_present(&mut self.doc, &self.objects.connections, id.as_str())
    }

    /// The sources of one workspace.
    #[must_use]
    pub fn sources(&self, workspace: &WorkspaceId) -> Vec<Source> {
        self.all_sources()
            .into_iter()
            .filter(|source| &source.workspace == workspace)
            .collect()
    }

    /// Every source of every workspace.
    #[must_use]
    pub fn all_sources(&self) -> Vec<Source> {
        let mut sources: Vec<Source> = keys(&self.doc, &self.objects.sources)
            .iter()
            .filter(|key| get_bool(&self.doc, &self.objects.sources, key) == Some(true))
            .filter_map(|key| {
                let mut parts = key.splitn(3, KEY_SEPARATOR);
                Some(Source {
                    workspace: WorkspaceId::new(parts.next()?),
                    vault: ConnectionId::new(parts.next()?),
                    subpath: parts.next()?.to_owned(),
                })
            })
            .collect();
        sources.sort();
        sources
    }

    /// Adds a source; adding one that exists is a no-op.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn add_source(&mut self, source: &Source) -> Result<(), AppStateError> {
        self.doc
            .put(&self.objects.sources, source_key(source), true)?;
        Ok(())
    }

    /// Removes a source.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_source(&mut self, source: &Source) -> Result<(), AppStateError> {
        delete_if_present(&mut self.doc, &self.objects.sources, &source_key(source))
    }

    // Feature enablement.

    /// Switches a feature on or off for the app or for one workspace.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_feature_enabled(
        &mut self,
        scope: &EnablementScope,
        feature: &FeatureId,
        is_enabled: bool,
    ) -> Result<(), AppStateError> {
        let key = match scope {
            EnablementScope::App => composite_key(&[APP_SCOPE, feature.as_str()]),
            EnablementScope::Workspace(workspace) => {
                composite_key(&[WORKSPACE_SCOPE, workspace.as_str(), feature.as_str()])
            }
        };
        self.doc.put(&self.objects.features, key, is_enabled)?;
        Ok(())
    }

    /// Every switch, as the registry reads them.
    #[must_use]
    pub fn enablement(&self) -> Enablement {
        let mut enablement = Enablement::new();
        for key in keys(&self.doc, &self.objects.features) {
            if get_bool(&self.doc, &self.objects.features, &key) != Some(true) {
                continue;
            }
            let parts: Vec<&str> = key.split(KEY_SEPARATOR).collect();
            match parts.as_slice() {
                [APP_SCOPE, feature] => enablement.enable_for_app(*feature),
                [WORKSPACE_SCOPE, workspace, feature] => {
                    enablement.enable_for_workspace(*workspace, *feature);
                }
                _ => {}
            }
        }
        enablement
    }

    // Settings.

    /// Whether hovering selects on this device; on until switched off.
    #[must_use]
    pub fn hover_selects(&self) -> bool {
        get_bool(
            &self.doc,
            &self.objects.device_settings,
            &self.device_key("hover_selects"),
        )
        .unwrap_or(true)
    }

    /// Sets hover selects for this device only.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_hover_selects(&mut self, is_on: bool) -> Result<(), AppStateError> {
        let key = self.device_key("hover_selects");
        self.doc.put(&self.objects.device_settings, key, is_on)?;
        Ok(())
    }

    /// When a ritual runs, by ritual name.
    #[must_use]
    pub fn ritual_times(&self) -> BTreeMap<String, Time> {
        keys(&self.doc, &self.objects.ritual_times)
            .into_iter()
            .filter_map(|ritual| {
                let time = get_str(&self.doc, &self.objects.ritual_times, &ritual)?
                    .parse()
                    .ok()?;
                Some((ritual, time))
            })
            .collect()
    }

    /// Sets when a ritual runs; `None` clears it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_ritual_time(
        &mut self,
        ritual: &str,
        time: Option<Time>,
    ) -> Result<(), AppStateError> {
        let rituals = &self.objects.ritual_times;
        match time {
            Some(time) => {
                self.doc.put(rituals, ritual, time.to_string())?;
                Ok(())
            }
            None => delete_if_present(&mut self.doc, rituals, ritual),
        }
    }

    /// The planned work a day may hold before planning warns.
    #[must_use]
    pub fn workload_threshold(&self) -> Duration {
        get_u64(&self.doc, &self.objects.settings, "workload_threshold_secs")
            .map_or(DEFAULT_WORKLOAD_THRESHOLD, Duration::from_secs)
    }

    /// Sets the workload threshold, in whole seconds.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_workload_threshold(&mut self, threshold: Duration) -> Result<(), AppStateError> {
        self.doc.put(
            &self.objects.settings,
            "workload_threshold_secs",
            threshold.as_secs(),
        )?;
        Ok(())
    }

    /// The keyboard scheme.
    #[must_use]
    pub fn keyboard_scheme(&self) -> KeyboardScheme {
        match get_str(&self.doc, &self.objects.settings, "keyboard_scheme").as_deref() {
            Some("vim") => KeyboardScheme::Vim,
            _ => KeyboardScheme::Standard,
        }
    }

    /// Sets the keyboard scheme.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_keyboard_scheme(&mut self, scheme: KeyboardScheme) -> Result<(), AppStateError> {
        let label = match scheme {
            KeyboardScheme::Standard => "standard",
            KeyboardScheme::Vim => "vim",
        };
        self.doc
            .put(&self.objects.settings, "keyboard_scheme", label)?;
        Ok(())
    }

    // Security fields.

    /// A workspace's security fields; off until set, and inert in v1.
    #[must_use]
    pub fn security(&self, workspace: &WorkspaceId) -> WorkspaceSecurity {
        let field = |name: &str| composite_key(&[workspace.as_str(), name]);
        let mode = match get_str(&self.doc, &self.objects.security, &field("mode")).as_deref() {
            Some("enhanced") => SecurityMode::Enhanced,
            Some("strict") => SecurityMode::Strict,
            _ => SecurityMode::Off,
        };
        WorkspaceSecurity {
            mode,
            always_ask_before_sending: get_bool(
                &self.doc,
                &self.objects.security,
                &field("always_ask_before_sending"),
            )
            .unwrap_or(false),
        }
    }

    /// Stores a workspace's security fields. Nothing enforces them in v1.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_security(
        &mut self,
        workspace: &WorkspaceId,
        security: &WorkspaceSecurity,
    ) -> Result<(), AppStateError> {
        let field = |name: &str| composite_key(&[workspace.as_str(), name]);
        self.doc
            .put(&self.objects.security, field("mode"), security.mode.label())?;
        self.doc.put(
            &self.objects.security,
            field("always_ask_before_sending"),
            security.always_ask_before_sending,
        )?;
        Ok(())
    }

    // Folder grants.

    /// This device's folder grants, by connection id.
    #[must_use]
    pub fn folder_grants(&self) -> Vec<FolderGrant> {
        let prefix = composite_key(&[self.device.as_str(), FOLDER_GRANT_BOOKMARK, ""]);
        keys(&self.doc, &self.objects.device_settings)
            .iter()
            .filter_map(|key| key.strip_prefix(&prefix))
            .filter_map(|connection| self.folder_grant(&ConnectionId::new(connection)))
            .collect()
    }

    /// This device's grant for one connection.
    #[must_use]
    pub fn folder_grant(&self, connection: &ConnectionId) -> Option<FolderGrant> {
        let settings = &self.objects.device_settings;
        let bookmark = get_bytes(
            &self.doc,
            settings,
            &self.grant_key(FOLDER_GRANT_BOOKMARK, connection),
        )?;
        Some(FolderGrant {
            connection: connection.clone(),
            bookmark,
            path: get_str(
                &self.doc,
                settings,
                &self.grant_key(FOLDER_GRANT_PATH, connection),
            )
            .unwrap_or_default(),
        })
    }

    /// Stores this device's grant for a connection, replacing an older one
    /// (a stale bookmark refreshed at launch).
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn put_folder_grant(&mut self, grant: &FolderGrant) -> Result<(), AppStateError> {
        let settings = self.objects.device_settings.clone();
        let bookmark_key = self.grant_key(FOLDER_GRANT_BOOKMARK, &grant.connection);
        let path_key = self.grant_key(FOLDER_GRANT_PATH, &grant.connection);
        self.doc.put(
            &settings,
            bookmark_key,
            ScalarValue::Bytes(grant.bookmark.clone()),
        )?;
        self.doc.put(&settings, path_key, grant.path.as_str())?;
        Ok(())
    }

    /// Forgets this device's grant for a connection.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_folder_grant(&mut self, connection: &ConnectionId) -> Result<(), AppStateError> {
        let settings = self.objects.device_settings.clone();
        for field in [FOLDER_GRANT_BOOKMARK, FOLDER_GRANT_PATH] {
            let key = self.grant_key(field, connection);
            delete_if_present(&mut self.doc, &settings, &key)?;
        }
        Ok(())
    }

    fn grant_key(&self, field: &str, connection: &ConnectionId) -> String {
        composite_key(&[self.device.as_str(), field, connection.as_str()])
    }

    fn device_key(&self, setting: &str) -> String {
        composite_key(&[self.device.as_str(), setting])
    }
}

fn source_key(source: &Source) -> String {
    composite_key(&[
        source.workspace.as_str(),
        source.vault.as_str(),
        &source.subpath,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(device: &str) -> AppState {
        AppState::new(DeviceId::new(device).unwrap()).unwrap()
    }

    #[test]
    fn defaults_hold_until_set() {
        let state = state("mac-a");
        assert!(state.hover_selects());
        assert_eq!(state.workload_threshold(), DEFAULT_WORKLOAD_THRESHOLD);
        assert_eq!(state.keyboard_scheme(), KeyboardScheme::Standard);
        assert_eq!(
            state.security(&WorkspaceId::new("acme")),
            WorkspaceSecurity::default()
        );
        assert!(state.connections().is_empty());
        assert_eq!(state.enablement(), Enablement::new());
    }

    #[test]
    fn settings_round_trip_and_hover_selects_is_per_device() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        mac.set_hover_selects(false).unwrap();
        mac.set_workload_threshold(Duration::from_secs(6 * 3600))
            .unwrap();
        mac.set_keyboard_scheme(KeyboardScheme::Vim).unwrap();
        mac.set_ritual_time("planning", Some(Time::constant(8, 30, 0, 0)))
            .unwrap();
        mac.set_ritual_time("shutdown", Some(Time::constant(17, 0, 0, 0)))
            .unwrap();
        mac.set_ritual_time("shutdown", None).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert!(!mac.hover_selects());
        assert!(other.hover_selects(), "hover selects is set per device");
        assert_eq!(other.workload_threshold(), Duration::from_secs(6 * 3600));
        assert_eq!(other.keyboard_scheme(), KeyboardScheme::Vim);
        assert_eq!(
            other.ritual_times().into_iter().collect::<Vec<_>>(),
            [("planning".to_owned(), Time::constant(8, 30, 0, 0))]
        );
    }

    #[test]
    fn folder_grants_round_trip_per_device() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        let grant = FolderGrant {
            connection: ConnectionId::new("con-1"),
            bookmark: vec![0, 1, 2, 0xff],
            path: "/Users/someone/Vault".to_owned(),
        };
        mac.put_folder_grant(&grant).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert_eq!(mac.folder_grants(), std::slice::from_ref(&grant));
        assert!(
            other.folder_grants().is_empty(),
            "a grant only opens the folder on the device that made it"
        );

        let refreshed = FolderGrant {
            bookmark: vec![9; 3],
            path: "/Users/someone/Moved".to_owned(),
            ..grant.clone()
        };
        mac.put_folder_grant(&refreshed).unwrap();
        assert_eq!(mac.folder_grant(&grant.connection), Some(refreshed));
        mac.remove_folder_grant(&grant.connection).unwrap();
        mac.remove_folder_grant(&grant.connection).unwrap();
        assert!(mac.folder_grants().is_empty());
    }

    #[test]
    fn connections_sources_features_and_security_round_trip() {
        let mut state = state("mac-a");
        let vault = Connection {
            id: ConnectionId::new("con-1"),
            kind: ConnectionKind::Vault,
            label: "Synthetic vault".to_owned(),
        };
        state.put_connection(&vault).unwrap();
        let source = Source {
            workspace: WorkspaceId::new("acme"),
            vault: vault.id.clone(),
            subpath: "Clients/Acme".to_owned(),
        };
        state.add_source(&source).unwrap();
        assert_eq!(state.connections(), std::slice::from_ref(&vault));
        assert_eq!(
            state.sources(&WorkspaceId::new("acme")),
            std::slice::from_ref(&source)
        );
        assert!(state.sources(&WorkspaceId::new("personal")).is_empty());

        let feature = FeatureId::new("synthetic-feature");
        state
            .set_feature_enabled(&EnablementScope::App, &feature, true)
            .unwrap();
        state
            .set_feature_enabled(
                &EnablementScope::Workspace(WorkspaceId::new("acme")),
                &feature,
                true,
            )
            .unwrap();
        let enablement = state.enablement();
        assert!(enablement.is_on_for_app(&feature));
        assert!(enablement.is_on_for_workspace(&WorkspaceId::new("acme"), &feature));
        state
            .set_feature_enabled(&EnablementScope::App, &feature, false)
            .unwrap();
        assert!(!state.enablement().is_on_for_app(&feature));

        let strict = WorkspaceSecurity {
            mode: SecurityMode::Strict,
            always_ask_before_sending: true,
        };
        state
            .set_security(&WorkspaceId::new("acme"), &strict)
            .unwrap();
        assert_eq!(state.security(&WorkspaceId::new("acme")), strict);

        state.remove_connection(&vault.id).unwrap();
        assert!(state.connections().is_empty());
        assert!(
            state.all_sources().is_empty(),
            "sources go with their connection"
        );
    }
}
