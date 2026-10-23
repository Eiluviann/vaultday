//! The close-out over a real workspace document and a temporary vault: the
//! FTE week with Toggl off and on, the allocation's guards, the ticks, the
//! closing line, the Monday alert, the hourly and fixed-price reviews and
//! the month. Nothing here talks to Toggl: queueing writes to the document.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_state::{
    AppStateError, DeviceId, InMemorySecretStore, PlainSyncFiles, SyncConfig, WorkspaceState,
    WorkspaceSync, create_workspace_key, workspace_key,
};
use close_out::{
    CloseOutContext, CloseOutError, CloseOutModel, LeaveCandidate, Missing, PLAIN_SINK, PeriodWork,
    StepKind, StepStatus, StoryFacts, TableFormatter, Touch, WEEK_CLOSED,
};
use domain::allocation::Allotment;
use domain::ports::{FixedClock, Notification, RecordingNotifier};
use domain::registry::{Enablement, Registry};
use domain::task::StoryNumber;
use domain::week::{DaysOff, Period};
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use jiff::civil::{Date, DateTime, date};
use jiff::tz::{Offset, TimeZone};
use vault_base::{
    BillingModel, BillingSettings, LeaveNote, LogDeadline, PlainFileAccess, SettingsWeekday,
    VaultWriter, Workspace, WorkspaceKind, WorkspaceSettings,
};
use vaultday_feature_toggl::{FlatDayStatus, TogglFeature, WorkItemFacts, WorkSession};

const WORKSPACE: &str = "ws-close-out";

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    sync: WorkspaceSync,
    settings: WorkspaceSettings,
}

fn fixture(model: Option<BillingModel>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    std::fs::create_dir_all(root.join(".vaultday")).unwrap();
    let mut settings = WorkspaceSettings::new("Mock FTE", WorkspaceKind::Work);
    settings.hours_per_day = 6.5;
    settings.billing = model.map(BillingSettings::new);
    if let Some(billing) = settings.billing.as_mut()
        && billing.model == BillingModel::Fte
    {
        billing.flat_hours = Some(8.0);
    }
    settings.log_deadline = Some(LogDeadline {
        weekday: SettingsWeekday::Monday,
        time: "08:00".to_owned(),
    });
    std::fs::write(
        root.join(".vaultday/settings.json"),
        settings.to_json().unwrap(),
    )
    .unwrap();
    let secrets = InMemorySecretStore::new();
    let id = WorkspaceId::new(WORKSPACE);
    if workspace_key(&secrets, &id).unwrap().is_none() {
        create_workspace_key(&secrets, &id).unwrap();
    }
    let sync = WorkspaceSync::create(
        Arc::new(PlainSyncFiles),
        &dir.path().join("state"),
        DeviceId::new("mac-a").unwrap(),
        workspace_key(&secrets, &id).unwrap().unwrap(),
        SyncConfig::default(),
    )
    .unwrap();
    Fixture {
        _dir: dir,
        root,
        sync,
        settings,
    }
}

fn story(number: &str) -> StoryNumber {
    StoryNumber::new(number)
}

/// The week of 19 October 2026 with the fixture's leave on Wednesday 21.
fn week() -> Period {
    Period::week_containing(date(2026, 10, 19)).unwrap()
}

fn leave() -> DaysOff {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/two-workspaces/Leave.md"),
    )
    .unwrap();
    close_out::days_off(&LeaveNote::parse(&source))
}

