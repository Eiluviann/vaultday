//! The report bundle: what a person or Claude Code needs to write a status
//! report for a story, a client or a whole workspace over a date range. The
//! bundle is assembled here, once, so the app and the `vaultday` CLI hand out
//! the same content; nothing in it is produced by a model.
//!
//! - [`build_report_bundle`] reads the vault: frontmatter, Next Steps and
//!   Pending Input state, the day-log lines in range, a sessions summary and
//!   the hours per the workspace's billing model.
//! - [`ReportBundle::to_json`] is the canonical JSON; [`ReportBundle::to_markdown`]
//!   the readable form. Both are plain text, byte for byte stable.
//! - [`write_report`] stores the JSON beside the subject, in
//!   `_agent/reports/<from>-<to>.json`; that file is the only thing it writes.
//!
//! Time here is what the day log says was logged (`kind:: session` lines
//! ending in a duration). The timer's tracked time is private to the person
//! and lives outside the vault, so it cannot reach a bundle; a report of an
//! FTE workspace carries logged hours only, no rounding, no billable split.
//!
//! The authorship summary (model-text notes and words, model-written log
//! lines) is computed by the TypeScript provenance core, which the CLI runs.
//! The bundle carries the slot, `"authorship": null`, for the CLI to fill.

use std::fmt::Write as _;
use std::path::Path;

use domain::billing::{parse_billable_flag, parse_billing_minutes, round_up_minutes};
use serde_json::{Map, Value};

use crate::date::Date;
use crate::file_access::FileAccess;
use crate::log::VERB_SEPARATOR;
use crate::ops::{
    FrontmatterEntry, OpError, Report, ReportLogLine, StoryReport, TaskRecord, build_report,
    minutes_text,
};
use crate::settings::BillingModel;
use crate::vault::Vault;
use crate::workspace::Workspace;
use crate::writer::VaultWriter;

/// Minutes logged, per story; `None` is time that names no story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryMinutes {
    /// The story number.
    pub story: Option<String>,
    /// Minutes.
    pub minutes: u64,
}

/// The sessions in range, from the day log's `session` lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsSummary {
    /// Session lines in range.
    pub count: u32,
    /// Those whose text carries no duration, so they add no minutes.
    pub unmeasured_count: u32,
    /// Minutes the measured ones logged.
    pub logged_minutes: u64,
    /// Logged minutes per story, in order of first appearance.
    pub by_story: Vec<StoryMinutes>,
}

/// Logged time only: a workspace without a billing model, or an FTE one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedHours {
    /// Minutes logged.
    pub logged_minutes: u64,
    /// Per story.
    pub by_story: Vec<StoryMinutes>,
}

/// Hourly: billable time rounded up per session, non-billable apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyHours {
    /// The increment each session rounds up to.
    pub rounding_minutes: u32,
    /// Billable minutes after rounding.
    pub billable_minutes: u64,
    /// Non-billable minutes as logged.
    pub non_billable_minutes: u64,
    /// Billable minutes per story.
    pub billable_by_story: Vec<StoryMinutes>,
    /// Non-billable minutes per story.
    pub non_billable_by_story: Vec<StoryMinutes>,
}

/// One task of a fixed-price report: the quote against what was delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedTask {
    /// The story the task belongs to.
    pub story: Option<String>,
    /// The task id; `None` for time that names no task.
    pub task: Option<String>,
    /// The task's text.
    pub text: Option<String>,
    /// The estimate, which is the quote; `None` quotes nothing.
    pub quoted_minutes: Option<u64>,
    /// Minutes logged on the task.
    pub delivered_minutes: u64,
    /// Minutes beyond the quote, unpaid; everything when nothing is quoted.
    pub overrun_minutes: u64,
}

/// Fixed price per task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedHours {
    /// The quotes added up.
    pub quoted_minutes: u64,
    /// Everything logged.
    pub delivered_minutes: u64,
    /// The unpaid overruns added up.
    pub overrun_minutes: u64,
    /// Per task.
    pub tasks: Vec<FixedTask>,
}

