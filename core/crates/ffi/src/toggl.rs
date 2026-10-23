//! The Toggl and billing feature set over the bridge: a workspace's billing
//! model in `settings.json`, its rate in the signed app document, the
//! feature's switches, the token (written to the secret store only), the
//! mapping, the queue, the Toggl week and the flat job's dry run.
//!
//! Every call that touches the feature asks the registry first; with the
//! feature off for the workspace, none reaches `vaultday-feature-toggl`
//! beyond that question. Network calls go through the live client, which
//! is refused while `allow_live` is off, and this bridge offers no switch
//! for it: in this build nothing reaches the live service.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_state::{
    AppStateError, BillingRate, EnablementScope, SecretBytes, SecretItem, SecretStore,
};
use domain::billing::parse_billable_flag;
use domain::quick_add::parse_estimate;
use domain::registry::Registry;
use domain::session::SessionId;
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use vault_base::{
    BillingModel, BillingSettings, PlainFileAccess, Workspace, WorkspaceSettings, ops,
    update_settings_file,
};
use vaultday_feature_toggl::{
    BillingPolicy, DrainReport, EnqueueOutcome, EntryDetails, FlatDayStatus, FlatJobInput,
    HttpTogglApi, QueueState, TogglApi, TogglError, TogglFeature, TogglMapping, TogglSettings,
    WeekDayStatus, allow_live, feature_id, has_token, token_id,
};

use crate::app_state::{AppStateHandle, WorkspaceStateHandle, lock};
use crate::{CoreError, WorkspaceHandle};

/// How a workspace bills its time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BillingModelView {
    /// Sessions are billable time.
    Hourly,
    /// The estimate is the quote and the cap.
    Fixed,
    /// Full-time equivalent: a flat day goes out.
    Fte,
}

/// The `billing` block as the Configure sheet edits it, defaults filled in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BillingSettingsView {
    /// The model; `None` when the workspace has none.
    pub model: Option<BillingModelView>,
    /// Hourly: the rounding increment in minutes.
    pub rounding_minutes: u32,
    /// Hourly: whether tasks are billable until marked otherwise.
    pub is_billable_by_default: bool,
    /// FTE: hours of the flat Toggl entry per working day.
    pub flat_hours: f64,
}

/// An hourly rate, kept in the signed app document only.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BillingRateView {
    /// Per hour, in the currency's minor unit.
    pub amount_minor_per_hour: u64,
    /// ISO 4217 code.
    pub currency: String,
}

/// Where the feature stands for one workspace. Never carries the token.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglStatusView {
    /// The app switch.
    pub is_on_for_app: bool,
    /// The workspace switch.
    pub is_on_for_workspace: bool,
    /// Both switches: the registry's answer.
    pub is_enabled: bool,
    /// Whether a token is stored in the Keychain on this device.
    pub has_token: bool,
    /// The app-level `allow_live` switch; off in this build.
    pub is_live_allowed: bool,
}

/// One story's Toggl project.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryProjectView {
    /// The story number.
    pub story: String,
    /// The Toggl project id.
    pub project_id: u64,
}

/// A workspace's Toggl mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct TogglMappingView {
    /// The Toggl workspace id.
    pub toggl_workspace_id: Option<u64>,
    /// The default project id.
    pub default_project_id: Option<u64>,
    /// Projects by story.
    pub story_projects: Vec<StoryProjectView>,
}

/// A Toggl workspace or project to pick from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglChoiceView {
    /// Its Toggl id.
    pub id: u64,
    /// Its name.
    pub name: String,
}

/// Where a queued entry stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TogglEntryStateView {
    /// Waiting to be sent.
    Queued,
    /// In Toggl.
    Sent,
    /// Failed; waits for the user.
    Failed,
}

/// One entry of the queue.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglQueueItemView {
    /// The record key: the session id or `<session>.overrun`.
    pub key: String,
    /// The task's text.
    pub description: String,
    /// The story, when known.
    pub story: Option<String>,
    /// The start, RFC 3339.
    pub start: String,
    /// Length in seconds.
    pub duration_seconds: u64,
    /// Whether Toggl marks it billable.
    pub is_billable: bool,
    /// Where it stands.
    pub state: TogglEntryStateView,
    /// Failed attempts so far.
    pub attempts: u32,
    /// Why the last attempt failed.
    pub last_error: Option<String>,
}

/// How a day of the Toggl week stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TogglDayStateView {
    /// Everything is in Toggl.
    Pushed,
    /// Something waits to be sent.
    Queued,
    /// Something failed.
    Failed,
    /// Nothing that day.
    Empty,
}

/// One day of the Toggl week.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglDayView {
    /// The day, `YYYY-MM-DD`.
    pub date: String,
    /// Seconds for Toggl that day, sent, queued and failed together.
    pub seconds: u64,
    /// Whether it is a flat FTE day.
    pub is_flat: bool,
    /// How it stands.
    pub state: TogglDayStateView,
}

/// The Toggl week of a workspace, or of one story.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglWeekView {
    /// Whether the feature is on for the workspace; off shows "Toggl not connected".
    pub is_enabled: bool,
    /// The seven days; empty when the feature is off.
    pub days: Vec<TogglDayView>,
}

