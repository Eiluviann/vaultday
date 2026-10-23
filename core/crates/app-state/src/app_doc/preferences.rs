//! The app-level choices of the Rituals and routines, AI and Settings
//! screens: how the day runs, where AI is switched on, the metadata of
//! provider keys, and the last single workspace.
//!
//! All of it lives in the document's `settings` map (and `device_settings`
//! for what belongs to one Mac). The AI choices and the rituals are stored
//! and inert in the base version. Key material never enters the document:
//! a provider key's secret goes to the [`crate::SecretStore`] under
//! [`crate::SecretKind::ProviderApiKey`] and only its name, provider and
//! scope are recorded here.

use std::collections::BTreeMap;

use automerge::transaction::Transactable;
use domain::workspace::WorkspaceId;
use jiff::civil::{Time, Weekday};

use super::{AppState, FeatureScopeMode};
use crate::doc::{
    KEY_SEPARATOR, composite_key, delete_if_present, get_bool, get_str, get_u64, keys,
};
use crate::error::AppStateError;

const BREAK_PATTERN_KEY: &str = "break_pattern";
const ROLLOVER_KEY: &str = "rollover_position";
const NEW_TASK_KEY: &str = "new_task_position";
const OVERRUN_KEY: &str = "overrun_warning";
const SHORTCUT_CAPS_KEY: &str = "shortcut_caps";
const WORK_HOURS_KIND_KEY: &str = "work_hours_kind";
const WORK_HOURS_START_KEY: &str = "work_hours_start";
const WORK_HOURS_END_KEY: &str = "work_hours_end";
const WORK_DAYS_KEY: &str = "work_days";
const PLAN_LIFE_KEY: &str = "plan_life_after_work";
const REMINDERS_RING_KEY: &str = "reminder_times_ring";
const RITUAL_DAY_PREFIX: &str = "ritual_day";
const AI_SCOPE_PREFIX: &str = "ai_scope";
const AI_TARGETS_PREFIX: &str = "ai_targets";
const AI_TIERS_PREFIX: &str = "ai_tiers";
const AI_RETENTION_KEY: &str = "ai_retention";
const AI_BUDGET_KEY: &str = "ai_monthly_budget";
const AI_OTHER_APPS_KEY: &str = "ai_other_apps";
const AI_AGENT_CHANGES_KEY: &str = "ai_agent_changes";
const AI_PREVIEW_KEY: &str = "ai_preview_before_sending";
const AI_ACTIVITY_LOG_KEY: &str = "ai_activity_log";
const PROVIDER_KEY_PREFIX: &str = "provider_key";
const LAST_WORKSPACE_KEY: &str = "last_single_workspace";
const FOLDERS_CONVERTED_KEY: &str = "folders_converted";

/// The target an [`AiScopeChoice`] tier is stored under when it applies to
/// everything the scope covers.
pub const AI_TIER_EVERYWHERE: &str = "*";

/// The longest name or target an AI scope or provider key accepts, in bytes.
const MAX_NAME_BYTES: usize = 64;

/// Focus and break length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakPattern {
    /// No breaks.
    None,
    /// Fifty minutes of focus, ten of break.
    #[default]
    FiftyTen,
    /// Twenty-five minutes of focus, five of break.
    TwentyFiveFive,
}

/// Where a task lands in a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListPosition {
    /// At the top.
    Top,
    /// At the bottom.
    Bottom,
}

/// When a day that runs over warns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverrunWarning {
    /// When the plan reaches into free time.
    #[default]
    IntoFreeTime,
    /// Never.
    Never,
}

/// How the shortcut caps show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutCaps {
    /// They animate as keys are pressed.
    #[default]
    Animated,
    /// They change at once.
    Instant,
}

/// Whether the working hours are fixed times or a daily target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkingHoursKind {
    /// Set start and end.
    #[default]
    Fixed,
    /// Planned around the calendar and a daily target.
    Flexible,
}

/// The working hours of the Work sphere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingHours {
    /// Fixed or flexible.
    pub kind: WorkingHoursKind,
    /// When work starts.
    pub start: Time,
    /// When work ends.
    pub end: Time,
    /// The days worked, in week order.
    pub days: Vec<Weekday>,
}

/// How much an AI feature may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTier {
    /// One item at a time; light and on-device models.
    Glance,
    /// Related tasks, notes and mail; a larger model.
    Insight,
}

/// Where an AI level or feature is on, and at which tier for each target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiScopeChoice {
    /// The switch.
    pub mode: FeatureScopeMode,
    /// Sphere names or workspace ids, by mode; empty for off and everywhere.
    pub targets: Vec<String>,
    /// The tier per target; [`AI_TIER_EVERYWHERE`] is the tier of the whole scope.
    pub tiers: BTreeMap<String, AiTier>,
}