fn fte_work() -> PeriodWork {
    let facts = |title: &str, budget: Option<u32>| StoryFacts {
        title: title.to_owned(),
        budget_minutes: budget,
    };
    let touch = |day: i8, number: &str, tracked: u32| Touch {
        date: date(2026, 10, day),
        story: story(number),
        tracked_minutes: tracked,
    };
    PeriodWork {
        stories: BTreeMap::from([
            (story("WEB-142"), facts("Checkout redesign", Some(1800))),
            (story("WEB-157"), facts("Newsletter signup", Some(1200))),
            (story("ACME-9"), facts("Onboarding emails", Some(90))),
            (story("ACME-12"), facts("Invoice export", None)),
        ]),
        touched: vec![
            touch(19, "WEB-142", 185),
            touch(19, "WEB-157", 100),
            touch(19, "ACME-9", 75),
            touch(20, "WEB-142", 300),
            touch(22, "WEB-157", 0),
            touch(22, "ACME-12", 60),
            touch(23, "WEB-142", 90),
            touch(23, "ACME-9", 30),
        ],
        ..PeriodWork::default()
    }
}

fn context<'a>(
    settings: &'a WorkspaceSettings,
    period: Period,
    days_off: &'a DaysOff,
    work: &'a PeriodWork,
    toggl: Option<&'a TogglFeature>,
    draft: &'a BTreeMap<Date, Vec<Allotment>>,
) -> CloseOutContext<'a> {
    CloseOutContext {
        workspace: WORKSPACE,
        period,
        settings,
        days_off,
        work,
        toggl,
        toggl_tags: &[],
        draft,
    }
}

fn toggl_on() -> TogglFeature {
    let mut registry = Registry::new();
    vaultday_feature_toggl::register(&mut registry).unwrap();
    let mut enablement = Enablement::new();
    enablement.enable_for_app(vaultday_feature_toggl::FEATURE_ID);
    enablement.enable_for_workspace(WORKSPACE, vaultday_feature_toggl::FEATURE_ID);
    TogglFeature::for_workspace(&registry, &enablement, &WorkspaceId::new(WORKSPACE)).unwrap()
}

fn edit<R>(
    sync: &mut WorkspaceSync,
    action: impl FnOnce(&mut WorkspaceState) -> Result<R, CloseOutError>,
) -> Result<R, CloseOutError> {
    let mut failure = None;
    let result = sync.edit(Instant::now(), |state| {
        action(state).map_err(|error| {
            let message = error.to_string();
            failure = Some(error);
            AppStateError::InvalidSetting(message)
        })
    });
    result.map_err(|error| failure.unwrap_or(CloseOutError::AppState(error)))
}

fn local(day: Date, hour: i8, minute: i8) -> DateTime {
    day.at(hour, minute, 0, 0)
}

fn instant(at: DateTime) -> Timestamp {
    at.to_zoned(TimeZone::fixed(Offset::UTC))
        .unwrap()
        .timestamp()
}

fn start_of_day(day: Date) -> Result<Timestamp, vaultday_feature_toggl::TogglError> {
    Ok(instant(day.at(9, 0, 0, 0)))
}