/// What the flat job would do with one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FlatDayStateView {
    /// A run would create its entry.
    WillCreate,
    /// Queued.
    Queued,
    /// In Toggl.
    Sent,
    /// Failed.
    Failed,
    /// Leave.
    Leave,
    /// A public holiday.
    Holiday,
    /// Not a working day.
    NotWorkingDay,
}

/// The range and the days off a flat job covers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlatJobRangeView {
    /// The first day, `YYYY-MM-DD`.
    pub from: String,
    /// The last day, included.
    pub to: String,
    /// Days of leave.
    pub leave: Vec<String>,
    /// Public holidays.
    pub holidays: Vec<String>,
}

/// One day of the flat job's dry run.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlatDayView {
    /// The day, `YYYY-MM-DD`.
    pub date: String,
    /// What happens to it.
    pub state: FlatDayStateView,
    /// The entry's length in seconds; 0 without one.
    pub seconds: u64,
}

/// What putting a stopped session in the queue did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TogglEnqueueView {
    /// The feature is off for the workspace: nothing was looked at.
    FeatureOff,
    /// The workspace has no billing model, or an FTE one: nothing is sent.
    NotSent,
    /// Queued this many entries.
    Queued {
        /// Entries queued (two for a fixed-price overrun).
        entries: u32,
    },
    /// Queued before; nothing changed.
    AlreadyQueued,
    /// The session has not stopped.
    StillRunning,
}

/// What a drain did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TogglDrainView {
    /// Why nothing was sent at all, when so ("Toggl is not connected: …").
    pub not_connected: Option<String>,
    /// Entries now in Toggl.
    pub sent: u32,
    /// Entries to try again later.
    pub retrying: u32,
    /// Entries that failed for good.
    pub failed: u32,
    /// Entries left for later.
    pub waiting: u32,
    /// Toggl refused the token.
    pub is_unauthorized: bool,
}

/// The registry this build runs with: the Toggl and billing feature set.
///
/// # Errors
/// Never in practice: a registry refusal would be a contract violation.
pub fn feature_registry() -> Result<Registry, CoreError> {
    let mut registry = Registry::new();
    vaultday_feature_toggl::register(&mut registry)
        .map_err(|error| CoreError::AppState(error.to_string()))?;
    Ok(registry)
}

/// Reads the folder's `billing` block, defaults filled in.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] or [`CoreError::InvalidSettings`] as the manifest read fails.
#[uniffi::export]
pub fn read_workspace_billing(root: String) -> Result<BillingSettingsView, CoreError> {
    let workspace = open_settings(&root)?;
    Ok(billing_view(workspace.settings.billing.as_ref()))
}

/// Writes the folder's `billing` block: only the keys of the chosen model,
/// keeping keys this build does not know; `model: None` removes the block.
///
/// # Errors
/// [`CoreError::InvalidSettings`] for out-of-range values, file errors otherwise.
#[uniffi::export]
pub fn update_workspace_billing(
    root: String,
    billing: BillingSettingsView,
) -> Result<BillingSettingsView, CoreError> {
    let settings = update_settings_file(Path::new(&root), &PlainFileAccess, |settings| {
        let extra = settings
            .billing
            .take()
            .map(|block| block.extra)
            .unwrap_or_default();
        settings.billing = billing.model.map(|model| BillingSettings {
            extra,
            ..billing_block(model, &billing)
        });
    })
    .map_err(|error| CoreError::InvalidSettings(error.to_string()))?;
    Ok(billing_view(settings.billing.as_ref()))
}

#[uniffi::export]
impl AppStateHandle {
    /// The workspace's hourly rate, `None` until set.
    pub fn billing_rate(&self, workspace: String) -> Option<BillingRateView> {
        self.read_state(|state| state.billing_rate(&WorkspaceId::new(workspace)))
            .map(|rate| BillingRateView {
                amount_minor_per_hour: rate.amount_minor_per_hour,
                currency: rate.currency,
            })
    }

    /// Sets or clears the workspace's hourly rate.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for a malformed currency.
    pub fn set_billing_rate(
        &self,
        workspace: String,
        rate: Option<BillingRateView>,
    ) -> Result<(), CoreError> {
        let rate = rate.map(|rate| BillingRate {
            amount_minor_per_hour: rate.amount_minor_per_hour,
            currency: rate.currency.trim().to_uppercase(),
        });
        self.edit_state(|state| state.set_billing_rate(&WorkspaceId::new(workspace), rate.as_ref()))
    }

    /// Where the Toggl feature stands for the workspace.
    ///
    /// # Errors
    /// [`CoreError::SecretStore`] when the Keychain cannot be read.
    pub fn toggl_status(&self, workspace: String) -> Result<TogglStatusView, CoreError> {
        let workspace = WorkspaceId::new(workspace);
        let registry = feature_registry()?;
        let (enablement, is_live_allowed) =
            self.read_state(|state| (state.enablement(), allow_live(state)));
        let id = feature_id();
        Ok(TogglStatusView {
            is_on_for_app: enablement.is_on_for_app(&id),
            is_on_for_workspace: enablement.is_on_for_workspace(&workspace, &id),
            is_enabled: registry.is_enabled(&id, &enablement, &workspace),
            has_token: has_token(self.secrets(), &workspace).map_err(toggl_error)?,
            is_live_allowed,
        })
    }