/// The hours section, by billing model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportHours {
    /// The workspace has no billing model yet: logged time.
    Unbilled(LoggedHours),
    /// FTE: logged time, nothing billed.
    Fte(LoggedHours),
    /// Hourly billing.
    Hourly(HourlyHours),
    /// Fixed price per task.
    Fixed(FixedHours),
}

/// Everything a report holds.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportBundle {
    /// Stories, log lines and what the caller asked for.
    pub report: Report,
    /// The workspace's billing model, when it has one.
    pub billing: Option<BillingModel>,
    /// The sessions in range.
    pub sessions: SessionsSummary,
    /// Hours by the billing model.
    pub hours: ReportHours,
}

/// Where a written report landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenReport {
    /// The file, relative to the workspace root.
    pub relative_path: String,
    /// The file as an absolute path.
    pub absolute_path: String,
}

/// Day-log kinds in the order a report lists them; any other follows, then lines with no kind.
pub const LOG_KIND_ORDER: [&str; 6] = ["status", "decision", "session", "blocker", "done", "note"];

/// The clients a workspace-wide report can be asked for: the workspace itself
/// first (every story in it), then each project hub by its title, else its key.
#[must_use]
pub fn report_clients(workspace: &Workspace, vault: &Vault) -> Vec<String> {
    let mut clients = vec![workspace.settings.name.clone()];
    for (_, hub) in vault.projects() {
        if let Some(name) = hub.title.clone().or_else(|| hub.key.clone())
            && !clients.contains(&name)
        {
            clients.push(name);
        }
    }
    clients
}

/// Builds the bundle for a story number or a client over `from..=to`.
///
/// # Errors
/// As [`build_report`]: [`OpError::InvalidInput`] for a reversed range,
/// [`OpError::NotFound`] when the subject matches nothing.
pub fn build_report_bundle(
    workspace: &Workspace,
    vault: &Vault,
    subject: &str,
    from: Date,
    to: Date,
) -> Result<ReportBundle, OpError> {
    let report = build_report(workspace, vault, subject, from, to)?;
    let billing = workspace.settings.billing.as_ref();
    let sessions = session_facts(&report);
    let summary = summarise_sessions(&sessions);
    let hours = match billing.map(|billing| billing.model) {
        None => ReportHours::Unbilled(logged_hours(&sessions)),
        Some(BillingModel::Fte) => ReportHours::Fte(logged_hours(&sessions)),
        Some(BillingModel::Hourly) => {
            let rounding = billing.map_or(1, |billing| billing.rounding_minutes_or_default());
            let is_billable_by_default =
                billing.is_none_or(|billing| billing.billable_default_or_default());
            ReportHours::Hourly(hourly_hours(
                vault,
                &sessions,
                rounding,
                is_billable_by_default,
            ))
        }
        Some(BillingModel::Fixed) => ReportHours::Fixed(fixed_hours(&report, &sessions)),
    };
    Ok(ReportBundle {
        report,
        billing: billing.map(|billing| billing.model),
        sessions: summary,
        hours,
    })
}

/// Builds the bundle and writes its JSON to `_agent/reports/<from>-<to>.json`
/// beside the subject: the story's folder, the client's project folder or the
/// workspace root. Writes that one file and nothing else.
///
/// # Errors
/// The errors of [`build_report_bundle`]; [`OpError::InvalidInput`] when the
/// file may not go there, [`OpError::FileAccess`] when writing fails.
pub fn write_report(
    workspace: &Workspace,
    vault: &Vault,
    file_access: &dyn FileAccess,
    subject: &str,
    from: Date,
    to: Date,
) -> Result<WrittenReport, OpError> {
    let bundle = build_report_bundle(workspace, vault, subject, from, to)?;
    let relative = VaultWriter::new(file_access, workspace).write_report_file(
        Path::new(&bundle.report.home_folder),
        from,
        to,
        &bundle.to_json(),
    )?;
    let absolute = workspace.root.join(&relative);
    Ok(WrittenReport {
        relative_path: relative.to_string_lossy().into_owned(),
        absolute_path: absolute.to_string_lossy().into_owned(),
    })
}

