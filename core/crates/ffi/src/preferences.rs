//! The day-rhythm, Settings and AI choices of the app-level document as
//! plain records and enums. The AI choices and the rituals are stored and
//! inert in the base version; provider keys cross here as metadata only, the
//! key itself goes to the platform secret store.

use std::collections::BTreeMap;

use app_state::{
    AI_TIER_EVERYWHERE, AgentChanges, AiScopeChoice, AiTier, BreakPattern, DataRetention,
    FeatureScopeMode, ListPosition, OtherAppsAccess, OverrunWarning, ProviderKeyRecord,
    ProviderKeyScope, ProviderKind, SecurityMode, ShortcutCaps, WorkingHours, WorkingHoursKind,
    WorkspaceSecurity,
};
use domain::workspace::WorkspaceId;
use jiff::civil::{Time, Weekday};

use crate::CoreError;
use crate::app_state::AppStateHandle;
use crate::onboarding::FeatureScopeModeView;

/// Focus and break length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BreakPatternView {
    /// No breaks.
    None,
    /// Fifty minutes of focus, ten of break.
    FiftyTen,
    /// Twenty-five minutes of focus, five of break.
    TwentyFiveFive,
}

/// Where a task lands in a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ListPositionView {
    /// At the top.
    Top,
    /// At the bottom.
    Bottom,
}

/// When a day that runs over warns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OverrunWarningView {
    /// When the plan reaches into free time.
    IntoFreeTime,
    /// Never.
    Never,
}

/// How the shortcut caps show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ShortcutCapsView {
    /// They animate.
    Animated,
    /// They change at once.
    Instant,
}

/// Fixed times or a daily target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WorkingHoursKindView {
    /// Set start and end.
    Fixed,
    /// Planned around the calendar.
    Flexible,
}

/// A day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum WeekdayView {
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday.
    Sunday,
}

/// The working hours of the Work sphere; times are minutes after midnight.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkingHoursView {
    /// Fixed or flexible.
    pub kind: WorkingHoursKindView,
    /// When work starts.
    pub start_minute: u32,
    /// When work ends.
    pub end_minute: u32,
    /// The days worked.
    pub days: Vec<WeekdayView>,
}

/// How much an AI feature may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AiTierView {
    /// One item at a time.
    Glance,
    /// Related tasks, notes and mail.
    Insight,
}

/// The tier chosen for one target of an AI scope.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AiTierEntryView {
    /// A sphere name, a workspace id, or `*` for the whole scope.
    pub target: String,
    /// The tier.
    pub tier: AiTierView,
}

/// Where an AI level or feature is on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AiScopeView {
    /// The switch.
    pub mode: FeatureScopeModeView,
    /// Sphere names or workspace ids, by mode.
    pub targets: Vec<String>,
    /// The tier per target.
    pub tiers: Vec<AiTierEntryView>,
}

/// What a cloud provider may keep of the requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DataRetentionView {
    /// Nothing kept.
    Disabled,
    /// Safety retention allowed.
    SafetyRetention,
    /// Only zero-retention providers.
    ZeroRetention,
}

/// What other AI apps may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OtherAppsAccessView {
    /// Nothing.
    Off,
    /// Read.
    ReadOnly,
    /// Read and write.
    ReadAndWrite,
}

/// What an agent may change without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentChangesView {
    /// Ask every time.
    AskEveryTime,
    /// Ask before deleting.
    AskBeforeDeleting,
    /// Never ask.
    NeverAsk,
}

/// Which provider a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProviderKindView {
    /// A local or self-hosted model.
    Local,
    /// OpenRouter.
    OpenRouter,
    /// Anthropic.
    Claude,
    /// OpenAI.
    OpenAi,
    /// Google Gemini.
    Gemini,
    /// An OpenAI-compatible endpoint.
    Other,
}

/// Where a provider key is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProviderKeyScopeView {
    /// Every workspace without its own key.
    Default,
    /// The chosen spheres.
    PerSphere,
    /// The chosen workspaces.
    PerWorkspace,
}

/// What the document records of a provider key; the key itself is in the
/// secret store under the same id.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProviderKeyView {
    /// The key's id, which names its secret.
    pub id: String,
    /// The provider.
    pub provider: ProviderKindView,
    /// What the user calls it.
    pub name: String,
    /// Where it is used.
    pub scope: ProviderKeyScopeView,
    /// Sphere names or workspace ids, by scope.
    pub targets: Vec<String>,
    /// The model for AI Insight.
    pub insight_model: Option<String>,
    /// The model for AI Glance jobs too heavy for this Mac.
    pub glance_model: Option<String>,
    /// The endpoint of a local or other provider.
    pub endpoint: Option<String>,
}

