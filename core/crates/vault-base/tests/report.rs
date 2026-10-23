//! The report bundle, its ranges, its hours per billing model, the FTE privacy
//! rule and the one file a report writes; all on a sandbox copy of the mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{Sandbox, diff, sandbox, snapshot, workspace, write};
use serde_json::Value;
use vault_base::ops::{OpError, SessionLogRequest, append_session_log, open_vault};
use vault_base::report::{ReportBundle, ReportHours, build_report_bundle, write_report};
use vault_base::writer::{VaultWriter, WriteError};
use vault_base::{Date, PlainFileAccess, Time};

const GOLDEN_FOLDER: &str = "../../fixtures/golden";
const WEB_142_FOLDER: &str = "Projects/Website/WEB-142 Checkout redesign";

fn date(text: &str) -> Date {
    Date::parse(text).unwrap()
}

fn bundle_in(sandbox: &Sandbox, subject: &str, from: &str, to: &str) -> ReportBundle {
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    build_report_bundle(&workspace, &vault, subject, date(from), date(to)).unwrap()
}

fn bundle(subject: &str, from: &str, to: &str) -> ReportBundle {
    bundle_in(&sandbox(), subject, from, to)
}

fn set_billing(sandbox: &Sandbox, billing: &str) {
    let path = sandbox.root.join(".vaultday/settings.json");
    let mut settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    settings["billing"] = serde_json::from_str(billing).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();
}

fn log_session(
    sandbox: &Sandbox,
    id: &str,
    day: &str,
    story: &str,
    task: Option<&str>,
    minutes: u64,
) {
    let workspace = workspace(&sandbox.root);
    append_session_log(
        &workspace,
        &PlainFileAccess,
        &SessionLogRequest {
            date: date(day),
            time: Time::new(15, 0).unwrap(),
            session: id.to_owned(),
            task: task.map(str::to_owned),
            story: Some(story.to_owned()),
            fallback_text: story.to_owned(),
            minutes,
        },
    )
    .unwrap();
}

/// WEB-142 with three sessions: 25 and 50 minutes on task-2 (estimate 2h) and 80 on task-4 (estimate 45m).
fn sandbox_with_sessions(billing: Option<&str>) -> Sandbox {
    let sandbox = sandbox();
    if let Some(billing) = billing {
        set_billing(&sandbox, billing);
    }
    log_session(&sandbox, "s1", "2026-10-21", "WEB-142", Some("task-2"), 25);
    log_session(&sandbox, "s2", "2026-10-22", "WEB-142", Some("task-2"), 50);
    log_session(&sandbox, "s3", "2026-10-22", "WEB-142", Some("task-4"), 80);
    sandbox
}

fn golden(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_FOLDER)
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        return String::new();
    }
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing golden {}", path.display()))
}

fn assert_golden(name: &str, actual: &str) {
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(GOLDEN_FOLDER)
            .join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, actual).unwrap();
        return;
    }
    assert_eq!(
        actual,
        golden(name),
        "{name} differs; rerun with UPDATE_GOLDEN=1 to refresh it"
    );
}

#[test]
fn a_story_bundle_matches_its_golden_json_with_the_authorship_slot_empty() {
    let bundle = bundle("WEB-142", "2026-10-21", "2026-10-22");
    assert_golden("report-web-142.json", &bundle.to_json());
    let value = bundle.to_value();
    assert_eq!(value["authorship"], Value::Null);
    assert_eq!(value["subjectKind"], "story");
    assert_eq!(value["stories"].as_array().unwrap().len(), 1);
}

#[test]
fn a_story_bundle_renders_to_the_markdown_byte_for_byte() {
    let bundle = bundle("WEB-142", "2026-10-21", "2026-10-22");
    assert_golden("report-web-142.md", &bundle.to_markdown());
    let client = self::bundle("Website", "2026-10-21", "2026-10-22");
    assert_golden("report-website.md", &client.to_markdown());
}