impl ReportBundle {
    /// The canonical JSON: two-space indent, keys in the order of
    /// [`ReportBundle::to_value`], absent optional fields left out, a
    /// trailing newline. The CLI parses this, fills `authorship` and prints it
    /// the same way.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(&self.to_value()).unwrap_or_default();
        text.push('\n');
        text
    }

    /// The bundle as a JSON value.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let report = &self.report;
        let mut bundle = Map::new();
        bundle.insert("subject".into(), report.subject.clone().into());
        bundle.insert("subjectKind".into(), report.subject_kind.clone().into());
        bundle.insert("from".into(), report.from.clone().into());
        bundle.insert("to".into(), report.to.clone().into());
        bundle.insert(
            "billing".into(),
            self.billing
                .map_or(Value::Null, |model| model_name(model).into()),
        );
        bundle.insert(
            "stories".into(),
            report.stories.iter().map(story_value).collect(),
        );
        bundle.insert(
            "log".into(),
            report.log.iter().map(log_line_value).collect(),
        );
        bundle.insert("sessions".into(), sessions_value(&self.sessions));
        bundle.insert("hours".into(), hours_value(&self.hours));
        // Computed by the provenance core, which only the CLI runs.
        bundle.insert("authorship".into(), Value::Null);
        Value::Object(bundle)
    }

    /// The readable form, for pasting into a prompt or a document.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let report = &self.report;
        let mut text = String::new();
        let _ = writeln!(text, "# Report: {}\n", report.subject);
        let _ = writeln!(
            text,
            "{} · {} to {}\n",
            capitalised(&report.subject_kind),
            report.from,
            report.to
        );
        let _ = writeln!(text, "## Summary\n");
        let _ = writeln!(text, "- Stories: {}", report.stories.len());
        let _ = writeln!(text, "- Log lines: {}", report.log.len());
        let _ = writeln!(text, "- {}", sessions_line(&self.sessions));
        let _ = writeln!(
            text,
            "- Billing: {}\n",
            self.billing.map_or("not set", model_name)
        );
        let _ = writeln!(text, "## Stories");
        for story in &report.stories {
            story_markdown(&mut text, story);
        }
        let _ = writeln!(text, "\n## Log");
        log_markdown(&mut text, &report.log);
        let _ = writeln!(text, "\n## Hours\n");
        hours_markdown(&mut text, &self.hours);
        let _ = writeln!(text, "\n## Authorship\n");
        let _ = writeln!(text, "Computed by the CLI (vaultday report)");
        text
    }

    /// Log lines grouped by kind in [`LOG_KIND_ORDER`], then any other kind
    /// alphabetically, then lines with no kind (`None`).
    #[must_use]
    pub fn log_by_kind(&self) -> Vec<(Option<String>, Vec<&ReportLogLine>)> {
        group_log_by_kind(&self.report.log)
    }
}

// MARK: Sessions and hours

/// One session line in range, with the story it counts for.
struct SessionFact {
    story: Option<String>,
    task: Option<String>,
    minutes: Option<u64>,
}

fn session_facts(report: &Report) -> Vec<SessionFact> {
    let story_of_task: Vec<(&str, &str)> = report
        .stories
        .iter()
        .flat_map(|story| {
            story
                .next_steps
                .iter()
                .chain(&story.pending_input)
                .filter_map(|task| Some((task.id.as_deref()?, story.number.as_deref()?)))
        })
        .collect();
    report
        .log
        .iter()
        .filter(|line| line.kind.as_deref() == Some("session"))
        .map(|line| SessionFact {
            story: line.story.clone().or_else(|| {
                let task = line.task.as_deref()?;
                story_of_task
                    .iter()
                    .find(|(id, _)| *id == task)
                    .map(|(_, story)| (*story).to_owned())
            }),
            task: line.task.clone(),
            minutes: session_minutes(&line.text),
        })
        .collect()
}