#[test]
fn the_fte_week_runs_as_a_dry_run_while_toggl_is_off() {
    let fixture = fixture(Some(BillingModel::Fte));
    let settings_fixture = fixture.settings.clone();
    let (off, work, draft) = (leave(), fte_work(), BTreeMap::new());
    let ctx = context(&settings_fixture, week(), &off, &work, None, &draft);
    let close = close_out::close_out(fixture.sync.state(), &ctx).unwrap();

    assert_eq!(close.model, CloseOutModel::Fte);
    assert!(!close.is_toggl_on);
    assert_eq!(close.deadline, local(date(2026, 10, 26), 8, 0));
    let flat: Vec<(Date, FlatDayStatus, u64)> = close
        .flat
        .iter()
        .map(|line| (line.date, line.status, line.seconds))
        .collect();
    assert_eq!(
        flat,
        [
            (date(2026, 10, 19), FlatDayStatus::WillCreate, 8 * 3600),
            (date(2026, 10, 20), FlatDayStatus::WillCreate, 8 * 3600),
            (date(2026, 10, 21), FlatDayStatus::Leave, 0),
            (date(2026, 10, 22), FlatDayStatus::WillCreate, 8 * 3600),
            (date(2026, 10, 23), FlatDayStatus::WillCreate, 8 * 3600),
        ]
    );
    let statuses: Vec<(StepKind, StepStatus)> = close
        .steps
        .iter()
        .map(|step| (step.kind, step.status))
        .collect();
    assert_eq!(
        statuses,
        [
            (StepKind::Toggl, StepStatus::Skipped),
            (StepKind::Allocate, StepStatus::Open),
            (StepKind::Table, StepStatus::Open),
            (StepKind::Close, StepStatus::Open),
        ]
    );
    assert_eq!(close.steps[1].title, "Allocate 6.5 h per day");
    assert_eq!(
        close.missing,
        [
            Missing::AllocationNotConfirmed(4),
            Missing::RowsNotTicked(3),
            Missing::NotClosed,
        ]
    );

    // Monday: 390 minutes over the three stories touched, by remaining budget.
    let monday = &close.allocation[0].allocation;
    let minutes: Vec<(&str, u32)> = monday
        .allotments
        .iter()
        .map(|allotment| (allotment.story.as_str(), allotment.minutes))
        .collect();
    assert_eq!(
        minutes,
        [("ACME-9", 11), ("WEB-142", 227), ("WEB-157", 152)]
    );
    // Tuesday: only WEB-142 was touched; it alone gets the day.
    assert_eq!(close.allocation[1].allocation.total_minutes(), 390);
    assert_eq!(close.allocation[1].allocation.allotments.len(), 1);
    // Thursday: ACME-12 has no estimate, so only WEB-157 fills the day.
    let thursday = &close.allocation[2].allocation;
    assert_eq!(thursday.date, date(2026, 10, 22));
    assert_eq!(
        thursday
            .allotments
            .iter()
            .map(|a| (a.story.as_str(), a.minutes))
            .collect::<Vec<_>>(),
        [("ACME-12", 0), ("WEB-157", 390)]
    );
    // No allocation names an untouched story.
    for day in &close.allocation {
        for allotment in &day.allocation.allotments {
            assert!(day.tracked.contains_key(&allotment.story));
        }
    }
    // ACME-12 got nothing, so it has no row; Friday shares by what is left.
    assert_eq!(
        close.table_text,
        "Story\tTitle\t2026-10-19\t2026-10-20\t2026-10-22\t2026-10-23\tTotal\n\
         ACME-9\tOnboarding emails\t0.18\t\t\t0.40\t0.58\n\
         WEB-142\tCheckout redesign\t3.78\t6.50\t\t6.10\t16.38\n\
         WEB-157\tNewsletter signup\t2.53\t\t6.50\t\t9.03\n"
    );
}

