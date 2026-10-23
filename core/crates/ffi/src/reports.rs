//! The report bundle for the Reports tab: a summary the views render, the
//! canonical JSON and the markdown to copy, and the one file "Open in Claude
//! Code" writes. Assembly is the core's ([`vault_base::report`]); the
//! authorship summary is computed by the CLI, so the JSON here carries an
//! empty slot for it.

use vault_base::report::{self, ReportBundle, ReportHours, ReportVariant, StoryMinutes};
use vault_base::{PlainFileAccess, ops};

use crate::{CoreError, ReportLogLineView, WorkspaceHandle};

/// How many tasks of a section are in each state.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportTaskCountsView {
    /// All tasks.
    pub total: u32,
    /// Open.
    pub open: u32,
    /// In progress.
    pub in_progress: u32,
    /// Done.
    pub done: u32,
}

/// One story in a report summary.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportStoryView {
    /// The ticket number.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The story status as written.
    pub status: Option<String>,
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// Next Steps by state.
    pub next_steps: ReportTaskCountsView,
    /// Pending Input by state.
    pub pending_input: ReportTaskCountsView,
}

/// Log lines of one kind.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportLogGroupView {
    /// The kind; absent for lines without one.
    pub kind: Option<String>,
    /// The lines, oldest first.
    pub lines: Vec<ReportLogLineView>,
}

/// Minutes for one story.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportStoryMinutesView {
    /// The story number; absent for time that names none.
    pub story: Option<String>,
    /// Minutes.
    pub minutes: u64,
}

/// One task of a fixed-price report.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportFixedTaskView {
    /// The story.
    pub story: Option<String>,
    /// The task id.
    pub task: Option<String>,
    /// The task's text.
    pub text: Option<String>,
    /// The quote in minutes; absent when nothing is quoted.
    pub quoted_minutes: Option<u64>,
    /// Minutes logged on the task.
    pub delivered_minutes: u64,
    /// Minutes beyond the quote.
    pub overrun_minutes: u64,
}

/// The hours section. `model` says which fields mean anything: `fte` and no
/// model use `logged_minutes` and `by_story` only; `hourly` adds the billable
/// split; `fixed` adds the quote against what was delivered. Tracked time is
/// not a field of any of them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportHoursView {
    /// `hourly`, `fixed`, `fte`, or absent when the workspace has no model.
    pub model: Option<String>,
    /// Minutes the sessions logged.
    pub logged_minutes: u64,
    /// Logged minutes per story (hourly: the billable ones, rounded).
    pub by_story: Vec<ReportStoryMinutesView>,
    /// Hourly: the rounding increment.
    pub rounding_minutes: Option<u32>,
    /// Hourly: billable minutes after rounding.
    pub billable_minutes: u64,
    /// Hourly: non-billable minutes.
    pub non_billable_minutes: u64,
    /// Hourly: non-billable minutes per story.
    pub non_billable_by_story: Vec<ReportStoryMinutesView>,
    /// Fixed: the quotes added up.
    pub quoted_minutes: u64,
    /// Fixed: everything delivered.
    pub delivered_minutes: u64,
    /// Fixed: the unpaid overruns added up.
    pub overrun_minutes: u64,
    /// Fixed: per task.
    pub tasks: Vec<ReportFixedTaskView>,
}

/// A report: what the tab shows, and the two texts it copies.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReportBundleView {
    /// What was asked for.
    pub subject: String,
    /// `story` or `client`.
    pub subject_kind: String,
    /// First day, `YYYY-MM-DD`.
    pub from: String,
    /// Last day, `YYYY-MM-DD`.
    pub to: String,
    /// The billing model, when the workspace has one.
    pub billing: Option<String>,
    /// The stories covered.
    pub stories: Vec<ReportStoryView>,
    /// The log lines in range grouped by kind.
    pub log_groups: Vec<ReportLogGroupView>,
    /// Session lines in range.
    pub session_count: u32,
    /// Session lines without a duration.
    pub unmeasured_session_count: u32,
    /// The hours.
    pub hours: ReportHoursView,
    /// The canonical JSON, authorship slot empty.
    pub json: String,
    /// The markdown rendering.
    pub markdown: String,
}

/// A report file just written.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WrittenReportView {
    /// The file, relative to the workspace root.
    pub relative_path: String,
    /// The file's absolute path.
    pub absolute_path: String,
}

/// Which hours section a report carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReportVariantView {
    /// The hours of the workspace's billing model.
    Full,
    /// Logged hours only, as an FTE report: safe on a shared screen.
    FteSafe,
}

impl From<ReportVariantView> for ReportVariant {
    fn from(view: ReportVariantView) -> Self {
        match view {
            ReportVariantView::Full => ReportVariant::Full,
            ReportVariantView::FteSafe => ReportVariant::FteSafe,
        }
    }
}

/// The clients a report can be asked for: the workspace, then each project.
///
/// # Errors
/// [`CoreError::FileAccess`] for an unreadable vault.
#[uniffi::export]
pub fn report_clients(workspace: &WorkspaceHandle) -> Result<Vec<String>, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(report::report_clients(&workspace.workspace, &vault))
}