/// The duration a session line ends with: `Subject · 1h 5m`.
fn session_minutes(text: &str) -> Option<u64> {
    let (_, tail) = text.rsplit_once(&format!(" {VERB_SEPARATOR} "))?;
    parse_billing_minutes(tail)
}

fn summarise_sessions(sessions: &[SessionFact]) -> SessionsSummary {
    let logged = logged_hours(sessions);
    SessionsSummary {
        count: u32::try_from(sessions.len()).unwrap_or(u32::MAX),
        unmeasured_count: u32::try_from(
            sessions
                .iter()
                .filter(|session| session.minutes.is_none())
                .count(),
        )
        .unwrap_or(u32::MAX),
        logged_minutes: logged.logged_minutes,
        by_story: logged.by_story,
    }
}

fn logged_hours(sessions: &[SessionFact]) -> LoggedHours {
    let mut by_story = Vec::new();
    for session in sessions {
        if let Some(minutes) = session.minutes {
            add_minutes(&mut by_story, session.story.as_deref(), minutes);
        }
    }
    LoggedHours {
        logged_minutes: sessions.iter().filter_map(|session| session.minutes).sum(),
        by_story,
    }
}

fn hourly_hours(
    vault: &Vault,
    sessions: &[SessionFact],
    rounding_minutes: u32,
    is_billable_by_default: bool,
) -> HourlyHours {
    let mut billable_by_story = Vec::new();
    let mut non_billable_by_story = Vec::new();
    for session in sessions {
        let Some(minutes) = session.minutes else {
            continue;
        };
        let is_billable = session
            .task
            .as_deref()
            .and_then(|id| vault.task_by_id(id))
            .and_then(|found| found.task.field("billable").and_then(parse_billable_flag))
            .unwrap_or(is_billable_by_default);
        if is_billable {
            add_minutes(
                &mut billable_by_story,
                session.story.as_deref(),
                round_up_minutes(minutes, rounding_minutes),
            );
        } else {
            add_minutes(
                &mut non_billable_by_story,
                session.story.as_deref(),
                minutes,
            );
        }
    }
    HourlyHours {
        rounding_minutes,
        billable_minutes: billable_by_story.iter().map(|entry| entry.minutes).sum(),
        non_billable_minutes: non_billable_by_story
            .iter()
            .map(|entry| entry.minutes)
            .sum(),
        billable_by_story,
        non_billable_by_story,
    }
}

fn fixed_hours(report: &Report, sessions: &[SessionFact]) -> FixedHours {
    let mut tasks: Vec<FixedTask> = Vec::new();
    for story in &report.stories {
        for record in story.next_steps.iter().chain(&story.pending_input) {
            let Some(id) = record.id.as_deref() else {
                continue;
            };
            let quoted = record.estimate.as_deref().and_then(parse_billing_minutes);
            let delivered = delivered_on(sessions, Some(id));
            if quoted.is_some() || delivered > 0 {
                tasks.push(fixed_task(
                    story.number.clone(),
                    Some(id.to_owned()),
                    Some(record.text.clone()),
                    quoted,
                    delivered,
                ));
            }
        }
    }
    // Time on a task this report's stories do not hold, or on no task.
    let mut leftover: Vec<Option<&str>> = Vec::new();
    for session in sessions.iter().filter(|session| session.minutes.is_some()) {
        let key = session.task.as_deref();
        let is_listed = tasks.iter().any(|task| task.task.as_deref() == key);
        if !is_listed && !leftover.contains(&key) {
            leftover.push(key);
        }
    }
    for key in leftover {
        let story = sessions
            .iter()
            .find(|session| session.task.as_deref() == key)
            .and_then(|session| session.story.clone());
        tasks.push(fixed_task(
            story,
            key.map(str::to_owned),
            None,
            None,
            delivered_on(sessions, key),
        ));
    }
    FixedHours {
        quoted_minutes: tasks.iter().filter_map(|task| task.quoted_minutes).sum(),
        delivered_minutes: tasks.iter().map(|task| task.delivered_minutes).sum(),
        overrun_minutes: tasks.iter().map(|task| task.overrun_minutes).sum(),
        tasks,
    }
}