    /// Switches the feature on for the workspace (and the app, which it
    /// needs), or off for the workspace alone.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_toggl_enabled(&self, workspace: String, is_on: bool) -> Result<(), CoreError> {
        let workspace = WorkspaceId::new(workspace);
        let id = feature_id();
        self.edit_state(|state| {
            if is_on {
                state.set_feature_enabled(&EnablementScope::App, &id, true)?;
            }
            state.set_feature_enabled(&EnablementScope::Workspace(workspace), &id, is_on)
        })
    }

    /// Stores the workspace's Toggl token in the Keychain, on this device
    /// only. It never reaches the app document, `settings.json` or the vault.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for a blank token, [`CoreError::SecretStore`] otherwise.
    pub fn store_toggl_token(&self, workspace: String, token: String) -> Result<(), CoreError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(CoreError::InvalidInput("The token is empty".to_owned()));
        }
        let id = token_id(&WorkspaceId::new(workspace));
        self.secrets()
            .put(SecretItem::new(
                id.kind,
                id.name,
                SecretBytes::new(token.as_bytes().to_vec()),
            ))
            .map_err(|error| CoreError::SecretStore(error.to_string()))
    }

    /// Removes the workspace's Toggl token from the Keychain.
    ///
    /// # Errors
    /// [`CoreError::SecretStore`] when the Keychain fails.
    pub fn delete_toggl_token(&self, workspace: String) -> Result<(), CoreError> {
        self.secrets()
            .delete(&token_id(&WorkspaceId::new(workspace)))
            .map_err(|error| CoreError::SecretStore(error.to_string()))
    }

    /// The workspace's Toggl mapping.
    pub fn toggl_mapping(&self, workspace: String) -> TogglMappingView {
        let mapping =
            self.read_state(|state| TogglMapping::read(state, &WorkspaceId::new(workspace)));
        mapping_view(&mapping)
    }

    /// Replaces the workspace's Toggl mapping, typed by hand or picked.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] while the feature is off for the workspace,
    /// [`CoreError::InvalidInput`] for a blank story.
    pub fn set_toggl_mapping(
        &self,
        workspace: String,
        mapping: TogglMappingView,
    ) -> Result<(), CoreError> {
        let feature = self.enabled_feature(&WorkspaceId::new(workspace))?;
        let mapping = TogglMapping {
            toggl_workspace_id: mapping.toggl_workspace_id,
            default_project_id: mapping.default_project_id,
            projects_by_story: mapping
                .story_projects
                .into_iter()
                .map(|entry| (entry.story, entry.project_id))
                .collect(),
        };
        let mut failure = None;
        self.edit_state(|state| {
            mapping.write(&feature, state).map_err(|error| {
                let message = error.to_string();
                failure = Some(error);
                AppStateError::InvalidSetting(message)
            })
        })
        .map_err(|error| failure.map_or(error, toggl_error))
    }

    /// The Toggl workspaces of the token's account, through the live client.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] "Toggl is not connected: …" unless the
    /// feature is on, a token is stored and `allow_live` is on.
    pub fn fetch_toggl_workspaces(
        &self,
        workspace: String,
    ) -> Result<Vec<TogglChoiceView>, CoreError> {
        let api = self.live_api(&WorkspaceId::new(workspace))?;
        Ok(api
            .workspaces()
            .map_err(toggl_error)?
            .into_iter()
            .map(|found| TogglChoiceView {
                id: found.id,
                name: found.name,
            })
            .collect())
    }

    /// The active projects of a Toggl workspace, through the live client.
    ///
    /// # Errors
    /// As [`AppStateHandle::fetch_toggl_workspaces`].
    pub fn fetch_toggl_projects(
        &self,
        workspace: String,
        toggl_workspace_id: u64,
    ) -> Result<Vec<TogglChoiceView>, CoreError> {
        let api = self.live_api(&WorkspaceId::new(workspace))?;
        Ok(api
            .projects(toggl_workspace_id)
            .map_err(toggl_error)?
            .into_iter()
            .filter(|project| project.is_active)
            .map(|project| TogglChoiceView {
                id: project.id,
                name: project.name,
            })
            .collect())
    }
}

/// Puts a stopped session of the workspace in the Toggl queue, as its
/// billing model says: hourly and fixed-price sessions become entries, FTE
/// ones nothing. Does nothing while the feature is off. Every stop surface
/// may call it: a session is queued once.
///
/// # Errors
/// [`CoreError::NotFound`] for an unknown session, [`CoreError::InvalidSettings`]
/// for a refused manifest, or the document refused the edit.
#[uniffi::export]
pub fn enqueue_toggl_session(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    workspace_id: String,
    session_id: String,
) -> Result<TogglEnqueueView, CoreError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    let Some(feature) = enabled(app, &workspace_id)? else {
        return Ok(TogglEnqueueView::FeatureOff);
    };
    let mut sync = lock(&state.sync);
    let Some(record) = sync.state().session(&SessionId::new(session_id.clone())) else {
        return Err(CoreError::NotFound(format!("no session {session_id}")));
    };
    enqueue_record(&feature, &mut sync, &workspace.workspace, &record)
}