/// What a cloud provider may keep of the requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataRetention {
    /// Nothing kept.
    Disabled,
    /// Safety retention allowed.
    SafetyRetention,
    /// Only providers with zero data retention.
    #[default]
    ZeroRetention,
}

/// What other AI apps may do through the `vaultday` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OtherAppsAccess {
    /// Nothing.
    #[default]
    Off,
    /// Read.
    ReadOnly,
    /// Read and write.
    ReadAndWrite,
}

/// What an agent may change without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentChanges {
    /// Ask for every change.
    #[default]
    AskEveryTime,
    /// Ask before deleting.
    AskBeforeDeleting,
    /// Never ask.
    NeverAsk,
}

/// Which provider a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Ollama, LM Studio or a server the user runs.
    Local,
    /// OpenRouter.
    OpenRouter,
    /// Anthropic.
    Claude,
    /// OpenAI.
    OpenAi,
    /// Google Gemini.
    Gemini,
    /// Any OpenAI-compatible endpoint.
    Other,
}

/// Where a provider key is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKeyScope {
    /// The key for every workspace without its own.
    Default,
    /// The chosen spheres.
    PerSphere,
    /// The chosen workspaces.
    PerWorkspace,
}

/// What the document records of a provider key. The key itself is in the
/// secret store under [`crate::SecretKind::ProviderApiKey`], named by `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderKeyRecord {
    /// The key's id, which also names its secret.
    pub id: String,
    /// The provider.
    pub provider: ProviderKind,
    /// What the user calls the key.
    pub name: String,
    /// Where it is used.
    pub scope: ProviderKeyScope,
    /// Sphere names or workspace ids, by scope.
    pub targets: Vec<String>,
    /// The model for AI Insight, if one was picked.
    pub insight_model: Option<String>,
    /// The model for AI Glance jobs too heavy for this Mac, if one was picked.
    pub glance_model: Option<String>,
    /// The endpoint of a local or other provider; not a secret.
    pub endpoint: Option<String>,
}

impl BreakPattern {
    const fn label(self) -> &'static str {
        match self {
            BreakPattern::None => "none",
            BreakPattern::FiftyTen => "50-10",
            BreakPattern::TwentyFiveFive => "25-5",
        }
    }
}

impl ListPosition {
    const fn label(self) -> &'static str {
        match self {
            ListPosition::Top => "top",
            ListPosition::Bottom => "bottom",
        }
    }
}

impl AiTier {
    const fn label(self) -> &'static str {
        match self {
            AiTier::Glance => "glance",
            AiTier::Insight => "insight",
        }
    }

    fn parse(label: &str) -> Option<AiTier> {
        match label {
            "glance" => Some(AiTier::Glance),
            "insight" => Some(AiTier::Insight),
            _ => None,
        }
    }
}

impl DataRetention {
    const fn label(self) -> &'static str {
        match self {
            DataRetention::Disabled => "disabled",
            DataRetention::SafetyRetention => "safety",
            DataRetention::ZeroRetention => "zero",
        }
    }
}

impl OtherAppsAccess {
    const fn label(self) -> &'static str {
        match self {
            OtherAppsAccess::Off => "off",
            OtherAppsAccess::ReadOnly => "read-only",
            OtherAppsAccess::ReadAndWrite => "read-and-write",
        }
    }
}

impl AgentChanges {
    const fn label(self) -> &'static str {
        match self {
            AgentChanges::AskEveryTime => "ask-every-time",
            AgentChanges::AskBeforeDeleting => "ask-before-deleting",
            AgentChanges::NeverAsk => "never-ask",
        }
    }
}

impl ProviderKind {
    const fn label(self) -> &'static str {
        match self {
            ProviderKind::Local => "local",
            ProviderKind::OpenRouter => "openrouter",
            ProviderKind::Claude => "claude",
            ProviderKind::OpenAi => "openai",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Other => "other",
        }
    }

    fn parse(label: &str) -> Option<ProviderKind> {
        [
            ProviderKind::Local,
            ProviderKind::OpenRouter,
            ProviderKind::Claude,
            ProviderKind::OpenAi,
            ProviderKind::Gemini,
            ProviderKind::Other,
        ]
        .into_iter()
        .find(|kind| kind.label() == label)
    }
}

impl ProviderKeyScope {
    const fn label(self) -> &'static str {
        match self {
            ProviderKeyScope::Default => "default",
            ProviderKeyScope::PerSphere => "per-sphere",
            ProviderKeyScope::PerWorkspace => "per-workspace",
        }
    }
}

const WEEK: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