#[test]
fn the_fte_week_closes_once_with_the_status_line() {
    let mut fixture = fixture(Some(BillingModel::Fte));
    let settings_fixture = fixture.settings.clone();
    let (off, work, draft) = (leave(), fte_work(), BTreeMap::new());
    let feature = toggl_on();

    // Step 1: the flat entries are queued, never sent.
    let ctx = context(
        &settings_fixture,
        week(),
        &off,
        &work,
        Some(&feature),
        &draft,
    );
    let before = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    assert_eq!(before.missing[0], Missing::FlatEntries(4));
    let queued = edit(&mut fixture.sync, |state| {
        close_out::queue_flat_entries(state, &ctx, &feature, &start_of_day)
    })
    .unwrap();
    assert_eq!(queued.len(), 4);
    assert!(!queued.contains(&date(2026, 10, 21)), "leave has no entry");
    let again = edit(&mut fixture.sync, |state| {
        close_out::queue_flat_entries(state, &ctx, &feature, &start_of_day)
    })
    .unwrap();
    assert!(again.is_empty(), "a second run queues nothing");
    let flat_days = feature.flat_days(fixture.sync.state());
    assert_eq!(flat_days.len(), 4);
    assert!(flat_days.iter().all(|item| item.toggl_entry_id.is_none()));

    // Closing too early is refused with what is missing.
    let writer_root = Workspace::open(&fixture.root, &PlainFileAccess).unwrap();
    let writer = VaultWriter::new(&PlainFileAccess, &writer_root);
    let now = local(date(2026, 10, 23), 16, 5);
    let early = edit(&mut fixture.sync, |state| {
        close_out::close(state, &writer, &ctx, now, instant(now))
    });
    assert!(
        matches!(early, Err(CloseOutError::NotReady(ref open)) if open.len() == 2),
        "{early:?}"
    );

    // Step 2: confirm the proposal.
    let proposal: BTreeMap<Date, Vec<Allotment>> = close_out::close_out(fixture.sync.state(), &ctx)
        .unwrap()
        .allocation
        .iter()
        .map(|day| (day.allocation.date, day.allocation.allotments.clone()))
        .collect();
    edit(&mut fixture.sync, |state| {
        close_out::confirm_allocation(state, &ctx, &proposal)
    })
    .unwrap();

    // Step 3: tick every row; the ticks are in the document.
    let close = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    for row in &close.table.rows {
        edit(&mut fixture.sync, |state| {
            close_out::set_row_ticked(state, &ctx.period, PLAIN_SINK, &row.key, true)
        })
        .unwrap();
    }
    let ready = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    assert!(ready.table.rows.iter().all(|row| row.is_ticked));
    assert_eq!(ready.missing, [Missing::NotClosed]);
    assert!(ready.can_close());

    // Step 4: exactly one status line in Friday's day log.
    let closed = edit(&mut fixture.sync, |state| {
        close_out::close(state, &writer, &ctx, now, instant(now))
    })
    .unwrap();
    assert_eq!(closed.date, date(2026, 10, 23));
    assert_eq!(
        closed.line.as_deref(),
        Some("- 16:05 status · week closed [kind:: status]")
    );
    let twice = edit(&mut fixture.sync, |state| {
        close_out::close(state, &writer, &ctx, now, instant(now))
    })
    .unwrap();
    assert_eq!(twice.line, None);
    let log = std::fs::read_to_string(fixture.root.join("Log/2026-10-23.md")).unwrap();
    assert_eq!(log.matches(WEEK_CLOSED).count(), 1, "{log}");
    assert!(
        log.ends_with("- 16:05 status · week closed [kind:: status]\n"),
        "{log}"
    );
    let done = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    assert!(done.missing.is_empty());
    assert!(
        done.steps
            .iter()
            .all(|step| step.status == StepStatus::Done)
    );
}

#[test]
fn the_allocation_never_invents_work_and_keeps_each_day_whole() {
    let mut fixture = fixture(Some(BillingModel::Fte));
    let settings_fixture = fixture.settings.clone();
    let (off, work) = (leave(), fte_work());
    let empty = BTreeMap::new();
    let ctx = context(&settings_fixture, week(), &off, &work, None, &empty);

    let untouched = BTreeMap::from([(
        date(2026, 10, 20),
        vec![Allotment {
            story: story("WEB-157"),
            minutes: 60,
        }],
    )]);
    let refused = edit(&mut fixture.sync, |state| {
        close_out::confirm_allocation(state, &ctx, &untouched)
    });
    assert!(
        matches!(refused, Err(CloseOutError::InvalidInput(ref m)) if m.contains("never adds work"))
    );
    let leave_day = BTreeMap::from([(date(2026, 10, 21), Vec::new())]);
    assert!(
        edit(&mut fixture.sync, |state| close_out::confirm_allocation(
            state, &ctx, &leave_day
        ))
        .is_err()
    );
    let too_long = BTreeMap::from([(
        date(2026, 10, 20),
        vec![Allotment {
            story: story("WEB-142"),
            minutes: 400,
        }],
    )]);
    assert!(
        edit(&mut fixture.sync, |state| close_out::confirm_allocation(
            state, &ctx, &too_long
        ))
        .is_err()
    );
    assert!(
        close_out::rebalance(
            fixture.sync.state(),
            &ctx,
            date(2026, 10, 20),
            &story("WEB-157"),
            10
        )
        .is_err()
    );

    // Monday: ACME-9 set to 60 minutes, the rest re-balanced to 390.
    let draft = close_out::rebalance(
        fixture.sync.state(),
        &ctx,
        date(2026, 10, 19),
        &story("ACME-9"),
        60,
    )
    .unwrap();
    let adjusted = context(&settings_fixture, week(), &off, &work, None, &draft);
    let close = close_out::close_out(fixture.sync.state(), &adjusted).unwrap();
    let monday = &close.allocation[0];
    assert!(monday.is_draft && !monday.is_confirmed);
    assert_eq!(monday.allocation.total_minutes(), 390);
    assert_eq!(
        monday
            .allocation
            .allotments
            .iter()
            .map(|a| (a.story.as_str(), a.minutes))
            .collect::<Vec<_>>(),
        [("ACME-9", 60), ("WEB-142", 198), ("WEB-157", 132)]
    );
    // Friday draws on what Monday left of ACME-9: 30 minutes.
    let friday = &close.allocation[3].allocation;
    let acme = friday
        .allotments
        .iter()
        .find(|a| a.story.as_str() == "ACME-9")
        .unwrap();
    assert!(acme.minutes <= 30);
}