/// A workspace's security mode; stored now, enforced from a later release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SecurityModeView {
    /// No mode.
    Off,
    /// Enhanced.
    Enhanced,
    /// Strict.
    Strict,
}

/// A workspace's security fields; inert in the base version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct WorkspaceSecurityView {
    /// The mode.
    pub mode: SecurityModeView,
    /// Whether anything leaving the device asks first.
    pub always_ask_before_sending: bool,
}

/// The tier key of a whole scope.
#[uniffi::export]
#[must_use]
pub fn ai_tier_everywhere() -> String {
    AI_TIER_EVERYWHERE.to_owned()
}

#[uniffi::export]
impl AppStateHandle {
    /// The break pattern.
    pub fn break_pattern(&self) -> BreakPatternView {
        match self.read_state(|state| state.break_pattern()) {
            BreakPattern::None => BreakPatternView::None,
            BreakPattern::FiftyTen => BreakPatternView::FiftyTen,
            BreakPattern::TwentyFiveFive => BreakPatternView::TwentyFiveFive,
        }
    }

    /// Sets the break pattern.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_break_pattern(&self, pattern: BreakPatternView) -> Result<(), CoreError> {
        let pattern = match pattern {
            BreakPatternView::None => BreakPattern::None,
            BreakPatternView::FiftyTen => BreakPattern::FiftyTen,
            BreakPatternView::TwentyFiveFive => BreakPattern::TwentyFiveFive,
        };
        self.edit_state(|state| state.set_break_pattern(pattern))
    }

    /// Where unfinished tasks land at roll-over.
    pub fn rollover_position(&self) -> ListPositionView {
        list_position_view(self.read_state(|state| state.rollover_position()))
    }

    /// Sets where unfinished tasks land at roll-over.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_rollover_position(&self, position: ListPositionView) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_rollover_position(list_position(position)))
    }

    /// Where a new task lands in a list.
    pub fn new_task_position(&self) -> ListPositionView {
        list_position_view(self.read_state(|state| state.new_task_position()))
    }

    /// Sets where a new task lands in a list.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_new_task_position(&self, position: ListPositionView) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_new_task_position(list_position(position)))
    }

    /// When a day that runs over warns.
    pub fn overrun_warning(&self) -> OverrunWarningView {
        match self.read_state(|state| state.overrun_warning()) {
            OverrunWarning::IntoFreeTime => OverrunWarningView::IntoFreeTime,
            OverrunWarning::Never => OverrunWarningView::Never,
        }
    }

    /// Sets when a day that runs over warns.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_overrun_warning(&self, warning: OverrunWarningView) -> Result<(), CoreError> {
        let warning = match warning {
            OverrunWarningView::IntoFreeTime => OverrunWarning::IntoFreeTime,
            OverrunWarningView::Never => OverrunWarning::Never,
        };
        self.edit_state(|state| state.set_overrun_warning(warning))
    }

    /// How the shortcut caps show.
    pub fn shortcut_caps(&self) -> ShortcutCapsView {
        match self.read_state(|state| state.shortcut_caps()) {
            ShortcutCaps::Animated => ShortcutCapsView::Animated,
            ShortcutCaps::Instant => ShortcutCapsView::Instant,
        }
    }

    /// Sets how the shortcut caps show.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_shortcut_caps(&self, caps: ShortcutCapsView) -> Result<(), CoreError> {
        let caps = match caps {
            ShortcutCapsView::Animated => ShortcutCaps::Animated,
            ShortcutCapsView::Instant => ShortcutCaps::Instant,
        };
        self.edit_state(|state| state.set_shortcut_caps(caps))
    }

    /// The planned work a day may hold before planning warns, in minutes.
    pub fn workload_threshold_minutes(&self) -> u32 {
        let minutes = self.read_state(|state| state.workload_threshold().as_secs() / 60);
        u32::try_from(minutes).unwrap_or(u32::MAX)
    }

    /// Sets the workload threshold, from one minute to a full day.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] outside 1 to 1440 minutes; the document
    /// refused the edit.
    pub fn set_workload_threshold_minutes(&self, minutes: u32) -> Result<(), CoreError> {
        if !(1..=24 * 60).contains(&minutes) {
            return Err(CoreError::InvalidInput(format!(
                "{minutes} minutes is not a workload threshold: 1 to 1440"
            )));
        }
        let threshold = std::time::Duration::from_secs(u64::from(minutes) * 60);
        self.edit_state(|state| state.set_workload_threshold(threshold))
    }

    /// How long a session may run before the long-timer check asks, in minutes.
    pub fn long_timer_limit_minutes(&self) -> u32 {
        let minutes = self.read_state(|state| state.long_timer_limit().as_secs() / 60);
        u32::try_from(minutes).unwrap_or(u32::MAX)
    }

    /// Sets the long-timer limit, from one minute to a full day.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] outside 1 to 1440 minutes; the document
    /// refused the edit.
    pub fn set_long_timer_limit_minutes(&self, minutes: u32) -> Result<(), CoreError> {
        if !(1..=24 * 60).contains(&minutes) {
            return Err(CoreError::InvalidInput(format!(
                "{minutes} minutes is not a long-timer limit: 1 to 1440"
            )));
        }
        let limit = std::time::Duration::from_secs(u64::from(minutes) * 60);
        self.edit_state(|state| state.set_long_timer_limit(limit))
    }

    /// When a ritual runs, as minutes after midnight.
    pub fn ritual_minute(&self, ritual: String) -> Option<u32> {
        self.read_state(|state| state.ritual_times().get(&ritual).copied())
            .map(minute_of_day)
    }

    /// Sets when a ritual runs, as minutes after midnight; `None` clears it.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for a time beyond 23:59; the document
    /// refused the edit.
    pub fn set_ritual_minute(&self, ritual: String, minute: Option<u32>) -> Result<(), CoreError> {
        let time = minute.map(time_from_minute).transpose()?;
        self.edit_state(|state| state.set_ritual_time(&ritual, time))
    }

    /// A workspace's security fields; off until set.
    pub fn security(&self, workspace: String) -> WorkspaceSecurityView {
        let workspace = WorkspaceId::new(workspace);
        let security = self.read_state(|state| state.security(&workspace));
        WorkspaceSecurityView {
            mode: match security.mode {
                SecurityMode::Off => SecurityModeView::Off,
                SecurityMode::Enhanced => SecurityModeView::Enhanced,
                SecurityMode::Strict => SecurityModeView::Strict,
            },
            always_ask_before_sending: security.always_ask_before_sending,
        }
    }

    /// Stores a workspace's security fields. Nothing enforces them yet.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_security(
        &self,
        workspace: String,
        security: WorkspaceSecurityView,
    ) -> Result<(), CoreError> {
        let workspace = WorkspaceId::new(workspace);
        let security = WorkspaceSecurity {
            mode: match security.mode {
                SecurityModeView::Off => SecurityMode::Off,
                SecurityModeView::Enhanced => SecurityMode::Enhanced,
                SecurityModeView::Strict => SecurityMode::Strict,
            },
            always_ask_before_sending: security.always_ask_before_sending,
        };
        self.edit_state(|state| state.set_security(&workspace, &security))
    }

    /// The weekday a weekly ritual runs on.
    pub fn ritual_day(&self, ritual: String) -> Option<WeekdayView> {
        self.read_state(|state| state.ritual_day(&ritual))
            .map(weekday_view)
    }

    /// Sets the weekday a weekly ritual runs on; `None` clears it.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for an unusable ritual name; the document
    /// refused the edit.
    pub fn set_ritual_day(
        &self,
        ritual: String,
        day: Option<WeekdayView>,
    ) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_ritual_day(&ritual, day.map(weekday)))
    }

    /// The working hours.
    pub fn working_hours(&self) -> WorkingHoursView {
        let hours = self.read_state(|state| state.working_hours());
        WorkingHoursView {
            kind: match hours.kind {
                WorkingHoursKind::Fixed => WorkingHoursKindView::Fixed,
                WorkingHoursKind::Flexible => WorkingHoursKindView::Flexible,
            },
            start_minute: minute_of_day(hours.start),
            end_minute: minute_of_day(hours.end),
            days: hours.days.into_iter().map(weekday_view).collect(),
        }
    }

    /// Stores the working hours.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for a time beyond 23:59; the document
    /// refused the edit.
    pub fn set_working_hours(&self, hours: WorkingHoursView) -> Result<(), CoreError> {
        let hours = WorkingHours {
            kind: match hours.kind {
                WorkingHoursKindView::Fixed => WorkingHoursKind::Fixed,
                WorkingHoursKindView::Flexible => WorkingHoursKind::Flexible,
            },
            start: time_from_minute(hours.start_minute)?,
            end: time_from_minute(hours.end_minute)?,
            days: hours.days.into_iter().map(weekday).collect(),
        };
        self.edit_state(|state| state.set_working_hours(&hours))
    }

    /// Whether Life tasks are planned into the time after work.
    pub fn plans_life_after_work(&self) -> bool {
        self.read_state(|state| state.plans_life_after_work())
    }

    /// Sets whether Life tasks are planned into the time after work.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_plans_life_after_work(&self, is_on: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_plans_life_after_work(is_on))
    }

    /// Whether a reminder rings at its time.
    pub fn reminder_times_ring(&self) -> bool {
        self.read_state(|state| state.reminder_times_ring())
    }

    /// Sets whether a reminder rings at its time.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_reminder_times_ring(&self, is_on: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_reminder_times_ring(is_on))
    }

    /// Where an AI level or feature is on, by name.
    pub fn ai_scope(&self, name: String) -> AiScopeView {
        let choice = self.read_state(|state| state.ai_scope(&name));
        AiScopeView {
            mode: match choice.mode {
                FeatureScopeMode::Off => FeatureScopeModeView::Off,
                FeatureScopeMode::Everywhere => FeatureScopeModeView::Everywhere,
                FeatureScopeMode::PerSphere => FeatureScopeModeView::PerSphere,
                FeatureScopeMode::PerWorkspace => FeatureScopeModeView::PerWorkspace,
            },
            targets: choice.targets,
            tiers: choice
                .tiers
                .into_iter()
                .map(|(target, tier)| AiTierEntryView {
                    target,
                    tier: match tier {
                        AiTier::Glance => AiTierView::Glance,
                        AiTier::Insight => AiTierView::Insight,
                    },
                })
                .collect(),
        }
    }

    /// Stores where an AI level or feature is on, by name.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for an unusable name or target; the
    /// document refused the edit.
    pub fn set_ai_scope(&self, name: String, scope: AiScopeView) -> Result<(), CoreError> {
        let choice = AiScopeChoice {
            mode: match scope.mode {
                FeatureScopeModeView::Off => FeatureScopeMode::Off,
                FeatureScopeModeView::Everywhere => FeatureScopeMode::Everywhere,
                FeatureScopeModeView::PerSphere => FeatureScopeMode::PerSphere,
                FeatureScopeModeView::PerWorkspace => FeatureScopeMode::PerWorkspace,
            },
            targets: scope.targets,
            tiers: scope
                .tiers
                .into_iter()
                .map(|entry| {
                    let tier = match entry.tier {
                        AiTierView::Glance => AiTier::Glance,
                        AiTierView::Insight => AiTier::Insight,
                    };
                    (entry.target, tier)
                })
                .collect::<BTreeMap<_, _>>(),
        };
        self.edit_state(|state| state.set_ai_scope(&name, &choice))
    }

    /// What a cloud provider may keep.
    pub fn data_retention(&self) -> DataRetentionView {
        match self.read_state(|state| state.data_retention()) {
            DataRetention::Disabled => DataRetentionView::Disabled,
            DataRetention::SafetyRetention => DataRetentionView::SafetyRetention,
            DataRetention::ZeroRetention => DataRetentionView::ZeroRetention,
        }
    }

    /// Sets what a cloud provider may keep.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_data_retention(&self, retention: DataRetentionView) -> Result<(), CoreError> {
        let retention = match retention {
            DataRetentionView::Disabled => DataRetention::Disabled,
            DataRetentionView::SafetyRetention => DataRetention::SafetyRetention,
            DataRetentionView::ZeroRetention => DataRetention::ZeroRetention,
        };
        self.edit_state(|state| state.set_data_retention(retention))
    }

    /// The monthly cloud budget in whole currency units.
    pub fn monthly_cloud_budget(&self) -> Option<u64> {
        self.read_state(|state| state.monthly_cloud_budget())
    }

    /// Sets the monthly cloud budget; `None` clears it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_monthly_cloud_budget(&self, budget: Option<u64>) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_monthly_cloud_budget(budget))
    }

    /// What other AI apps may do.
    pub fn other_apps_access(&self) -> OtherAppsAccessView {
        match self.read_state(|state| state.other_apps_access()) {
            OtherAppsAccess::Off => OtherAppsAccessView::Off,
            OtherAppsAccess::ReadOnly => OtherAppsAccessView::ReadOnly,
            OtherAppsAccess::ReadAndWrite => OtherAppsAccessView::ReadAndWrite,
        }
    }

    /// Sets what other AI apps may do.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_other_apps_access(&self, access: OtherAppsAccessView) -> Result<(), CoreError> {
        let access = match access {
            OtherAppsAccessView::Off => OtherAppsAccess::Off,
            OtherAppsAccessView::ReadOnly => OtherAppsAccess::ReadOnly,
            OtherAppsAccessView::ReadAndWrite => OtherAppsAccess::ReadAndWrite,
        };
        self.edit_state(|state| state.set_other_apps_access(access))
    }

    /// What an agent may change without asking.
    pub fn agent_changes(&self) -> AgentChangesView {
        match self.read_state(|state| state.agent_changes()) {
            AgentChanges::AskEveryTime => AgentChangesView::AskEveryTime,
            AgentChanges::AskBeforeDeleting => AgentChangesView::AskBeforeDeleting,
            AgentChanges::NeverAsk => AgentChangesView::NeverAsk,
        }
    }

    /// Sets what an agent may change without asking.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_agent_changes(&self, changes: AgentChangesView) -> Result<(), CoreError> {
        let changes = match changes {
            AgentChangesView::AskEveryTime => AgentChanges::AskEveryTime,
            AgentChangesView::AskBeforeDeleting => AgentChanges::AskBeforeDeleting,
            AgentChangesView::NeverAsk => AgentChanges::NeverAsk,
        };
        self.edit_state(|state| state.set_agent_changes(changes))
    }

    /// Whether the compacted copy is shown before a cloud model sees it.
    pub fn previews_before_sending(&self) -> bool {
        self.read_state(|state| state.previews_before_sending())
    }

    /// Sets whether the compacted copy is shown before sending.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_previews_before_sending(&self, is_on: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_previews_before_sending(is_on))
    }

    /// Whether every AI request is logged on this Mac.
    pub fn keeps_ai_activity_log(&self) -> bool {
        self.read_state(|state| state.keeps_ai_activity_log())
    }

    /// Sets whether every AI request is logged on this Mac.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_keeps_ai_activity_log(&self, is_on: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_keeps_ai_activity_log(is_on))
    }

    /// The provider keys recorded; metadata only.
    pub fn provider_keys(&self) -> Vec<ProviderKeyView> {
        self.read_state(|state| state.provider_keys())
            .into_iter()
            .map(|record| ProviderKeyView {
                id: record.id,
                provider: match record.provider {
                    ProviderKind::Local => ProviderKindView::Local,
                    ProviderKind::OpenRouter => ProviderKindView::OpenRouter,
                    ProviderKind::Claude => ProviderKindView::Claude,
                    ProviderKind::OpenAi => ProviderKindView::OpenAi,
                    ProviderKind::Gemini => ProviderKindView::Gemini,
                    ProviderKind::Other => ProviderKindView::Other,
                },
                name: record.name,
                scope: match record.scope {
                    ProviderKeyScope::Default => ProviderKeyScopeView::Default,
                    ProviderKeyScope::PerSphere => ProviderKeyScopeView::PerSphere,
                    ProviderKeyScope::PerWorkspace => ProviderKeyScopeView::PerWorkspace,
                },
                targets: record.targets,
                insight_model: record.insight_model,
                glance_model: record.glance_model,
                endpoint: record.endpoint,
            })
            .collect()
    }

    /// Records a provider key's metadata, or replaces the record with the
    /// same id. The key itself is stored by the platform secret store.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for an unusable id or target; the
    /// document refused the edit.
    pub fn put_provider_key(&self, key: ProviderKeyView) -> Result<(), CoreError> {
        let record = ProviderKeyRecord {
            id: key.id,
            provider: match key.provider {
                ProviderKindView::Local => ProviderKind::Local,
                ProviderKindView::OpenRouter => ProviderKind::OpenRouter,
                ProviderKindView::Claude => ProviderKind::Claude,
                ProviderKindView::OpenAi => ProviderKind::OpenAi,
                ProviderKindView::Gemini => ProviderKind::Gemini,
                ProviderKindView::Other => ProviderKind::Other,
            },
            name: key.name,
            scope: match key.scope {
                ProviderKeyScopeView::Default => ProviderKeyScope::Default,
                ProviderKeyScopeView::PerSphere => ProviderKeyScope::PerSphere,
                ProviderKeyScopeView::PerWorkspace => ProviderKeyScope::PerWorkspace,
            },
            targets: key.targets,
            insight_model: key.insight_model,
            glance_model: key.glance_model,
            endpoint: key.endpoint,
        };
        self.edit_state(|state| state.put_provider_key(&record))
    }

    /// Forgets a provider key's metadata. The caller deletes its secret.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_provider_key(&self, id: String) -> Result<(), CoreError> {
        self.edit_state(|state| state.remove_provider_key(&id))
    }

    /// The workspace the app opens on at launch on this Mac.
    pub fn last_single_workspace(&self) -> Option<String> {
        self.read_state(|state| state.last_single_workspace())
            .map(|workspace| workspace.as_str().to_owned())
    }

    /// Remembers the single workspace shown last; `None` forgets it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_last_single_workspace(&self, workspace: Option<String>) -> Result<(), CoreError> {
        let workspace = workspace.map(WorkspaceId::new);
        self.edit_state(|state| state.set_last_single_workspace(workspace.as_ref()))
    }

    /// Whether this Mac's folders follow the one-root model, so the
    /// conversion question is not asked again.
    pub fn folders_converted(&self) -> bool {
        self.read_state(app_state::AppState::folders_converted)
    }

    /// Records that this Mac's folders follow the one-root model.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_folders_converted(&self) -> Result<(), CoreError> {
        self.edit_state(app_state::AppState::set_folders_converted)
    }
}