#[test]
fn a_client_bundle_covers_its_stories_and_every_event_kind_grouped_in_order() {
    let bundle = bundle("Website", "2026-10-21", "2026-10-22");
    assert_eq!(bundle.report.subject_kind, "client");
    assert_eq!(bundle.report.stories.len(), 2);
    let kinds: Vec<Option<String>> = bundle
        .log_by_kind()
        .into_iter()
        .map(|(kind, _)| kind)
        .collect();
    let expected: Vec<Option<String>> =
        ["status", "decision", "session", "blocker", "done", "note"]
            .iter()
            .map(|kind| Some((*kind).to_owned()))
            .collect();
    assert_eq!(kinds, expected);
    let grouped: usize = bundle
        .log_by_kind()
        .iter()
        .map(|(_, lines)| lines.len())
        .sum();
    assert_eq!(grouped, bundle.report.log.len());
}

#[test]
fn the_range_limits_the_log_and_an_empty_range_says_so() {
    let one_day = bundle("Website", "2026-10-22", "2026-10-22");
    assert!(
        one_day
            .report
            .log
            .iter()
            .all(|line| line.date == "2026-10-22")
    );
    let quiet = bundle("WEB-142", "2026-09-01", "2026-09-07");
    assert!(quiet.report.log.is_empty());
    assert!(quiet.to_markdown().contains("No log lines in this range"));
    assert_eq!(quiet.sessions.count, 0);
}

#[test]
fn a_reversed_range_and_an_unknown_subject_are_refused() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let reversed = build_report_bundle(
        &workspace,
        &vault,
        "WEB-142",
        date("2026-10-22"),
        date("2026-10-21"),
    );
    assert!(matches!(reversed.unwrap_err(), OpError::InvalidInput(_)));
    let unknown = build_report_bundle(
        &workspace,
        &vault,
        "Nobody",
        date("2026-10-21"),
        date("2026-10-22"),
    );
    assert!(matches!(unknown.unwrap_err(), OpError::NotFound(_)));
}

#[test]
fn without_a_billing_model_hours_are_logged_time_only() {
    let sandbox = sandbox_with_sessions(None);
    let bundle = bundle_in(&sandbox, "WEB-142", "2026-10-21", "2026-10-22");
    let ReportHours::Unbilled(logged) = &bundle.hours else {
        panic!("expected logged hours, got {:?}", bundle.hours);
    };
    assert_eq!(logged.logged_minutes, 155);
    assert_eq!(bundle.billing, None);
    // The mock vault's two session lines carry no duration.
    assert_eq!(bundle.sessions.count, 5);
    assert_eq!(bundle.sessions.unmeasured_count, 2);
    assert_eq!(bundle.sessions.logged_minutes, 155);
}