fn fixed_task(
    story: Option<String>,
    task: Option<String>,
    text: Option<String>,
    quoted_minutes: Option<u64>,
    delivered_minutes: u64,
) -> FixedTask {
    FixedTask {
        story,
        task,
        text,
        quoted_minutes,
        delivered_minutes,
        overrun_minutes: delivered_minutes.saturating_sub(quoted_minutes.unwrap_or(0)),
    }
}

fn delivered_on(sessions: &[SessionFact], task: Option<&str>) -> u64 {
    sessions
        .iter()
        .filter(|session| session.task.as_deref() == task)
        .filter_map(|session| session.minutes)
        .sum()
}

fn add_minutes(entries: &mut Vec<StoryMinutes>, story: Option<&str>, minutes: u64) {
    match entries
        .iter_mut()
        .find(|entry| entry.story.as_deref() == story)
    {
        Some(entry) => entry.minutes += minutes,
        None => entries.push(StoryMinutes {
            story: story.map(str::to_owned),
            minutes,
        }),
    }
}

fn model_name(model: BillingModel) -> &'static str {
    match model {
        BillingModel::Hourly => "hourly",
        BillingModel::Fixed => "fixed",
        BillingModel::Fte => "fte",
    }
}

// MARK: JSON

fn story_value(story: &StoryReport) -> Value {
    let mut map = Map::new();
    map.insert("relativePath".into(), story.relative_path.clone().into());
    put(&mut map, "number", story.number.as_deref());
    put(&mut map, "title", story.title.as_deref());
    put(&mut map, "status", story.status.as_deref());
    map.insert("frontmatter".into(), frontmatter_value(&story.frontmatter));
    map.insert(
        "nextSteps".into(),
        story.next_steps.iter().map(task_value).collect(),
    );
    map.insert(
        "pendingInput".into(),
        story.pending_input.iter().map(task_value).collect(),
    );
    Value::Object(map)
}

fn frontmatter_value(entries: &[FrontmatterEntry]) -> Value {
    let mut map = Map::new();
    for entry in entries {
        map.insert(
            entry.key.clone(),
            entry.values.iter().cloned().map(Value::from).collect(),
        );
    }
    Value::Object(map)
}

fn task_value(task: &TaskRecord) -> Value {
    let mut map = Map::new();
    map.insert("relativePath".into(), task.relative_path.clone().into());
    put(&mut map, "storyNumber", task.story_number.as_deref());
    put(&mut map, "section", task.section.as_deref());
    map.insert("lineNumber".into(), task.line_number.into());
    put(&mut map, "id", task.id.as_deref());
    map.insert("text".into(), task.text.clone().into());
    map.insert("status".into(), task.status.clone().into());
    map.insert("depth".into(), task.depth.into());
    put(&mut map, "assignedTo", task.assigned_to.as_deref());
    put(&mut map, "created", task.created.as_deref());
    put(&mut map, "scheduled", task.scheduled.as_deref());
    put(&mut map, "due", task.due.as_deref());
    put(&mut map, "estimate", task.estimate.as_deref());
    put(&mut map, "completion", task.completion.as_deref());
    Value::Object(map)
}