#[test]
fn a_worked_story_without_an_estimate_is_flagged_on_its_day() {
    let fixture = fixture(Some(BillingModel::Fte));
    let settings_fixture = fixture.settings.clone();
    let (off, work, draft) = (leave(), fte_work(), BTreeMap::new());
    let ctx = context(&settings_fixture, week(), &off, &work, None, &draft);
    let close = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    // ACME-12 has no estimate: it gets no minutes, and the day says why.
    let thursday = close
        .allocation
        .iter()
        .find(|day| day.allocation.date == date(2026, 10, 22))
        .unwrap();
    assert_eq!(thursday.unestimated, BTreeSet::from([story("ACME-12")]));
    assert!(
        thursday
            .allocation
            .allotments
            .iter()
            .any(|a| a.story.as_str() == "ACME-12" && a.minutes == 0)
    );
    assert!(
        close
            .allocation
            .iter()
            .filter(|day| day.allocation.date != date(2026, 10, 22))
            .all(|day| day.unestimated.is_empty())
    );
}

#[test]
fn the_monday_alert_fires_once_per_week_and_only_for_what_is_missing() {
    let mut fixture = fixture(Some(BillingModel::Fte));
    let settings_fixture = fixture.settings.clone();
    let (off, work, draft) = (leave(), fte_work(), BTreeMap::new());
    let ctx = context(&settings_fixture, week(), &off, &work, None, &draft);
    let notifier = RecordingNotifier::new();
    let clock = |at: DateTime| FixedClock::new(instant(at), at.date());

    let friday = clock(local(date(2026, 10, 23), 16, 0));
    assert!(
        !edit(&mut fixture.sync, |state| close_out::run_alert(
            state, &ctx, &friday, &notifier
        ))
        .unwrap()
    );
    let early = clock(local(date(2026, 10, 26), 7, 29));
    assert!(
        !edit(&mut fixture.sync, |state| close_out::run_alert(
            state, &ctx, &early, &notifier
        ))
        .unwrap()
    );
    assert!(notifier.sent().is_empty());

    let monday = clock(local(date(2026, 10, 26), 7, 30));
    assert!(
        edit(&mut fixture.sync, |state| close_out::run_alert(
            state, &ctx, &monday, &notifier
        ))
        .unwrap()
    );
    let later = clock(local(date(2026, 10, 26), 9, 0));
    assert!(
        !edit(&mut fixture.sync, |state| close_out::run_alert(
            state, &ctx, &later, &notifier
        ))
        .unwrap()
    );
    assert_eq!(
        notifier.sent(),
        [Notification::CloseOutMissing {
            workspace: WORKSPACE.to_owned(),
            week_start: date(2026, 10, 19),
            missing: vec![
                "Allocation not confirmed for 4 days".to_owned(),
                "3 table rows not ticked".to_owned(),
                "The week is not closed".to_owned(),
            ],
        }]
    );
}

