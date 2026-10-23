//! The FTE flat week against the mock server: one entry per working day,
//! leave and holidays left out, idempotent by date.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use jiff::Timestamp;
use jiff::civil::{Date, date};
use support::{Fault, MockToggl, create_workspace, feature, mapping, secrets_with_token};
use vaultday_feature_toggl::{FlatDayStatus, FlatJobInput, TogglError, WeekDayStatus};

fn nine_utc(day: Date) -> Result<Timestamp, TogglError> {
    day.at(9, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| TogglError::InvalidInput(error.to_string()))
}

fn week() -> FlatJobInput {
    let mut input = FlatJobInput::weekdays(date(2026, 10, 19), date(2026, 10, 25), 8.0);
    input.leave.push(date(2026, 10, 20));
    input.holidays.push(date(2026, 10, 21));
    input.tags.push("fte".to_owned());
    input
}

#[test]
fn the_dry_run_changes_nothing_and_the_run_is_idempotent_by_date() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let feature = feature();
    let preview = feature.preview_flat_job(sync.state(), &week()).unwrap();
    let statuses: Vec<FlatDayStatus> = preview.iter().map(|day| day.status).collect();
    assert_eq!(
        statuses,
        [
            FlatDayStatus::WillCreate,
            FlatDayStatus::Leave,
            FlatDayStatus::Holiday,
            FlatDayStatus::WillCreate,
            FlatDayStatus::WillCreate,
            FlatDayStatus::NotWorkingDay,
            FlatDayStatus::NotWorkingDay,
        ]
    );
    assert!(
        feature.flat_days(sync.state()).is_empty(),
        "a dry run writes nothing"
    );

    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let now: Timestamp = "2026-10-23T16:00:00Z".parse().unwrap();
    let report = feature
        .run_flat_job(&mut sync, &api, &mapping(), &week(), &nine_utc, now)
        .unwrap();
    assert_eq!((report.queued, report.drain.sent), (3, 3));
    let entries = mock.entries();
    assert_eq!(entries.len(), 3);
    for entry in &entries {
        assert_eq!(entry["duration"], 8 * 3600);
        assert_eq!(entry["description"], "");
        assert_eq!(entry["project_id"], 502, "the default project");
        assert_eq!(entry["billable"], false);
        assert_eq!(entry["tags"], serde_json::json!(["fte"]));
    }
    assert_eq!(entries[0]["start"], "2026-10-19T09:00:00Z");

    let second = feature
        .run_flat_job(&mut sync, &api, &mapping(), &week(), &nine_utc, now)
        .unwrap();
    assert_eq!((second.queued, second.drain.sent), (0, 0));
    assert_eq!(mock.entries().len(), 3, "a second run creates nothing");
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 3);
    let sent: Vec<FlatDayStatus> = second.days.iter().map(|day| day.status).collect();
    assert_eq!(sent[0], FlatDayStatus::Sent);
    assert_eq!(sent[1], FlatDayStatus::Leave);
}

#[test]
fn a_failed_day_is_retried_by_the_next_run_without_a_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let feature = feature();
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let input = FlatJobInput::weekdays(date(2026, 10, 19), date(2026, 10, 19), 8.0);
    mock.fail_next("POST", Fault::ServerErrorAfterCommit);
    let first = feature
        .run_flat_job(
            &mut sync,
            &api,
            &mapping(),
            &input,
            &nine_utc,
            "2026-10-23T16:00:00Z".parse().unwrap(),
        )
        .unwrap();
    assert_eq!(first.drain.retrying, 1);
    assert_eq!(first.days[0].status, FlatDayStatus::Queued);
    let second = feature
        .run_flat_job(
            &mut sync,
            &api,
            &mapping(),
            &input,
            &nine_utc,
            "2026-10-23T17:00:00Z".parse().unwrap(),
        )
        .unwrap();
    assert_eq!((second.queued, second.drain.sent), (0, 1));
    assert_eq!(mock.entries().len(), 1);
}

#[test]
fn the_week_view_reads_the_queue_and_the_flat_days() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let feature = feature();
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let mut input = week();
    input.to = date(2026, 10, 22);
    feature
        .run_flat_job(
            &mut sync,
            &api,
            &mapping(),
            &input,
            &nine_utc,
            "2026-10-22T16:00:00Z".parse().unwrap(),
        )
        .unwrap();
    // Friday is queued but not sent: no mapping yet.
    let friday = FlatJobInput::weekdays(date(2026, 10, 23), date(2026, 10, 23), 8.0);
    feature
        .run_flat_job(
            &mut sync,
            &api,
            &vaultday_feature_toggl::TogglMapping::default(),
            &friday,
            &nine_utc,
            "2026-10-23T16:00:00Z".parse().unwrap(),
        )
        .unwrap();
    let day_of = |at: Timestamp| Some(at.to_zoned(jiff::tz::TimeZone::UTC).date());
    let days = feature.week(sync.state(), date(2026, 10, 19), None, &day_of);
    let shape: Vec<(WeekDayStatus, u64, bool)> = days
        .iter()
        .map(|day| (day.status, day.sent_secs + day.queued_secs, day.is_flat))
        .collect();
    assert_eq!(
        shape,
        [
            (WeekDayStatus::Sent, 8 * 3600, true),
            (WeekDayStatus::Empty, 0, false),
            (WeekDayStatus::Empty, 0, false),
            (WeekDayStatus::Sent, 8 * 3600, true),
            (WeekDayStatus::Queued, 8 * 3600, true),
            (WeekDayStatus::Empty, 0, false),
            (WeekDayStatus::Empty, 0, false),
        ]
    );
}