/// Queues one stored session as the workspace's billing model says.
pub(crate) fn enqueue_record(
    feature: &TogglFeature,
    sync: &mut app_state::WorkspaceSync,
    workspace: &Workspace,
    record: &domain::session::SessionRecord,
) -> Result<TogglEnqueueView, CoreError> {
    // Read afresh: the Configure sheet may have changed the billing block
    // since the handle was opened.
    let settings = &current_settings(workspace)?;
    let toggl_settings = TogglSettings::from_entry(settings.features.get(feature_id().as_str()))
        .map_err(toggl_error)?;
    let task = match &record.target.task {
        Some(task) => task_facts(workspace, task.as_str())?,
        None => TaskFacts::default(),
    };
    let Some(policy) =
        BillingPolicy::for_workspace(settings.billing.as_ref(), task.is_billable, task.estimate)
    else {
        return Ok(TogglEnqueueView::NotSent);
    };
    let details = EntryDetails {
        description: task.text.unwrap_or_else(|| meeting_or_story(record)),
        story: record
            .target
            .story
            .as_ref()
            .map(|story| story.as_str().to_owned()),
        task: record
            .target
            .task
            .as_ref()
            .map(|task| task.as_str().to_owned()),
        tags: toggl_settings.tags,
    };
    let mut failure = None;
    let outcome = sync.edit(Instant::now(), |state| {
        feature
            .enqueue_session(state, record, &policy, &details)
            .map_err(|error| {
                let message = error.to_string();
                failure = Some(error);
                AppStateError::InvalidSetting(message)
            })
    });
    let outcome = outcome.map_err(|error| failure.map_or(error.into(), toggl_error))?;
    Ok(match outcome {
        EnqueueOutcome::Queued(entries) => TogglEnqueueView::Queued {
            entries: u32::try_from(entries).unwrap_or(u32::MAX),
        },
        EnqueueOutcome::AlreadyQueued => TogglEnqueueView::AlreadyQueued,
        EnqueueOutcome::NotSent => TogglEnqueueView::NotSent,
        EnqueueOutcome::StillRunning => TogglEnqueueView::StillRunning,
    })
}

/// The workspace's queue, by start; empty while the feature is off.
///
/// # Errors
/// Never in practice; see [`feature_registry`].
#[uniffi::export]
pub fn toggl_queue(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace_id: String,
) -> Result<Vec<TogglQueueItemView>, CoreError> {
    let Some(feature) = enabled(app, &WorkspaceId::new(workspace_id))? else {
        return Ok(Vec::new());
    };
    let sync = lock(&state.sync);
    Ok(feature
        .queue(sync.state())
        .into_iter()
        .map(|item| TogglQueueItemView {
            key: item.key,
            description: item.details.description,
            story: item.details.story,
            start: item.start.to_string(),
            duration_seconds: item.duration_secs,
            is_billable: item.is_billable,
            state: entry_state_view(item.state),
            attempts: item.attempts,
            last_error: item.last_error,
        })
        .collect())
}

/// The Toggl week from `week_start` (`YYYY-MM-DD`), for the workspace or,
/// with `story`, for one story; days are local to this Mac.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed day.
#[uniffi::export]
pub fn toggl_week(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace_id: String,
    week_start: String,
    story: Option<String>,
) -> Result<TogglWeekView, CoreError> {
    let week_start = parse_day(&week_start)?;
    let Some(feature) = enabled(app, &WorkspaceId::new(workspace_id))? else {
        return Ok(TogglWeekView {
            is_enabled: false,
            days: Vec::new(),
        });
    };
    let sync = lock(&state.sync);
    let day_of = |at: Timestamp| Some(at.to_zoned(TimeZone::system()).date());
    let days = feature
        .week(sync.state(), week_start, story.as_deref(), &day_of)
        .into_iter()
        .map(|day| TogglDayView {
            date: day.date.to_string(),
            seconds: day.sent_secs + day.queued_secs + day.failed_secs,
            is_flat: day.is_flat,
            state: match day.status {
                WeekDayStatus::Sent => TogglDayStateView::Pushed,
                WeekDayStatus::Queued => TogglDayStateView::Queued,
                WeekDayStatus::Failed => TogglDayStateView::Failed,
                WeekDayStatus::Empty => TogglDayStateView::Empty,
            },
        })
        .collect();
    Ok(TogglWeekView {
        is_enabled: true,
        days,
    })
}

/// The flat job's dry run over the range for an FTE workspace: Monday to
/// Friday, `flat_hours` from the billing block, leave and holidays left out.
/// Changes nothing and sends nothing.
///
/// # Errors
/// [`CoreError::Conflict`] while the feature is off, [`CoreError::InvalidInput`]
/// for a workspace that is not FTE or a bad range, [`CoreError::InvalidDate`]
/// for a malformed day.
#[uniffi::export]
pub fn preview_toggl_flat_job(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    workspace_id: String,
    range: FlatJobRangeView,
) -> Result<Vec<FlatDayView>, CoreError> {
    let feature = app.enabled_feature(&WorkspaceId::new(workspace_id))?;
    let input = flat_input(&current_settings(&workspace.workspace)?, &range)?;
    let sync = lock(&state.sync);
    Ok(feature
        .preview_flat_job(sync.state(), &input)
        .map_err(toggl_error)?
        .into_iter()
        .map(|day| FlatDayView {
            date: day.date.to_string(),
            state: flat_state_view(day.status),
            seconds: day.duration_secs,
        })
        .collect())
}