#[test]
fn a_closed_week_raises_no_alert() {
    let mut fixture = fixture(None);
    let settings_fixture = fixture.settings.clone();
    let off = DaysOff::default();
    let (work, draft) = (PeriodWork::default(), BTreeMap::new());
    let ctx = context(&settings_fixture, week(), &off, &work, None, &draft);
    let close = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    assert_eq!(
        close.steps[0].status,
        StepStatus::Skipped,
        "no billing, no Toggl"
    );
    assert!(close.flat.is_empty());
    let empty: BTreeMap<Date, Vec<Allotment>> = close
        .allocation
        .iter()
        .map(|day| (day.allocation.date, Vec::new()))
        .collect();
    edit(&mut fixture.sync, |state| {
        close_out::confirm_allocation(state, &ctx, &empty)
    })
    .unwrap();
    let workspace = Workspace::open(&fixture.root, &PlainFileAccess).unwrap();
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let monday = local(date(2026, 10, 26), 7, 45);
    let closed = edit(&mut fixture.sync, |state| {
        close_out::close(state, &writer, &ctx, monday, instant(monday))
    })
    .unwrap();
    assert_eq!(
        closed.line.as_deref(),
        Some("- 23:59 status · week closed [kind:: status]"),
        "closed after the week, the line ends its last day"
    );
    let notifier = RecordingNotifier::new();
    let clock = FixedClock::new(instant(monday), monday.date());
    assert!(
        !edit(&mut fixture.sync, |state| close_out::run_alert(
            state, &ctx, &clock, &notifier
        ))
        .unwrap()
    );
    assert!(notifier.sent().is_empty());
}

fn session(id: &str, day: i8, task: &str, minutes: u64) -> WorkSession {
    WorkSession {
        session: id.to_owned(),
        start: instant(local(date(2026, 10, day), 10, 0)),
        story: Some("WEB-142".to_owned()),
        task: Some(task.to_owned()),
        title: format!("Task {task}"),
        duration: Duration::from_secs(minutes * 60),
    }
}

#[test]
fn hourly_and_fixed_weeks_review_then_tabulate() {
    let mut hourly = fixture(Some(BillingModel::Hourly));
    let settings_hourly = hourly.settings.clone();
    let off = DaysOff::default();
    let draft = BTreeMap::new();
    let work = PeriodWork {
        sessions: vec![
            session("s1", 19, "t-1", 7),
            session("s2", 20, "t-1", 16),
            session("s3", 20, "t-2", 10),
        ],
        facts: BTreeMap::from([(
            "t-2".to_owned(),
            WorkItemFacts {
                is_billable: Some(false),
                ..WorkItemFacts::default()
            },
        )]),
        ..PeriodWork::default()
    };
    let feature = toggl_on();
    let ctx = context(
        &settings_hourly,
        week(),
        &off,
        &work,
        Some(&feature),
        &draft,
    );
    let close = close_out::close_out(hourly.sync.state(), &ctx).unwrap();
    assert_eq!(close.model, CloseOutModel::Hourly);
    assert_eq!(close.unqueued_sessions, ["s1", "s2", "s3"]);
    assert_eq!(close.missing[0], Missing::SessionsNotQueued(3));
    assert_eq!(close.missing[1], Missing::ReviewNotConfirmed);
    assert_eq!(
        close.table_text,
        "Story\tItem\tTracked\tBilled\nWEB-142\tTask t-1\t0.38\t0.75\n"
    );
    edit(&mut hourly.sync, |state| {
        close_out::confirm_review(
            state,
            &ctx.period,
            instant(local(date(2026, 10, 23), 15, 0)),
        )
    })
    .unwrap();
    let reviewed = close_out::close_out(hourly.sync.state(), &ctx).unwrap();
    assert_eq!(reviewed.steps[1].status, StepStatus::Done);

    let fixed = fixture(Some(BillingModel::Fixed));
    let settings_fixed = fixed.settings.clone();
    let fixed_work = PeriodWork {
        sessions: vec![session("s1", 19, "t-1", 40), session("s2", 20, "t-1", 40)],
        facts: BTreeMap::from([(
            "t-1".to_owned(),
            WorkItemFacts {
                estimate: Some(Duration::from_secs(60 * 60)),
                ..WorkItemFacts::default()
            },
        )]),
        ..PeriodWork::default()
    };
    let ctx = context(&settings_fixed, week(), &off, &fixed_work, None, &draft);
    let close = close_out::close_out(fixed.sync.state(), &ctx).unwrap();
    assert_eq!(close.steps[0].status, StepStatus::Skipped, "Toggl off");
    assert_eq!(close.fixed[0].overrun, Duration::from_secs(20 * 60));
    assert_eq!(
        close_out::PlainTsv.format(&close.table),
        "Story\tItem\tEstimate\tBilled\nWEB-142\tTask t-1\t1.00\t1.00\n"
    );
}