const fn weekday_label(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "mon",
        Weekday::Tuesday => "tue",
        Weekday::Wednesday => "wed",
        Weekday::Thursday => "thu",
        Weekday::Friday => "fri",
        Weekday::Saturday => "sat",
        Weekday::Sunday => "sun",
    }
}

fn parse_weekday(label: &str) -> Option<Weekday> {
    WEEK.into_iter().find(|day| weekday_label(*day) == label)
}

impl AppState {
    // How the day runs.

    /// The break pattern; fifty minutes of focus and ten of break until set.
    #[must_use]
    pub fn break_pattern(&self) -> BreakPattern {
        match self.setting_str(BREAK_PATTERN_KEY).as_deref() {
            Some("none") => BreakPattern::None,
            Some("25-5") => BreakPattern::TwentyFiveFive,
            _ => BreakPattern::FiftyTen,
        }
    }

    /// Sets the break pattern.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_break_pattern(&mut self, pattern: BreakPattern) -> Result<(), AppStateError> {
        self.put_setting(BREAK_PATTERN_KEY, pattern.label())
    }

    /// Where unfinished tasks land at roll-over; the top until set.
    #[must_use]
    pub fn rollover_position(&self) -> ListPosition {
        self.position(ROLLOVER_KEY, ListPosition::Top)
    }

    /// Sets where unfinished tasks land at roll-over.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_rollover_position(&mut self, position: ListPosition) -> Result<(), AppStateError> {
        self.put_setting(ROLLOVER_KEY, position.label())
    }

    /// Where a new task lands in a list; the bottom until set.
    #[must_use]
    pub fn new_task_position(&self) -> ListPosition {
        self.position(NEW_TASK_KEY, ListPosition::Bottom)
    }

    /// Sets where a new task lands in a list.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_new_task_position(&mut self, position: ListPosition) -> Result<(), AppStateError> {
        self.put_setting(NEW_TASK_KEY, position.label())
    }

    /// When a day that runs over warns; into free time until set.
    #[must_use]
    pub fn overrun_warning(&self) -> OverrunWarning {
        match self.setting_str(OVERRUN_KEY).as_deref() {
            Some("never") => OverrunWarning::Never,
            _ => OverrunWarning::IntoFreeTime,
        }
    }

    /// Sets when a day that runs over warns.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_overrun_warning(&mut self, warning: OverrunWarning) -> Result<(), AppStateError> {
        let label = match warning {
            OverrunWarning::IntoFreeTime => "into-free-time",
            OverrunWarning::Never => "never",
        };
        self.put_setting(OVERRUN_KEY, label)
    }

    /// How the shortcut caps show; animated until set.
    #[must_use]
    pub fn shortcut_caps(&self) -> ShortcutCaps {
        match self.setting_str(SHORTCUT_CAPS_KEY).as_deref() {
            Some("instant") => ShortcutCaps::Instant,
            _ => ShortcutCaps::Animated,
        }
    }

    /// Sets how the shortcut caps show.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_shortcut_caps(&mut self, caps: ShortcutCaps) -> Result<(), AppStateError> {
        let label = match caps {
            ShortcutCaps::Animated => "animated",
            ShortcutCaps::Instant => "instant",
        };
        self.put_setting(SHORTCUT_CAPS_KEY, label)
    }

    /// The working hours: fixed, 09:00 to 17:30, Monday to Friday until set.
    #[must_use]
    pub fn working_hours(&self) -> WorkingHours {
        let time = |key: &str, fallback: Time| {
            self.setting_str(key)
                .and_then(|text| text.parse().ok())
                .unwrap_or(fallback)
        };
        let days = self.setting_str(WORK_DAYS_KEY).map_or_else(
            || WEEK[..5].to_vec(),
            |text| {
                WEEK.into_iter()
                    .filter(|day| text.split(',').any(|label| label == weekday_label(*day)))
                    .collect()
            },
        );
        WorkingHours {
            kind: match self.setting_str(WORK_HOURS_KIND_KEY).as_deref() {
                Some("flexible") => WorkingHoursKind::Flexible,
                _ => WorkingHoursKind::Fixed,
            },
            start: time(WORK_HOURS_START_KEY, Time::constant(9, 0, 0, 0)),
            end: time(WORK_HOURS_END_KEY, Time::constant(17, 30, 0, 0)),
            days,
        }
    }

    /// Stores the working hours; the days are kept in week order.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_working_hours(&mut self, hours: &WorkingHours) -> Result<(), AppStateError> {
        let kind = match hours.kind {
            WorkingHoursKind::Fixed => "fixed",
            WorkingHoursKind::Flexible => "flexible",
        };
        let days: Vec<&str> = WEEK
            .into_iter()
            .filter(|day| hours.days.contains(day))
            .map(weekday_label)
            .collect();
        self.put_setting(WORK_HOURS_KIND_KEY, kind)?;
        self.put_setting(WORK_HOURS_START_KEY, &hours.start.to_string())?;
        self.put_setting(WORK_HOURS_END_KEY, &hours.end.to_string())?;
        self.put_setting(WORK_DAYS_KEY, &days.join(","))
    }

    /// Whether Life tasks are planned into the time after work; on until set.
    #[must_use]
    pub fn plans_life_after_work(&self) -> bool {
        self.setting_bool(PLAN_LIFE_KEY, true)
    }

    /// Sets whether Life tasks are planned into the time after work.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_plans_life_after_work(&mut self, is_on: bool) -> Result<(), AppStateError> {
        self.put_setting_bool(PLAN_LIFE_KEY, is_on)
    }

    /// Whether a reminder rings at its time; on until set.
    #[must_use]
    pub fn reminder_times_ring(&self) -> bool {
        self.setting_bool(REMINDERS_RING_KEY, true)
    }

    /// Sets whether a reminder rings at its time.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_reminder_times_ring(&mut self, is_on: bool) -> Result<(), AppStateError> {
        self.put_setting_bool(REMINDERS_RING_KEY, is_on)
    }

    /// The weekday a weekly ritual runs on, by ritual name.
    #[must_use]
    pub fn ritual_day(&self, ritual: &str) -> Option<Weekday> {
        let key = composite_key(&[RITUAL_DAY_PREFIX, ritual]);
        parse_weekday(&self.setting_str(&key)?)
    }

    /// Sets the weekday a weekly ritual runs on; `None` clears it. Its time
    /// is [`AppState::set_ritual_time`].
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty or oversized ritual
    /// name; the document refused the edit.
    pub fn set_ritual_day(
        &mut self,
        ritual: &str,
        day: Option<Weekday>,
    ) -> Result<(), AppStateError> {
        check_name("ritual", ritual)?;
        let key = composite_key(&[RITUAL_DAY_PREFIX, ritual]);
        match day {
            Some(day) => self.put_setting(&key, weekday_label(day)),
            None => self.delete_setting(&key),
        }
    }

    // AI, stored and inert.

    /// Where an AI level or feature is on, by name; off until set.
    #[must_use]
    pub fn ai_scope(&self, name: &str) -> AiScopeChoice {
        let mode = match self
            .setting_str(&composite_key(&[AI_SCOPE_PREFIX, name]))
            .as_deref()
        {
            Some("everywhere") => FeatureScopeMode::Everywhere,
            Some("per-sphere") => FeatureScopeMode::PerSphere,
            Some("per-workspace") => FeatureScopeMode::PerWorkspace,
            _ => FeatureScopeMode::Off,
        };
        let targets = split_list(self.setting_str(&composite_key(&[AI_TARGETS_PREFIX, name])));
        let tiers = split_list(self.setting_str(&composite_key(&[AI_TIERS_PREFIX, name])))
            .iter()
            .filter_map(|pair| {
                let (target, tier) = pair.split_once('=')?;
                Some((target.to_owned(), AiTier::parse(tier)?))
            })
            .collect();
        AiScopeChoice {
            mode,
            targets,
            tiers,
        }
    }

    /// Stores where an AI level or feature is on, by name.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty or oversized name, or a
    /// target that is empty, oversized or holds `,` or `=`; the document
    /// refused the edit.
    pub fn set_ai_scope(
        &mut self,
        name: &str,
        choice: &AiScopeChoice,
    ) -> Result<(), AppStateError> {
        check_name("AI scope", name)?;
        for target in choice.targets.iter().chain(choice.tiers.keys()) {
            check_target(target)?;
        }
        let mode = match choice.mode {
            FeatureScopeMode::Off => "off",
            FeatureScopeMode::Everywhere => "everywhere",
            FeatureScopeMode::PerSphere => "per-sphere",
            FeatureScopeMode::PerWorkspace => "per-workspace",
        };
        self.put_setting(&composite_key(&[AI_SCOPE_PREFIX, name]), mode)?;
        let targets_key = composite_key(&[AI_TARGETS_PREFIX, name]);
        if choice.targets.is_empty() {
            self.delete_setting(&targets_key)?;
        } else {
            self.put_setting(&targets_key, &choice.targets.join(","))?;
        }
        let tiers_key = composite_key(&[AI_TIERS_PREFIX, name]);
        if choice.tiers.is_empty() {
            return self.delete_setting(&tiers_key);
        }
        let pairs: Vec<String> = choice
            .tiers
            .iter()
            .map(|(target, tier)| format!("{target}={}", tier.label()))
            .collect();
        self.put_setting(&tiers_key, &pairs.join(","))
    }

    /// What a cloud provider may keep; zero data retention until set.
    #[must_use]
    pub fn data_retention(&self) -> DataRetention {
        match self.setting_str(AI_RETENTION_KEY).as_deref() {
            Some("disabled") => DataRetention::Disabled,
            Some("safety") => DataRetention::SafetyRetention,
            _ => DataRetention::ZeroRetention,
        }
    }

    /// Sets what a cloud provider may keep.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_data_retention(&mut self, retention: DataRetention) -> Result<(), AppStateError> {
        self.put_setting(AI_RETENTION_KEY, retention.label())
    }

    /// The monthly cloud budget in whole currency units; none until set.
    #[must_use]
    pub fn monthly_cloud_budget(&self) -> Option<u64> {
        get_u64(&self.doc, &self.objects.settings, AI_BUDGET_KEY)
    }

    /// Sets the monthly cloud budget; `None` clears it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_monthly_cloud_budget(&mut self, budget: Option<u64>) -> Result<(), AppStateError> {
        match budget {
            Some(budget) => {
                self.doc
                    .put(&self.objects.settings, AI_BUDGET_KEY, budget)?;
                Ok(())
            }
            None => self.delete_setting(AI_BUDGET_KEY),
        }
    }

    /// What other AI apps may do; nothing until set.
    #[must_use]
    pub fn other_apps_access(&self) -> OtherAppsAccess {
        match self.setting_str(AI_OTHER_APPS_KEY).as_deref() {
            Some("read-only") => OtherAppsAccess::ReadOnly,
            Some("read-and-write") => OtherAppsAccess::ReadAndWrite,
            _ => OtherAppsAccess::Off,
        }
    }

    /// Sets what other AI apps may do.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_other_apps_access(&mut self, access: OtherAppsAccess) -> Result<(), AppStateError> {
        self.put_setting(AI_OTHER_APPS_KEY, access.label())
    }

    /// What an agent may change without asking; ask every time until set.
    #[must_use]
    pub fn agent_changes(&self) -> AgentChanges {
        match self.setting_str(AI_AGENT_CHANGES_KEY).as_deref() {
            Some("ask-before-deleting") => AgentChanges::AskBeforeDeleting,
            Some("never-ask") => AgentChanges::NeverAsk,
            _ => AgentChanges::AskEveryTime,
        }
    }

    /// Sets what an agent may change without asking.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_agent_changes(&mut self, changes: AgentChanges) -> Result<(), AppStateError> {
        self.put_setting(AI_AGENT_CHANGES_KEY, changes.label())
    }

    /// Whether the compacted copy is shown before a cloud model sees it; on until set.
    #[must_use]
    pub fn previews_before_sending(&self) -> bool {
        self.setting_bool(AI_PREVIEW_KEY, true)
    }

    /// Sets whether the compacted copy is shown before sending.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_previews_before_sending(&mut self, is_on: bool) -> Result<(), AppStateError> {
        self.put_setting_bool(AI_PREVIEW_KEY, is_on)
    }

    /// Whether every AI request is logged on this Mac; on until set.
    #[must_use]
    pub fn keeps_ai_activity_log(&self) -> bool {
        self.setting_bool(AI_ACTIVITY_LOG_KEY, true)
    }

    /// Sets whether every AI request is logged on this Mac.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_keeps_ai_activity_log(&mut self, is_on: bool) -> Result<(), AppStateError> {
        self.put_setting_bool(AI_ACTIVITY_LOG_KEY, is_on)
    }

    // Provider keys (metadata only).

    /// The provider keys recorded, by id.
    #[must_use]
    pub fn provider_keys(&self) -> Vec<ProviderKeyRecord> {
        let mut ids: Vec<String> = keys(&self.doc, &self.objects.settings)
            .iter()
            .filter_map(|key| {
                let mut parts = key.split(KEY_SEPARATOR);
                (parts.next() == Some(PROVIDER_KEY_PREFIX))
                    .then(|| parts.next().map(str::to_owned))?
            })
            .collect();
        ids.dedup();
        ids.into_iter()
            .filter_map(|id| self.provider_key(&id))
            .collect()
    }

    /// One provider key's record.
    #[must_use]
    pub fn provider_key(&self, id: &str) -> Option<ProviderKeyRecord> {
        let field = |name: &str| self.setting_str(&composite_key(&[PROVIDER_KEY_PREFIX, id, name]));
        let provider = ProviderKind::parse(&field("provider")?)?;
        let scope = match field("scope").as_deref() {
            Some("per-sphere") => ProviderKeyScope::PerSphere,
            Some("per-workspace") => ProviderKeyScope::PerWorkspace,
            _ => ProviderKeyScope::Default,
        };
        Some(ProviderKeyRecord {
            id: id.to_owned(),
            provider,
            name: field("name").unwrap_or_default(),
            scope,
            targets: split_list(field("targets")),
            insight_model: field("insight_model"),
            glance_model: field("glance_model"),
            endpoint: field("endpoint"),
        })
    }

    /// Records a provider key or replaces the record with the same id.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty or oversized id, or a
    /// target that is empty, oversized or holds `,` or `=`; the document
    /// refused the edit.
    pub fn put_provider_key(&mut self, record: &ProviderKeyRecord) -> Result<(), AppStateError> {
        check_name("provider key id", &record.id)?;
        for target in &record.targets {
            check_target(target)?;
        }
        let id = record.id.as_str();
        let key = |name: &str| composite_key(&[PROVIDER_KEY_PREFIX, id, name]);
        self.put_setting(&key("provider"), record.provider.label())?;
        self.put_setting(&key("name"), &record.name)?;
        self.put_setting(&key("scope"), record.scope.label())?;
        self.put_optional_setting(
            &key("targets"),
            (!record.targets.is_empty())
                .then(|| record.targets.join(","))
                .as_deref(),
        )?;
        self.put_optional_setting(&key("insight_model"), record.insight_model.as_deref())?;
        self.put_optional_setting(&key("glance_model"), record.glance_model.as_deref())?;
        self.put_optional_setting(&key("endpoint"), record.endpoint.as_deref())
    }

    /// Forgets a provider key's record. Its secret is deleted by the caller.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_provider_key(&mut self, id: &str) -> Result<(), AppStateError> {
        for name in [
            "provider",
            "name",
            "scope",
            "targets",
            "insight_model",
            "glance_model",
            "endpoint",
        ] {
            self.delete_setting(&composite_key(&[PROVIDER_KEY_PREFIX, id, name]))?;
        }
        Ok(())
    }

    // The last single workspace.

    /// The workspace the app opens on at launch on this Mac: the last single
    /// workspace shown. Showing All never replaces it.
    #[must_use]
    pub fn last_single_workspace(&self) -> Option<WorkspaceId> {
        get_str(
            &self.doc,
            &self.objects.device_settings,
            &self.device_key(LAST_WORKSPACE_KEY),
        )
        .filter(|id| !id.is_empty())
        .map(WorkspaceId::new)
    }

    /// Remembers the single workspace shown last; `None` forgets it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_last_single_workspace(
        &mut self,
        workspace: Option<&WorkspaceId>,
    ) -> Result<(), AppStateError> {
        let key = self.device_key(LAST_WORKSPACE_KEY);
        match workspace {
            Some(workspace) => {
                self.doc
                    .put(&self.objects.device_settings, key, workspace.as_str())?;
                Ok(())
            }
            None => delete_if_present(&mut self.doc, &self.objects.device_settings, &key),
        }
    }

    // The one-root folder model.

    /// Whether this device has converted the further folders of an earlier
    /// build into linked or moved folders, or found none to convert. The
    /// question is asked once per device, since grants are per device.
    #[must_use]
    pub fn folders_converted(&self) -> bool {
        get_bool(
            &self.doc,
            &self.objects.device_settings,
            &self.device_key(FOLDERS_CONVERTED_KEY),
        )
        .unwrap_or(false)
    }

    /// Records that this device's folders follow the one-root model.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_folders_converted(&mut self) -> Result<(), AppStateError> {
        let key = self.device_key(FOLDERS_CONVERTED_KEY);
        self.doc.put(&self.objects.device_settings, key, true)?;
        Ok(())
    }

    // Helpers.

    fn setting_str(&self, key: &str) -> Option<String> {
        get_str(&self.doc, &self.objects.settings, key)
    }

    fn setting_bool(&self, key: &str, fallback: bool) -> bool {
        get_bool(&self.doc, &self.objects.settings, key).unwrap_or(fallback)
    }

    fn position(&self, key: &str, fallback: ListPosition) -> ListPosition {
        match self.setting_str(key).as_deref() {
            Some("top") => ListPosition::Top,
            Some("bottom") => ListPosition::Bottom,
            _ => fallback,
        }
    }

    fn put_setting(&mut self, key: &str, value: &str) -> Result<(), AppStateError> {
        self.doc.put(&self.objects.settings, key, value)?;
        Ok(())
    }

    fn put_setting_bool(&mut self, key: &str, value: bool) -> Result<(), AppStateError> {
        self.doc.put(&self.objects.settings, key, value)?;
        Ok(())
    }

    fn put_optional_setting(
        &mut self,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), AppStateError> {
        match value {
            Some(value) => self.put_setting(key, value),
            None => self.delete_setting(key),
        }
    }

    fn delete_setting(&mut self, key: &str) -> Result<(), AppStateError> {
        let settings = self.objects.settings.clone();
        delete_if_present(&mut self.doc, &settings, key)
    }
}