#[test]
fn an_hourly_report_rounds_billable_sessions_up_and_keeps_non_billable_apart() {
    let sandbox = sandbox();
    set_billing(&sandbox, r#"{"model": "hourly", "rounding_minutes": 30}"#);
    // task-4 is not billable.
    let note = sandbox.root.join(common::WEB_142);
    let text = std::fs::read_to_string(&note).unwrap();
    std::fs::write(
        &note,
        text.replace("[estimate:: 45m]", "[estimate:: 45m] [billable:: false]"),
    )
    .unwrap();
    log_session(&sandbox, "s1", "2026-10-21", "WEB-142", Some("task-2"), 25);
    log_session(&sandbox, "s2", "2026-10-22", "WEB-142", Some("task-2"), 50);
    log_session(&sandbox, "s3", "2026-10-22", "WEB-142", Some("task-4"), 80);
    let bundle = bundle_in(&sandbox, "WEB-142", "2026-10-21", "2026-10-22");
    let ReportHours::Hourly(hourly) = &bundle.hours else {
        panic!("expected hourly hours, got {:?}", bundle.hours);
    };
    // 25 -> 30 and 50 -> 60; 80 is not billable and stays as logged.
    assert_eq!(hourly.billable_minutes, 90);
    assert_eq!(hourly.non_billable_minutes, 80);
    assert_eq!(
        hourly.billable_by_story[0].story.as_deref(),
        Some("WEB-142")
    );
    assert_eq!(hourly.non_billable_by_story[0].minutes, 80);
    let json = bundle.to_value();
    assert_eq!(json["hours"]["model"], "hourly");
    assert_eq!(json["billing"], "hourly");
    assert!(bundle.to_markdown().contains("Billable: 1h 30m"));
    assert!(bundle.to_markdown().contains("Non-billable: 1h 20m"));
}

#[test]
fn a_fixed_price_report_sets_the_quote_against_what_was_delivered() {
    let sandbox = sandbox_with_sessions(Some(r#"{"model": "fixed"}"#));
    let bundle = bundle_in(&sandbox, "WEB-142", "2026-10-21", "2026-10-22");
    let ReportHours::Fixed(fixed) = &bundle.hours else {
        panic!("expected fixed hours, got {:?}", bundle.hours);
    };
    let task = |id: &str| {
        fixed
            .tasks
            .iter()
            .find(|task| task.task.as_deref() == Some(id))
            .unwrap()
    };
    // task-2: quoted 2h, delivered 75m, inside the quote.
    assert_eq!(task("task-2").quoted_minutes, Some(120));
    assert_eq!(task("task-2").delivered_minutes, 75);
    assert_eq!(task("task-2").overrun_minutes, 0);
    // task-4: quoted 45m, delivered 80m, 35m unpaid.
    assert_eq!(task("task-4").quoted_minutes, Some(45));
    assert_eq!(task("task-4").overrun_minutes, 35);
    assert_eq!(fixed.quoted_minutes, 165);
    assert_eq!(fixed.delivered_minutes, 155);
    assert_eq!(fixed.overrun_minutes, 35);
    assert!(
        bundle
            .to_markdown()
            .contains("quoted 45m, delivered 1h 20m, overrun 35m")
    );
}

#[test]
fn an_fte_report_carries_logged_hours_and_never_tracked_time_or_a_money_section() {
    let sandbox = sandbox_with_sessions(Some(r#"{"model": "fte"}"#));
    for subject in ["WEB-142", "Website", "Mock Studio"] {
        let bundle = bundle_in(&sandbox, subject, "2026-10-01", "2026-10-31");
        let ReportHours::Fte(logged) = &bundle.hours else {
            panic!("expected FTE hours, got {:?}", bundle.hours);
        };
        assert_eq!(logged.logged_minutes, 155);
        let json = bundle.to_json();
        let markdown = bundle.to_markdown();
        for text in [&json, &markdown] {
            let lower = text.to_lowercase();
            for forbidden in [
                "tracked",
                "billable",
                "quoted",
                "overrun",
                "currency",
                "rounding_minutes",
                "roundingminutes",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "{subject}: the FTE report mentions {forbidden}"
                );
            }
        }
        let hours = bundle.to_value()["hours"].clone();
        let keys: Vec<&str> = hours
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["model", "loggedMinutes", "byStory"]);
    }
}

#[test]
fn no_bundle_of_any_billing_model_carries_tracked_time() {
    for billing in [
        None,
        Some(r#"{"model": "hourly"}"#),
        Some(r#"{"model": "fixed"}"#),
        Some(r#"{"model": "fte"}"#),
    ] {
        let sandbox = sandbox_with_sessions(billing);
        let bundle = bundle_in(&sandbox, "Mock Studio", "2026-10-01", "2026-10-31");
        assert!(
            !bundle.to_json().to_lowercase().contains("tracked"),
            "{billing:?}"
        );
        assert!(
            !bundle.to_markdown().to_lowercase().contains("tracked"),
            "{billing:?}"
        );
    }
}

#[test]
fn writing_a_report_creates_exactly_one_file_beside_the_story() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let before = snapshot(&sandbox.root);
    let written = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "WEB-142",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    let expected = format!("{WEB_142_FOLDER}/_agent/reports/2026-10-21-2026-10-22.json");
    assert_eq!(written.relative_path, expected);
    assert_eq!(
        PathBuf::from(&written.absolute_path),
        sandbox.root.join(&expected)
    );
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].path, PathBuf::from(&expected));
    assert!(changes[0].removed.is_empty());
    let on_disk = std::fs::read_to_string(&written.absolute_path).unwrap();
    assert_eq!(
        on_disk,
        bundle_in(&sandbox, "WEB-142", "2026-10-21", "2026-10-22").to_json()
    );
    assert!(!written.relative_path.starts_with("Log/"));
}

#[test]
fn writing_a_report_again_replaces_that_file_and_touches_no_other() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "WEB-142",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    let before = snapshot(&sandbox.root);
    write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "WEB-142",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

#[test]
fn a_client_and_the_workspace_report_beside_their_own_folders() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let client = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "Website",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    assert_eq!(
        client.relative_path,
        "Projects/Website/_agent/reports/2026-10-21-2026-10-22.json"
    );
    let whole = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "Mock Studio",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    assert_eq!(
        whole.relative_path,
        "_agent/reports/2026-10-21-2026-10-22.json"
    );
}

#[test]
fn building_a_bundle_or_failing_to_write_one_changes_nothing() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let before = snapshot(&sandbox.root);
    let _ = build_report_bundle(
        &workspace,
        &vault,
        "WEB-142",
        date("2026-10-21"),
        date("2026-10-22"),
    )
    .unwrap();
    let reversed = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "WEB-142",
        date("2026-10-22"),
        date("2026-10-21"),
    );
    assert!(matches!(reversed.unwrap_err(), OpError::InvalidInput(_)));
    let unknown = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "Nobody",
        date("2026-10-21"),
        date("2026-10-22"),
    );
    assert!(matches!(unknown.unwrap_err(), OpError::NotFound(_)));
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