fn log_line_value(line: &ReportLogLine) -> Value {
    let mut map = Map::new();
    map.insert("relativePath".into(), line.relative_path.clone().into());
    map.insert("date".into(), line.date.clone().into());
    map.insert("time".into(), line.time.clone().into());
    put(&mut map, "kind", line.kind.as_deref());
    map.insert("text".into(), line.text.clone().into());
    put(&mut map, "story", line.story.as_deref());
    put(&mut map, "project", line.project.as_deref());
    put(&mut map, "task", line.task.as_deref());
    put(&mut map, "agent", line.agent.as_deref());
    Value::Object(map)
}

fn sessions_value(sessions: &SessionsSummary) -> Value {
    let mut map = Map::new();
    map.insert("count".into(), sessions.count.into());
    map.insert("unmeasuredCount".into(), sessions.unmeasured_count.into());
    map.insert("loggedMinutes".into(), sessions.logged_minutes.into());
    map.insert("byStory".into(), story_minutes_value(&sessions.by_story));
    Value::Object(map)
}

fn hours_value(hours: &ReportHours) -> Value {
    let mut map = Map::new();
    match hours {
        ReportHours::Unbilled(logged) => {
            map.insert("model".into(), Value::Null);
            map.insert("loggedMinutes".into(), logged.logged_minutes.into());
            map.insert("byStory".into(), story_minutes_value(&logged.by_story));
        }
        ReportHours::Fte(logged) => {
            map.insert("model".into(), "fte".into());
            map.insert("loggedMinutes".into(), logged.logged_minutes.into());
            map.insert("byStory".into(), story_minutes_value(&logged.by_story));
        }
        ReportHours::Hourly(hourly) => {
            map.insert("model".into(), "hourly".into());
            map.insert("roundingMinutes".into(), hourly.rounding_minutes.into());
            map.insert("billableMinutes".into(), hourly.billable_minutes.into());
            map.insert(
                "nonBillableMinutes".into(),
                hourly.non_billable_minutes.into(),
            );
            map.insert(
                "billableByStory".into(),
                story_minutes_value(&hourly.billable_by_story),
            );
            map.insert(
                "nonBillableByStory".into(),
                story_minutes_value(&hourly.non_billable_by_story),
            );
        }
        ReportHours::Fixed(fixed) => {
            map.insert("model".into(), "fixed".into());
            map.insert("quotedMinutes".into(), fixed.quoted_minutes.into());
            map.insert("deliveredMinutes".into(), fixed.delivered_minutes.into());
            map.insert("overrunMinutes".into(), fixed.overrun_minutes.into());
            map.insert(
                "tasks".into(),
                fixed.tasks.iter().map(fixed_task_value).collect(),
            );
        }
    }
    Value::Object(map)
}

fn story_minutes_value(entries: &[StoryMinutes]) -> Value {
    entries
        .iter()
        .map(|entry| {
            let mut map = Map::new();
            map.insert(
                "story".into(),
                entry.story.clone().map_or(Value::Null, Value::from),
            );
            map.insert("minutes".into(), entry.minutes.into());
            Value::Object(map)
        })
        .collect()
}

fn fixed_task_value(task: &FixedTask) -> Value {
    let mut map = Map::new();
    put(&mut map, "story", task.story.as_deref());
    put(&mut map, "task", task.task.as_deref());
    put(&mut map, "text", task.text.as_deref());
    if let Some(quoted) = task.quoted_minutes {
        map.insert("quotedMinutes".into(), quoted.into());
    }
    map.insert("deliveredMinutes".into(), task.delivered_minutes.into());
    map.insert("overrunMinutes".into(), task.overrun_minutes.into());
    Value::Object(map)
}

fn put(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), value.into());
    }
}

// MARK: Markdown