#[test]
fn the_month_closes_on_its_last_working_day() {
    let mut fixture = fixture(None);
    let settings_fixture = fixture.settings.clone();
    let off = leave();
    let (work, draft) = (PeriodWork::default(), BTreeMap::new());
    let month = Period::month_containing(date(2026, 12, 10)).unwrap();
    let ctx = context(&settings_fixture, month, &off, &work, None, &draft);
    let close = close_out::close_out(fixture.sync.state(), &ctx).unwrap();
    // 28 to 31 December are leave and the 25th a holiday: the 24th is last.
    assert_eq!(close.opens_at, local(date(2026, 12, 24), 14, 0));
    assert_eq!(close.table.columns, ["Story", "Title", "Total"]);
    let empty: BTreeMap<Date, Vec<Allotment>> = close
        .allocation
        .iter()
        .map(|day| (day.allocation.date, Vec::new()))
        .collect();
    edit(&mut fixture.sync, |state| {
        close_out::confirm_allocation(state, &ctx, &empty)
    })
    .unwrap();
    let workspace = Workspace::open(&fixture.root, &PlainFileAccess).unwrap();
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let at = local(date(2026, 12, 24), 15, 30);
    let closed = edit(&mut fixture.sync, |state| {
        close_out::close(state, &writer, &ctx, at, instant(at))
    })
    .unwrap();
    assert_eq!(
        closed.line.as_deref(),
        Some("- 15:30 status · month closed [kind:: status]")
    );
}

#[test]
fn the_leave_source_is_named_or_the_life_workspace() {
    let candidates = [
        LeaveCandidate {
            name: "Acme".to_owned(),
            kind: WorkspaceKind::Work,
            root: "/a".to_owned(),
        },
        LeaveCandidate {
            name: "Home".to_owned(),
            kind: WorkspaceKind::Life,
            root: "/h".to_owned(),
        },
        LeaveCandidate {
            name: "Family".to_owned(),
            kind: WorkspaceKind::Life,
            root: "/f".to_owned(),
        },
    ];
    let mut settings = WorkspaceSettings::new("Acme", WorkspaceKind::Work);
    assert_eq!(
        close_out::leave_source(&settings, &candidates)
            .unwrap()
            .root,
        "/h"
    );
    settings.leave_source = Some("Family".to_owned());
    assert_eq!(
        close_out::leave_source(&settings, &candidates)
            .unwrap()
            .root,
        "/f"
    );
    settings.leave_source = Some("Gone".to_owned());
    assert!(close_out::leave_source(&settings, &candidates).is_none());
}