fn list_position_view(position: ListPosition) -> ListPositionView {
    match position {
        ListPosition::Top => ListPositionView::Top,
        ListPosition::Bottom => ListPositionView::Bottom,
    }
}

fn list_position(view: ListPositionView) -> ListPosition {
    match view {
        ListPositionView::Top => ListPosition::Top,
        ListPositionView::Bottom => ListPosition::Bottom,
    }
}

fn weekday_view(day: Weekday) -> WeekdayView {
    match day {
        Weekday::Monday => WeekdayView::Monday,
        Weekday::Tuesday => WeekdayView::Tuesday,
        Weekday::Wednesday => WeekdayView::Wednesday,
        Weekday::Thursday => WeekdayView::Thursday,
        Weekday::Friday => WeekdayView::Friday,
        Weekday::Saturday => WeekdayView::Saturday,
        Weekday::Sunday => WeekdayView::Sunday,
    }
}

fn weekday(view: WeekdayView) -> Weekday {
    match view {
        WeekdayView::Monday => Weekday::Monday,
        WeekdayView::Tuesday => Weekday::Tuesday,
        WeekdayView::Wednesday => Weekday::Wednesday,
        WeekdayView::Thursday => Weekday::Thursday,
        WeekdayView::Friday => Weekday::Friday,
        WeekdayView::Saturday => Weekday::Saturday,
        WeekdayView::Sunday => Weekday::Sunday,
    }
}