/// The report bundle for a story number or a client over `from..=to`
/// (`YYYY-MM-DD`). Reads the vault and writes nothing.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed day, [`CoreError::InvalidInput`]
/// for a reversed range, [`CoreError::NotFound`] when the subject matches
/// no story and no client.
#[uniffi::export]
pub fn report_bundle(
    workspace: &WorkspaceHandle,
    subject: String,
    from: String,
    to: String,
    variant: ReportVariantView,
) -> Result<ReportBundleView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let bundle = report::build_report_bundle_as(
        &workspace.workspace,
        &vault,
        &subject,
        ops::parse_date(&from)?,
        ops::parse_date(&to)?,
        variant.into(),
    )?;
    Ok(bundle_view(&bundle))
}

/// Writes the bundle's JSON to `_agent/reports/<from>-<to>.json` beside the
/// subject (the story's folder, the client's project folder or the workspace
/// root). That file is the only thing written.
///
/// # Errors
/// As [`report_bundle`], and [`CoreError::FileAccess`] when writing fails.
#[uniffi::export]
pub fn write_report(
    workspace: &WorkspaceHandle,
    subject: String,
    from: String,
    to: String,
    variant: ReportVariantView,
) -> Result<WrittenReportView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let written = report::write_report_as(
        &workspace.workspace,
        &vault,
        &PlainFileAccess,
        &subject,
        ops::parse_date(&from)?,
        ops::parse_date(&to)?,
        variant.into(),
    )?;
    Ok(WrittenReportView {
        relative_path: written.relative_path,
        absolute_path: written.absolute_path,
    })
}

fn bundle_view(bundle: &ReportBundle) -> ReportBundleView {
    let report = &bundle.report;
    ReportBundleView {
        subject: report.subject.clone(),
        subject_kind: report.subject_kind.clone(),
        from: report.from.clone(),
        to: report.to.clone(),
        billing: bundle.billing.map(model_name),
        stories: report
            .stories
            .iter()
            .map(|story| ReportStoryView {
                number: story.number.clone(),
                title: story.title.clone(),
                status: story.status.clone(),
                relative_path: story.relative_path.clone(),
                next_steps: counts(&story.next_steps),
                pending_input: counts(&story.pending_input),
            })
            .collect(),
        log_groups: bundle
            .log_by_kind()
            .into_iter()
            .map(|(kind, lines)| ReportLogGroupView {
                kind,
                lines: lines.into_iter().cloned().map(Into::into).collect(),
            })
            .collect(),
        session_count: bundle.sessions.count,
        unmeasured_session_count: bundle.sessions.unmeasured_count,
        hours: hours_view(&bundle.hours),
        json: bundle.to_json(),
        markdown: bundle.to_markdown(),
    }
}

fn model_name(model: vault_base::BillingModel) -> String {
    match model {
        vault_base::BillingModel::Hourly => "hourly",
        vault_base::BillingModel::Fixed => "fixed",
        vault_base::BillingModel::Fte => "fte",
    }
    .to_owned()
}

fn counts(tasks: &[ops::TaskRecord]) -> ReportTaskCountsView {
    let number = |status: &str| {
        u32::try_from(tasks.iter().filter(|task| task.status == status).count()).unwrap_or(u32::MAX)
    };
    ReportTaskCountsView {
        total: u32::try_from(tasks.len()).unwrap_or(u32::MAX),
        open: number("open"),
        in_progress: number("in-progress"),
        done: number("done"),
    }
}

fn hours_view(hours: &ReportHours) -> ReportHoursView {
    let mut view = ReportHoursView {
        model: None,
        logged_minutes: 0,
        by_story: Vec::new(),
        rounding_minutes: None,
        billable_minutes: 0,
        non_billable_minutes: 0,
        non_billable_by_story: Vec::new(),
        quoted_minutes: 0,
        delivered_minutes: 0,
        overrun_minutes: 0,
        tasks: Vec::new(),
    };
    match hours {
        ReportHours::Unbilled(logged) => {
            view.logged_minutes = logged.logged_minutes;
            view.by_story = story_minutes(&logged.by_story);
        }
        ReportHours::Fte(logged) => {
            view.model = Some("fte".to_owned());
            view.logged_minutes = logged.logged_minutes;
            view.by_story = story_minutes(&logged.by_story);
        }
        ReportHours::Hourly(hourly) => {
            view.model = Some("hourly".to_owned());
            view.logged_minutes = hourly.billable_minutes + hourly.non_billable_minutes;
            view.by_story = story_minutes(&hourly.billable_by_story);
            view.rounding_minutes = Some(hourly.rounding_minutes);
            view.billable_minutes = hourly.billable_minutes;
            view.non_billable_minutes = hourly.non_billable_minutes;
            view.non_billable_by_story = story_minutes(&hourly.non_billable_by_story);
        }
        ReportHours::Fixed(fixed) => {
            view.model = Some("fixed".to_owned());
            view.logged_minutes = fixed.delivered_minutes;
            view.quoted_minutes = fixed.quoted_minutes;
            view.delivered_minutes = fixed.delivered_minutes;
            view.overrun_minutes = fixed.overrun_minutes;
            view.tasks = fixed
                .tasks
                .iter()
                .map(|task| ReportFixedTaskView {
                    story: task.story.clone(),
                    task: task.task.clone(),
                    text: task.text.clone(),
                    quoted_minutes: task.quoted_minutes,
                    delivered_minutes: task.delivered_minutes,
                    overrun_minutes: task.overrun_minutes,
                })
                .collect();
        }
    }
    view
}

fn story_minutes(entries: &[StoryMinutes]) -> Vec<ReportStoryMinutesView> {
    entries
        .iter()
        .map(|entry| ReportStoryMinutesView {
            story: entry.story.clone(),
            minutes: entry.minutes,
        })
        .collect()
}