/// Sends this device's due queue entries and flat days through the live client. In this
/// build the live client is always refused (`allow_live` is off and nothing
/// switches it on), so the answer says why and nothing leaves the Mac.
///
/// # Errors
/// [`CoreError::SecretStore`] when the Keychain cannot be read, or the
/// document refused an edit.
#[uniffi::export]
pub fn drain_toggl_queue(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace_id: String,
) -> Result<TogglDrainView, CoreError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    let Some(feature) = enabled(app, &workspace_id)? else {
        return Ok(not_connected(
            "Toggl is not connected: the feature is off".to_owned(),
        ));
    };
    let api = match app.live_api(&workspace_id) {
        Ok(api) => api,
        Err(CoreError::Conflict(reason)) => return Ok(not_connected(reason)),
        Err(error) => return Err(error),
    };
    let mapping = app.read_state(|state| TogglMapping::read(state, &workspace_id));
    let mut sync = lock(&state.sync);
    let now = Timestamp::now();
    let sessions = feature
        .drain_queue(&mut sync, &api, &mapping, now)
        .map_err(toggl_error)?;
    let flat = feature
        .drain_flat(&mut sync, &api, &mapping, now)
        .map_err(toggl_error)?;
    Ok(drain_view(&DrainReport {
        sent: sessions.sent + flat.sent,
        retrying: sessions.retrying + flat.retrying,
        failed: sessions.failed + flat.failed,
        waiting: sessions.waiting + flat.waiting,
        is_unauthorized: sessions.is_unauthorized || flat.is_unauthorized,
        is_unmapped: sessions.is_unmapped || flat.is_unmapped,
    }))
}

impl AppStateHandle {
    fn enabled_feature(&self, workspace: &WorkspaceId) -> Result<TogglFeature, CoreError> {
        enabled(self, workspace)?
            .ok_or_else(|| CoreError::Conflict("Toggl is off for this workspace".to_owned()))
    }

    /// The live client, or [`CoreError::Conflict`] saying why it is refused.
    fn live_api(&self, workspace: &WorkspaceId) -> Result<HttpTogglApi, CoreError> {
        let registry = feature_registry()?;
        let secrets: Arc<dyn SecretStore> = Arc::new(self.secrets().clone());
        self.read_state(|state| HttpTogglApi::live(&registry, state, workspace, secrets))
            .map_err(toggl_error)
    }
}

pub(crate) fn enabled(
    app: &AppStateHandle,
    workspace: &WorkspaceId,
) -> Result<Option<TogglFeature>, CoreError> {
    let registry = feature_registry()?;
    let enablement = app.read_state(app_state::AppState::enablement);
    Ok(TogglFeature::for_workspace(
        &registry,
        &enablement,
        workspace,
    ))
}

#[derive(Debug, Default)]
pub(crate) struct TaskFacts {
    pub(crate) text: Option<String>,
    pub(crate) estimate: Option<Duration>,
    pub(crate) is_billable: Option<bool>,
}

/// The task's text, estimate and `[billable:: …]` flag, read from the vault.
pub(crate) fn task_facts(workspace: &Workspace, task: &str) -> Result<TaskFacts, CoreError> {
    let vault = ops::open_vault(workspace, &PlainFileAccess)?;
    let Some(found) = vault.task_by_id(task) else {
        return Ok(TaskFacts::default());
    };
    Ok(TaskFacts {
        text: Some(found.task.text.clone()),
        estimate: found
            .task
            .field("estimate")
            .and_then(parse_estimate)
            .map(|estimate| estimate.as_duration()),
        is_billable: found.task.field("billable").and_then(parse_billable_flag),
    })
}

pub(crate) fn meeting_or_story(record: &domain::session::SessionRecord) -> String {
    match &record.source {
        domain::session::SessionSource::Meeting { title } => title.clone(),
        domain::session::SessionSource::Timer => record
            .target
            .story
            .as_ref()
            .map(|story| story.as_str().to_owned())
            .unwrap_or_default(),
    }
}

fn flat_input(
    settings: &WorkspaceSettings,
    range: &FlatJobRangeView,
) -> Result<FlatJobInput, CoreError> {
    let Some(billing) = settings
        .billing
        .as_ref()
        .filter(|billing| billing.model == BillingModel::Fte)
    else {
        return Err(CoreError::InvalidInput(
            "The flat week is for FTE workspaces".to_owned(),
        ));
    };
    let tags = TogglSettings::from_entry(settings.features.get(feature_id().as_str()))
        .map_err(toggl_error)?
        .tags;
    let mut input = FlatJobInput::weekdays(
        parse_day(&range.from)?,
        parse_day(&range.to)?,
        billing.flat_hours_or_default(),
    );
    input.leave = range
        .leave
        .iter()
        .map(|day| parse_day(day))
        .collect::<Result<_, _>>()?;
    input.holidays = range
        .holidays
        .iter()
        .map(|day| parse_day(day))
        .collect::<Result<_, _>>()?;
    input.tags = tags;
    Ok(input)
}

pub(crate) fn current_settings(workspace: &Workspace) -> Result<WorkspaceSettings, CoreError> {
    Ok(open_settings(&workspace.root.to_string_lossy())?.settings)
}

fn open_settings(root: &str) -> Result<Workspace, CoreError> {
    Workspace::open(Path::new(root), &PlainFileAccess).map_err(|error| match error {
        vault_base::WorkspaceError::NotAWorkspace(_) => CoreError::NotAWorkspace(error.to_string()),
        other => CoreError::InvalidSettings(other.to_string()),
    })
}