fn split_list(text: Option<String>) -> Vec<String> {
    text.map(|text| {
        text.split(',')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

fn check_name(what: &str, name: &str) -> Result<(), AppStateError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.contains(KEY_SEPARATOR) {
        return Err(AppStateError::InvalidSetting(format!(
            "{what} {name:?} must be 1 to {MAX_NAME_BYTES} bytes without control separators"
        )));
    }
    Ok(())
}

fn check_target(target: &str) -> Result<(), AppStateError> {
    check_name("target", target)?;
    if target.contains([',', '=']) {
        return Err(AppStateError::InvalidSetting(format!(
            "target {target:?} must not hold `,` or `=`"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;

    fn fresh(device: &str) -> AppState {
        AppState::new(DeviceId::new(device).expect("device id")).expect("document")
    }

    #[test]
    fn defaults_hold_until_set() {
        let state = fresh("mac-a");
        assert_eq!(state.break_pattern(), BreakPattern::FiftyTen);
        assert_eq!(state.rollover_position(), ListPosition::Top);
        assert_eq!(state.new_task_position(), ListPosition::Bottom);
        assert_eq!(state.overrun_warning(), OverrunWarning::IntoFreeTime);
        assert_eq!(state.shortcut_caps(), ShortcutCaps::Animated);
        let hours = state.working_hours();
        assert_eq!(hours.kind, WorkingHoursKind::Fixed);
        assert_eq!(hours.days.len(), 5);
        assert!(state.plans_life_after_work() && state.reminder_times_ring());
        assert_eq!(state.ai_scope("level"), AiScopeChoice::default());
        assert_eq!(state.data_retention(), DataRetention::ZeroRetention);
        assert_eq!(state.other_apps_access(), OtherAppsAccess::Off);
        assert_eq!(state.agent_changes(), AgentChanges::AskEveryTime);
        assert!(state.previews_before_sending() && state.keeps_ai_activity_log());
        assert_eq!(state.monthly_cloud_budget(), None);
        assert!(state.provider_keys().is_empty());
        assert_eq!(state.last_single_workspace(), None);
    }

    #[test]
    fn the_day_choices_round_trip_and_merge_to_another_device() {
        let mut mac = fresh("mac-a");
        let mut other = fresh("mac-b");
        mac.set_break_pattern(BreakPattern::TwentyFiveFive).unwrap();
        mac.set_rollover_position(ListPosition::Bottom).unwrap();
        mac.set_new_task_position(ListPosition::Top).unwrap();
        mac.set_overrun_warning(OverrunWarning::Never).unwrap();
        mac.set_shortcut_caps(ShortcutCaps::Instant).unwrap();
        mac.set_plans_life_after_work(false).unwrap();
        mac.set_reminder_times_ring(false).unwrap();
        mac.set_ritual_day("weekly-review", Some(Weekday::Friday))
            .unwrap();
        mac.set_ritual_day("project-review", Some(Weekday::Sunday))
            .unwrap();
        mac.set_ritual_day("project-review", None).unwrap();
        let hours = WorkingHours {
            kind: WorkingHoursKind::Flexible,
            start: Time::constant(8, 0, 0, 0),
            end: Time::constant(16, 0, 0, 0),
            days: vec![Weekday::Wednesday, Weekday::Monday],
        };
        mac.set_working_hours(&hours).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        for state in [&mac, &other] {
            assert_eq!(state.break_pattern(), BreakPattern::TwentyFiveFive);
            assert_eq!(state.rollover_position(), ListPosition::Bottom);
            assert_eq!(state.new_task_position(), ListPosition::Top);
            assert_eq!(state.overrun_warning(), OverrunWarning::Never);
            assert_eq!(state.shortcut_caps(), ShortcutCaps::Instant);
            assert!(!state.plans_life_after_work() && !state.reminder_times_ring());
            assert_eq!(state.ritual_day("weekly-review"), Some(Weekday::Friday));
            assert_eq!(state.ritual_day("project-review"), None);
            assert_eq!(
                state.working_hours().days,
                vec![Weekday::Monday, Weekday::Wednesday]
            );
            assert_eq!(state.working_hours().kind, WorkingHoursKind::Flexible);
        }
    }

    #[test]
    fn an_ai_scope_keeps_its_targets_and_tiers() {
        let mut state = fresh("mac-a");
        let choice = AiScopeChoice {
            mode: FeatureScopeMode::PerWorkspace,
            targets: vec!["ws-1".to_owned(), "ws-2".to_owned()],
            tiers: BTreeMap::from([
                ("ws-1".to_owned(), AiTier::Insight),
                ("ws-2".to_owned(), AiTier::Glance),
            ]),
        };
        state.set_ai_scope("level", &choice).unwrap();
        assert_eq!(state.ai_scope("level"), choice);
        assert_eq!(state.ai_scope("sort-tasks"), AiScopeChoice::default());
        state
            .set_ai_scope("level", &AiScopeChoice::default())
            .unwrap();
        assert_eq!(state.ai_scope("level"), AiScopeChoice::default());
    }

    #[test]
    fn an_ai_scope_refuses_names_and_targets_that_would_break_the_encoding() {
        let mut state = fresh("mac-a");
        let bad_target = AiScopeChoice {
            mode: FeatureScopeMode::PerWorkspace,
            targets: vec!["a,b".to_owned()],
            tiers: BTreeMap::new(),
        };
        assert!(matches!(
            state.set_ai_scope("level", &bad_target),
            Err(AppStateError::InvalidSetting(_))
        ));
        assert!(matches!(
            state.set_ai_scope("", &AiScopeChoice::default()),
            Err(AppStateError::InvalidSetting(_))
        ));
        assert!(matches!(
            state.set_ritual_day("a\u{1f}b", Some(Weekday::Monday)),
            Err(AppStateError::InvalidSetting(_))
        ));
        assert_eq!(state.ai_scope("level"), AiScopeChoice::default());
    }

    #[test]
    fn the_other_ai_choices_round_trip() {
        let mut state = fresh("mac-a");
        state.set_data_retention(DataRetention::Disabled).unwrap();
        state.set_monthly_cloud_budget(Some(25)).unwrap();
        state
            .set_other_apps_access(OtherAppsAccess::ReadOnly)
            .unwrap();
        state
            .set_agent_changes(AgentChanges::AskBeforeDeleting)
            .unwrap();
        state.set_previews_before_sending(false).unwrap();
        state.set_keeps_ai_activity_log(false).unwrap();
        assert_eq!(state.data_retention(), DataRetention::Disabled);
        assert_eq!(state.monthly_cloud_budget(), Some(25));
        assert_eq!(state.other_apps_access(), OtherAppsAccess::ReadOnly);
        assert_eq!(state.agent_changes(), AgentChanges::AskBeforeDeleting);
        assert!(!state.previews_before_sending() && !state.keeps_ai_activity_log());
        state.set_monthly_cloud_budget(None).unwrap();
        assert_eq!(state.monthly_cloud_budget(), None);
    }

    #[test]
    fn a_provider_key_record_round_trips_and_holds_no_secret_field() {
        let mut state = fresh("mac-a");
        let work = ProviderKeyRecord {
            id: "key-1".to_owned(),
            provider: ProviderKind::Claude,
            name: "Work key".to_owned(),
            scope: ProviderKeyScope::PerSphere,
            targets: vec!["work".to_owned()],
            insight_model: Some("sonnet".to_owned()),
            glance_model: None,
            endpoint: None,
        };
        let local = ProviderKeyRecord {
            id: "key-2".to_owned(),
            provider: ProviderKind::Local,
            name: "Homelab".to_owned(),
            scope: ProviderKeyScope::Default,
            targets: Vec::new(),
            insight_model: None,
            glance_model: None,
            endpoint: Some("http://nas.local:11434".to_owned()),
        };
        state.put_provider_key(&work).unwrap();
        state.put_provider_key(&local).unwrap();
        assert_eq!(state.provider_keys(), vec![work.clone(), local.clone()]);
        state.remove_provider_key("key-1").unwrap();
        assert_eq!(state.provider_keys(), vec![local]);
        assert_eq!(state.provider_key("key-1"), None);
    }

    #[test]
    fn the_last_single_workspace_is_per_device_and_can_be_forgotten() {
        let mut mac = fresh("mac-a");
        let mut other = fresh("mac-b");
        mac.set_last_single_workspace(Some(&WorkspaceId::new("ws-1")))
            .unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert_eq!(mac.last_single_workspace(), Some(WorkspaceId::new("ws-1")));
        assert_eq!(other.last_single_workspace(), None);
        mac.set_last_single_workspace(None).unwrap();
        assert_eq!(mac.last_single_workspace(), None);
    }

    #[test]
    fn the_folder_conversion_is_answered_once_per_device() {
        let mut mac = fresh("mac-a");
        let mut other = fresh("mac-b");
        assert!(!mac.folders_converted());
        mac.set_folders_converted().unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert!(mac.folders_converted());
        assert!(
            !other.folders_converted(),
            "each Mac converts the grants it holds"
        );
    }
}