#[test]
fn a_report_folder_that_leaves_the_workspace_is_refused_and_nothing_is_written() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let outside = sandbox.root.parent().unwrap().join("elsewhere");
    for folder in [
        Path::new("../elsewhere"),
        Path::new("Projects/../../elsewhere"),
        outside.as_path(),
    ] {
        let refused =
            writer.write_report_file(folder, date("2026-10-21"), date("2026-10-22"), "{}\n");
        assert!(
            matches!(refused, Err(WriteError::OutsideWorkspace(_))),
            "{folder:?}: {refused:?}"
        );
    }
    assert!(!outside.exists());
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

#[test]
fn a_report_is_never_written_inside_the_day_log_folder() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    // A story that lives inside the log folder would otherwise get its report there.
    write(
        &sandbox.root,
        "Log/Odd story/Odd story.md",
        "---\ntype: story\nnumber: WEB-900\nstatus: draft\n---\n# Odd story\n\n## Next Steps\n- [ ] x  [id:: task-900]\n",
    );
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let before = snapshot(&sandbox.root);
    let refused = write_report(
        &workspace,
        &vault,
        &PlainFileAccess,
        "WEB-900",
        date("2026-10-21"),
        date("2026-10-22"),
    );
    assert!(matches!(refused.unwrap_err(), OpError::InvalidInput(_)));
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

#[test]
fn the_clients_a_report_can_be_asked_for_are_the_workspace_then_each_project() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let clients = vault_base::report::report_clients(&workspace, &vault);
    assert_eq!(clients.first().map(String::as_str), Some("Mock Studio"));
    assert!(clients.iter().any(|name| name == "Website"));
    for client in &clients {
        assert!(
            build_report_bundle(
                &workspace,
                &vault,
                client,
                date("2026-10-21"),
                date("2026-10-22")
            )
            .is_ok(),
            "{client} is asked for but not found"
        );
    }
}