fn billing_view(billing: Option<&BillingSettings>) -> BillingSettingsView {
    let defaults = BillingSettings::new(BillingModel::Hourly);
    let block = billing.unwrap_or(&defaults);
    BillingSettingsView {
        model: billing.map(|billing| match billing.model {
            BillingModel::Hourly => BillingModelView::Hourly,
            BillingModel::Fixed => BillingModelView::Fixed,
            BillingModel::Fte => BillingModelView::Fte,
        }),
        rounding_minutes: block.rounding_minutes_or_default(),
        is_billable_by_default: block.billable_default_or_default(),
        flat_hours: block.flat_hours_or_default(),
    }
}

fn billing_block(model: BillingModelView, view: &BillingSettingsView) -> BillingSettings {
    match model {
        BillingModelView::Hourly => BillingSettings {
            rounding_minutes: Some(view.rounding_minutes),
            billable_default: Some(view.is_billable_by_default),
            ..BillingSettings::new(BillingModel::Hourly)
        },
        BillingModelView::Fixed => BillingSettings::new(BillingModel::Fixed),
        BillingModelView::Fte => BillingSettings {
            flat_hours: Some(view.flat_hours),
            ..BillingSettings::new(BillingModel::Fte)
        },
    }
}

fn mapping_view(mapping: &TogglMapping) -> TogglMappingView {
    TogglMappingView {
        toggl_workspace_id: mapping.toggl_workspace_id,
        default_project_id: mapping.default_project_id,
        story_projects: mapping
            .projects_by_story
            .iter()
            .map(|(story, project)| StoryProjectView {
                story: story.clone(),
                project_id: *project,
            })
            .collect(),
    }
}

fn entry_state_view(state: QueueState) -> TogglEntryStateView {
    match state {
        QueueState::Queued => TogglEntryStateView::Queued,
        QueueState::Sent => TogglEntryStateView::Sent,
        QueueState::Failed => TogglEntryStateView::Failed,
    }
}

fn flat_state_view(status: FlatDayStatus) -> FlatDayStateView {
    match status {
        FlatDayStatus::WillCreate => FlatDayStateView::WillCreate,
        FlatDayStatus::Queued => FlatDayStateView::Queued,
        FlatDayStatus::Sent => FlatDayStateView::Sent,
        FlatDayStatus::Failed => FlatDayStateView::Failed,
        FlatDayStatus::Leave => FlatDayStateView::Leave,
        FlatDayStatus::Holiday => FlatDayStateView::Holiday,
        FlatDayStatus::NotWorkingDay => FlatDayStateView::NotWorkingDay,
    }
}

fn drain_view(report: &DrainReport) -> TogglDrainView {
    TogglDrainView {
        not_connected: report
            .is_unmapped
            .then(|| "Toggl is not connected: no Toggl workspace is mapped".to_owned()),
        sent: report.sent,
        retrying: report.retrying,
        failed: report.failed,
        waiting: report.waiting,
        is_unauthorized: report.is_unauthorized,
    }
}

fn not_connected(reason: String) -> TogglDrainView {
    TogglDrainView {
        not_connected: Some(reason),
        sent: 0,
        retrying: 0,
        failed: 0,
        waiting: 0,
        is_unauthorized: false,
    }
}

fn parse_day(text: &str) -> Result<Date, CoreError> {
    text.parse()
        .map_err(|_| CoreError::InvalidDate(format!("{text} is not a date; expected YYYY-MM-DD")))
}