fn minute_of_day(time: Time) -> u32 {
    u32::try_from(i32::from(time.hour()) * 60 + i32::from(time.minute())).unwrap_or(0)
}

fn time_from_minute(minute: u32) -> Result<Time, CoreError> {
    let invalid =
        || CoreError::InvalidInput(format!("{minute} is not a time of day: 0 to 1439 minutes"));
    let hour = i8::try_from(minute / 60).map_err(|_| invalid())?;
    let minute_part = i8::try_from(minute % 60).map_err(|_| invalid())?;
    Time::new(hour, minute_part, 0, 0).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_after_midnight_round_trip_and_the_day_ends_at_23_59() {
        for minute in [0, 59, 60, 525, 1050, 1439] {
            assert_eq!(minute_of_day(time_from_minute(minute).unwrap()), minute);
        }
        assert!(matches!(
            time_from_minute(1440),
            Err(CoreError::InvalidInput(_))
        ));
        assert!(matches!(
            time_from_minute(u32::MAX),
            Err(CoreError::InvalidInput(_))
        ));
    }

    #[test]
    fn every_weekday_maps_both_ways() {
        for view in [
            WeekdayView::Monday,
            WeekdayView::Tuesday,
            WeekdayView::Wednesday,
            WeekdayView::Thursday,
            WeekdayView::Friday,
            WeekdayView::Saturday,
            WeekdayView::Sunday,
        ] {
            assert_eq!(weekday_view(weekday(view)), view);
        }
    }
}