fn story_markdown(text: &mut String, story: &StoryReport) {
    let heading = match (&story.number, &story.title) {
        (Some(number), Some(title)) => format!("{number} · {title}"),
        (Some(only), None) | (None, Some(only)) => only.clone(),
        (None, None) => story.relative_path.clone(),
    };
    let _ = writeln!(text, "\n### {heading}\n");
    let _ = writeln!(
        text,
        "- Status: {}",
        story.status.as_deref().unwrap_or("not set")
    );
    let _ = writeln!(text, "- Note: {}", story.relative_path);
    for entry in &story.frontmatter {
        if matches!(entry.key.as_str(), "type" | "number" | "status") || entry.values.is_empty() {
            continue;
        }
        let _ = writeln!(text, "- {}: {}", entry.key, entry.values.join(", "));
    }
    tasks_markdown(text, "Next Steps", &story.next_steps);
    tasks_markdown(text, "Pending Input", &story.pending_input);
}

fn tasks_markdown(text: &mut String, heading: &str, tasks: &[TaskRecord]) {
    let counts = ["open", "in-progress", "done", "cancelled", "other"]
        .iter()
        .filter_map(|status| {
            let count = tasks.iter().filter(|task| task.status == *status).count();
            (count > 0).then(|| format!("{count} {}", status.replace('-', " ")))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        text,
        "\n{heading} ({}{})\n",
        tasks.len(),
        if counts.is_empty() {
            String::new()
        } else {
            format!(": {counts}")
        }
    );
    for task in tasks {
        let mark = match task.status.as_str() {
            "open" => " ",
            "in-progress" => "/",
            "done" => "x",
            "cancelled" => "-",
            _ => "?",
        };
        let indent = "  ".repeat(usize::try_from(task.depth).unwrap_or(0));
        let mut details: Vec<String> = Vec::new();
        if let Some(id) = &task.id {
            details.push(id.clone());
        }
        if let Some(who) = &task.assigned_to {
            details.push(format!("with {who}"));
        }
        if let Some(estimate) = &task.estimate {
            details.push(format!("estimate {estimate}"));
        }
        if let Some(due) = &task.due {
            details.push(format!("due {due}"));
        }
        if let Some(day) = &task.completion {
            details.push(format!("done {day}"));
        }
        let suffix = if details.is_empty() {
            String::new()
        } else {
            format!(" ({})", details.join(" · "))
        };
        let _ = writeln!(text, "{indent}- [{mark}] {}{suffix}", task.text);
    }
}

fn group_log_by_kind(log: &[ReportLogLine]) -> Vec<(Option<String>, Vec<&ReportLogLine>)> {
    let mut kinds: Vec<&str> = log.iter().filter_map(|line| line.kind.as_deref()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds.sort_by_key(|kind| {
        LOG_KIND_ORDER
            .iter()
            .position(|known| known == kind)
            .unwrap_or(LOG_KIND_ORDER.len())
    });
    let mut groups: Vec<(Option<String>, Vec<&ReportLogLine>)> = kinds
        .into_iter()
        .map(|kind| {
            (
                Some(kind.to_owned()),
                log.iter()
                    .filter(|line| line.kind.as_deref() == Some(kind))
                    .collect(),
            )
        })
        .collect();
    let unkinded: Vec<&ReportLogLine> = log.iter().filter(|line| line.kind.is_none()).collect();
    if !unkinded.is_empty() {
        groups.push((None, unkinded));
    }
    groups
}

fn log_markdown(text: &mut String, log: &[ReportLogLine]) {
    if log.is_empty() {
        let _ = writeln!(text, "\nNo log lines in this range");
        return;
    }
    for (kind, lines) in group_log_by_kind(log) {
        let _ = writeln!(
            text,
            "\n### {} ({})\n",
            kind.as_deref().map_or("No kind".to_owned(), capitalised),
            lines.len()
        );
        for line in lines {
            let mut references: Vec<String> = Vec::new();
            references.extend(line.story.clone());
            references.extend(line.task.clone());
            references.extend(line.project.clone());
            references.extend(
                line.agent
                    .as_ref()
                    .map(|agent| format!("written by {agent}")),
            );
            let suffix = if references.is_empty() {
                String::new()
            } else {
                format!(" ({})", references.join(" · "))
            };
            let _ = writeln!(text, "- {} {} {}{suffix}", line.date, line.time, line.text);
        }
    }
}

fn sessions_line(sessions: &SessionsSummary) -> String {
    let unmeasured = if sessions.unmeasured_count > 0 {
        format!(", {} without a duration", sessions.unmeasured_count)
    } else {
        String::new()
    };
    format!(
        "Sessions: {} ({} logged{unmeasured})",
        sessions.count,
        minutes_text(sessions.logged_minutes)
    )
}

fn hours_markdown(text: &mut String, hours: &ReportHours) {
    match hours {
        ReportHours::Unbilled(logged) => {
            let _ = writeln!(text, "Logged: {}", minutes_text(logged.logged_minutes));
            story_minutes_markdown(text, &logged.by_story);
        }
        ReportHours::Fte(logged) => {
            let _ = writeln!(text, "FTE: logged hours only");
            let _ = writeln!(text, "\nLogged: {}", minutes_text(logged.logged_minutes));
            story_minutes_markdown(text, &logged.by_story);
        }
        ReportHours::Hourly(hourly) => {
            let _ = writeln!(
                text,
                "Hourly: sessions round up to {} min",
                hourly.rounding_minutes
            );
            let _ = writeln!(
                text,
                "\nBillable: {}",
                minutes_text(hourly.billable_minutes)
            );
            story_minutes_markdown(text, &hourly.billable_by_story);
            let _ = writeln!(
                text,
                "\nNon-billable: {}",
                minutes_text(hourly.non_billable_minutes)
            );
            story_minutes_markdown(text, &hourly.non_billable_by_story);
        }
        ReportHours::Fixed(fixed) => {
            let _ = writeln!(text, "Fixed price: the estimate is the quote");
            let _ = writeln!(
                text,
                "\nQuoted: {} · Delivered: {} · Overrun: {}",
                minutes_text(fixed.quoted_minutes),
                minutes_text(fixed.delivered_minutes),
                minutes_text(fixed.overrun_minutes)
            );
            if !fixed.tasks.is_empty() {
                let _ = writeln!(text);
            }
            for task in &fixed.tasks {
                let name = task
                    .text
                    .clone()
                    .or_else(|| task.task.clone())
                    .unwrap_or_else(|| "No task".to_owned());
                let quote = task
                    .quoted_minutes
                    .map_or("no quote".to_owned(), minutes_text);
                let _ = writeln!(
                    text,
                    "- {name}: quoted {quote}, delivered {}, overrun {}",
                    minutes_text(task.delivered_minutes),
                    minutes_text(task.overrun_minutes)
                );
            }
        }
    }
}

fn story_minutes_markdown(text: &mut String, entries: &[StoryMinutes]) {
    for entry in entries {
        let _ = writeln!(
            text,
            "- {}: {}",
            entry.story.as_deref().unwrap_or("No story"),
            minutes_text(entry.minutes)
        );
    }
}

fn capitalised(word: &str) -> String {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse_the_way_the_log_and_task_lines_write_them() {
        for (text, minutes) in [
            ("45m", Some(45)),
            ("0m", Some(0)),
            ("2h", Some(120)),
            ("1h 5m", Some(65)),
            ("1h30", Some(90)),
            ("1h30m", Some(90)),
            ("1.5h", Some(90)),
            ("90min", Some(90)),
            ("", None),
            ("soon", None),
            ("h", None),
            ("1.5h 5m", None),
        ] {
            assert_eq!(parse_billing_minutes(text), minutes, "{text:?}");
        }
    }

    #[test]
    fn a_session_line_gives_its_duration_only_after_the_separator() {
        assert_eq!(session_minutes("Wire the payment form · 25m"), Some(25));
        assert_eq!(session_minutes("Session on the payment form"), None);
        assert_eq!(session_minutes("A · B · 1h 5m"), Some(65));
    }
}