pub(crate) fn toggl_error(error: TogglError) -> CoreError {
    let message = error.to_string();
    match error {
        TogglError::LiveRefused(_) | TogglError::NotLoopback(_) => CoreError::Conflict(message),
        TogglError::InvalidInput(_) => CoreError::InvalidInput(message),
        TogglError::InvalidSettings(_) => CoreError::InvalidSettings(message),
        TogglError::SecretStore(_) => CoreError::SecretStore(message),
        TogglError::AppState(error) => error.into(),
        _ => CoreError::AppState(message),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::app_state::open_app_state;
    use crate::app_state::tests::Keychain;
    use crate::files::plain_sync_files;
    use crate::home::tests::{open, sandbox, state};

    const WORKSPACE: &str = "mock-studio";
    const TOKEN: &str = "synthetic-toggl-token-0001";

    fn app(dir: &Path) -> Arc<AppStateHandle> {
        open_app_state(
            plain_sync_files(),
            dir.join("app").to_string_lossy().into_owned(),
            Arc::new(Keychain::default()),
        )
        .unwrap()
    }

    fn root_text(root: &Path) -> String {
        root.to_string_lossy().into_owned()
    }

    fn range(to: &str, leave: Vec<String>, holidays: Vec<String>) -> FlatJobRangeView {
        FlatJobRangeView {
            from: "2026-10-19".to_owned(),
            to: to.to_owned(),
            leave,
            holidays,
        }
    }

    fn hourly() -> BillingSettingsView {
        BillingSettingsView {
            model: Some(BillingModelView::Hourly),
            rounding_minutes: 6,
            is_billable_by_default: true,
            flat_hours: 8.0,
        }
    }

    #[test]
    fn the_billing_block_round_trips_and_keeps_unknown_keys() {
        let vault = sandbox();
        let root = root_text(vault.path());
        let unset = read_workspace_billing(root.clone()).unwrap();
        assert_eq!(unset.model, None);
        assert_eq!((unset.rounding_minutes, unset.flat_hours), (15, 8.0));

        let written = update_workspace_billing(root.clone(), hourly()).unwrap();
        assert_eq!(written.model, Some(BillingModelView::Hourly));
        let text = fs::read_to_string(vault.path().join(".vaultday/settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json["billing"],
            serde_json::json!({"model": "hourly", "rounding_minutes": 6, "billable_default": true})
        );
        assert_eq!(json["x_future_setting"]["kept"], true);

        let fte = update_workspace_billing(
            root.clone(),
            BillingSettingsView {
                model: Some(BillingModelView::Fte),
                flat_hours: 7.5,
                ..hourly()
            },
        )
        .unwrap();
        assert_eq!(
            (fte.model, fte.flat_hours),
            (Some(BillingModelView::Fte), 7.5)
        );
        let json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(vault.path().join(".vaultday/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            json["billing"],
            serde_json::json!({"model": "fte", "flat_hours": 7.5})
        );

        assert!(matches!(
            update_workspace_billing(
                root.clone(),
                BillingSettingsView {
                    rounding_minutes: 0,
                    ..hourly()
                }
            ),
            Err(CoreError::InvalidSettings(_))
        ));
        assert_eq!(
            read_workspace_billing(root.clone()).unwrap().model,
            Some(BillingModelView::Fte),
            "a refused write leaves the file alone"
        );
        let cleared = update_workspace_billing(
            root,
            BillingSettingsView {
                model: None,
                ..hourly()
            },
        )
        .unwrap();
        assert_eq!(cleared.model, None);
    }

    #[test]
    fn the_token_lives_in_the_keychain_only() {
        let vault = sandbox();
        let app = app(vault.path());
        assert!(!app.toggl_status(WORKSPACE.to_owned()).unwrap().has_token);
        assert!(matches!(
            app.store_toggl_token(WORKSPACE.to_owned(), "  ".to_owned()),
            Err(CoreError::InvalidInput(_))
        ));
        app.store_toggl_token(WORKSPACE.to_owned(), TOKEN.to_owned())
            .unwrap();
        app.set_toggl_enabled(WORKSPACE.to_owned(), true).unwrap();
        app.set_billing_rate(
            WORKSPACE.to_owned(),
            Some(BillingRateView {
                amount_minor_per_hour: 9_500,
                currency: "eur".to_owned(),
            }),
        )
        .unwrap();
        let status = app.toggl_status(WORKSPACE.to_owned()).unwrap();
        assert!(status.has_token && status.is_enabled && !status.is_live_allowed);
        assert_eq!(
            app.billing_rate(WORKSPACE.to_owned()),
            Some(BillingRateView {
                amount_minor_per_hour: 9_500,
                currency: "EUR".to_owned()
            })
        );
        let snapshot = app.save().unwrap();
        let contains = |bytes: &[u8]| {
            bytes
                .windows(TOKEN.len())
                .any(|window| window == TOKEN.as_bytes())
        };
        assert!(!contains(&fs::read(snapshot).unwrap()));
        assert!(!contains(
            &fs::read(vault.path().join(".vaultday/settings.json")).unwrap()
        ));
        app.delete_toggl_token(WORKSPACE.to_owned()).unwrap();
        assert!(!app.toggl_status(WORKSPACE.to_owned()).unwrap().has_token);
    }

    #[test]
    fn with_the_feature_off_nothing_of_it_runs() {
        let vault = sandbox();
        let app = app(vault.path());
        let workspace = open(vault.path());
        let state = state(vault.path());
        update_workspace_billing(root_text(vault.path()), hourly()).unwrap();
        let session = state
            .start_session_minutes_ago(Some("task-2".to_owned()), None, 30)
            .unwrap();
        state.stop_and_log_session(&workspace).unwrap();
        assert_eq!(
            enqueue_toggl_session(&app, &state, &workspace, WORKSPACE.to_owned(), session.id)
                .unwrap(),
            TogglEnqueueView::FeatureOff
        );
        assert!(
            toggl_queue(&app, &state, WORKSPACE.to_owned())
                .unwrap()
                .is_empty()
        );
        let week = toggl_week(
            &app,
            &state,
            WORKSPACE.to_owned(),
            "2026-10-19".to_owned(),
            None,
        )
        .unwrap();
        assert!(!week.is_enabled && week.days.is_empty());
        assert!(matches!(
            preview_toggl_flat_job(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                range("2026-10-23", Vec::new(), Vec::new()),
            ),
            Err(CoreError::Conflict(_))
        ));
        assert!(matches!(
            app.set_toggl_mapping(WORKSPACE.to_owned(), TogglMappingView::default()),
            Err(CoreError::Conflict(_))
        ));
        let drained = drain_toggl_queue(&app, &state, WORKSPACE.to_owned()).unwrap();
        assert!(drained.not_connected.is_some());
        // The workspace switch alone is not enough: the app switch is off.
        app.edit_state(|state| {
            state.set_feature_enabled(
                &EnablementScope::Workspace(WorkspaceId::new(WORKSPACE)),
                &feature_id(),
                true,
            )
        })
        .unwrap();
        assert!(!app.toggl_status(WORKSPACE.to_owned()).unwrap().is_enabled);
    }

    #[test]
    fn a_stopped_session_is_queued_once_and_the_live_client_stays_refused() {
        let vault = sandbox();
        let app = app(vault.path());
        let state = state(vault.path());
        // Opened before the billing block is written: the queue reads it afresh.
        let workspace = open(vault.path());
        update_workspace_billing(root_text(vault.path()), hourly()).unwrap();
        app.set_toggl_enabled(WORKSPACE.to_owned(), true).unwrap();
        app.store_toggl_token(WORKSPACE.to_owned(), TOKEN.to_owned())
            .unwrap();
        let session = state
            .start_session_minutes_ago(Some("task-2".to_owned()), Some("WEB-142".to_owned()), 30)
            .unwrap();
        assert_eq!(
            enqueue_toggl_session(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                session.id.clone()
            )
            .unwrap(),
            TogglEnqueueView::StillRunning
        );
        state.stop_and_log_session(&workspace).unwrap();
        assert_eq!(
            enqueue_toggl_session(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                session.id.clone()
            )
            .unwrap(),
            TogglEnqueueView::Queued { entries: 1 }
        );
        assert_eq!(
            enqueue_toggl_session(&app, &state, &workspace, WORKSPACE.to_owned(), session.id)
                .unwrap(),
            TogglEnqueueView::AlreadyQueued
        );
        let queue = toggl_queue(&app, &state, WORKSPACE.to_owned()).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue[0].description,
            "Wire the payment form to the gateway sandbox"
        );
        assert_eq!(queue[0].story.as_deref(), Some("WEB-142"));
        assert_eq!(queue[0].duration_seconds % 360, 0, "rounded to 6 minutes");
        assert_eq!(queue[0].state, TogglEntryStateView::Queued);

        let started: Timestamp = queue[0].start.parse().unwrap();
        let day = started.to_zoned(TimeZone::system()).date();
        let monday = if day.weekday() == jiff::civil::Weekday::Monday {
            day
        } else {
            day.nth_weekday(-1, jiff::civil::Weekday::Monday).unwrap()
        };
        let week =
            toggl_week(&app, &state, WORKSPACE.to_owned(), monday.to_string(), None).unwrap();
        assert!(week.is_enabled);
        assert_eq!(week.days.len(), 7);
        assert!(
            week.days
                .iter()
                .any(|day| day.state == TogglDayStateView::Queued)
        );
        let other_story = toggl_week(
            &app,
            &state,
            WORKSPACE.to_owned(),
            monday.to_string(),
            Some("ACME-1".to_owned()),
        )
        .unwrap();
        assert!(
            other_story
                .days
                .iter()
                .all(|day| day.state == TogglDayStateView::Empty)
        );

        // The feature is on and a token is stored, but allow_live is off:
        // nothing leaves this machine.
        let drained = drain_toggl_queue(&app, &state, WORKSPACE.to_owned()).unwrap();
        assert_eq!(
            drained.not_connected.as_deref(),
            Some("Toggl is not connected: allow_live is off")
        );
        assert!(matches!(
            app.fetch_toggl_workspaces(WORKSPACE.to_owned()),
            Err(CoreError::Conflict(message)) if message.contains("allow_live is off")
        ));
        assert_eq!(
            toggl_queue(&app, &state, WORKSPACE.to_owned()).unwrap()[0].attempts,
            0
        );
    }

    #[test]
    fn the_mapping_round_trips_and_the_flat_dry_run_reads_the_fte_block() {
        let vault = sandbox();
        let app = app(vault.path());
        let state = state(vault.path());
        app.set_toggl_enabled(WORKSPACE.to_owned(), true).unwrap();
        let mapping = TogglMappingView {
            toggl_workspace_id: Some(4242),
            default_project_id: Some(502),
            story_projects: vec![StoryProjectView {
                story: "WEB-142".to_owned(),
                project_id: 501,
            }],
        };
        app.set_toggl_mapping(WORKSPACE.to_owned(), mapping.clone())
            .unwrap();
        assert_eq!(app.toggl_mapping(WORKSPACE.to_owned()), mapping);

        let workspace = open(vault.path());
        assert!(matches!(
            preview_toggl_flat_job(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                range("2026-10-23", Vec::new(), Vec::new()),
            ),
            Err(CoreError::InvalidInput(_))
        ));
        update_workspace_billing(
            root_text(vault.path()),
            BillingSettingsView {
                model: Some(BillingModelView::Fte),
                flat_hours: 8.0,
                ..hourly()
            },
        )
        .unwrap();
        let workspace = open(vault.path());
        let preview = preview_toggl_flat_job(
            &app,
            &state,
            &workspace,
            WORKSPACE.to_owned(),
            range(
                "2026-10-25",
                vec!["2026-10-20".to_owned()],
                vec!["2026-10-21".to_owned()],
            ),
        )
        .unwrap();
        let states: Vec<FlatDayStateView> = preview.iter().map(|day| day.state).collect();
        assert_eq!(
            states,
            [
                FlatDayStateView::WillCreate,
                FlatDayStateView::Leave,
                FlatDayStateView::Holiday,
                FlatDayStateView::WillCreate,
                FlatDayStateView::WillCreate,
                FlatDayStateView::NotWorkingDay,
                FlatDayStateView::NotWorkingDay,
            ]
        );
        assert_eq!(preview[0].seconds, 8 * 3600);
        assert!(
            toggl_queue(&app, &state, WORKSPACE.to_owned())
                .unwrap()
                .is_empty()
        );
    }
}
