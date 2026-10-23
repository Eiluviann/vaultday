//! What a close-out keeps in the workspace document, as records of its own
//! (`close-out`): each day's confirmed allocation by date, the review and the
//! closing of each period, and the ticked table rows. Days are kept by date,
//! not by period, so a month's close-out finds the days its weeks confirmed.

use std::collections::BTreeMap;

use app_state::{AppStateError, WorkspaceState};
use domain::allocation::Allotment;
use domain::registry::FeatureId;
use domain::task::StoryNumber;
use domain::week::Period;
use jiff::Timestamp;
use jiff::civil::Date;

/// The id the close-out's records are kept under.
pub const RECORDS: &str = "close-out";
const ALLOCATION: &str = "allocation";
const REVIEWED: &str = "reviewed";
const CLOSED: &str = "closed";
const TICKS: &str = "ticks";

fn records() -> FeatureId {
    FeatureId::new(RECORDS)
}

/// Every confirmed day, by date.
pub(crate) fn confirmed_days(state: &WorkspaceState) -> BTreeMap<Date, Vec<Allotment>> {
    state
        .feature_records(&records(), ALLOCATION)
        .into_iter()
        .filter_map(|(id, text)| Some((id.parse().ok()?, decode_allotments(&text)?)))
        .collect()
}

/// Stores one confirmed day, replacing what was there.
pub(crate) fn store_day(
    state: &mut WorkspaceState,
    date: Date,
    allotments: &[Allotment],
) -> Result<(), AppStateError> {
    let map: serde_json::Map<String, serde_json::Value> = allotments
        .iter()
        .map(|allotment| {
            (
                allotment.story.as_str().to_owned(),
                serde_json::Value::from(allotment.minutes),
            )
        })
        .collect();
    let text = serde_json::Value::Object(map).to_string();
    state.put_feature_record(&records(), ALLOCATION, &date.to_string(), &text)
}

/// When the period's review was confirmed.
pub(crate) fn reviewed_at(state: &WorkspaceState, period: &Period) -> Option<Timestamp> {
    timestamp(state, REVIEWED, &period.id())
}

pub(crate) fn store_reviewed(
    state: &mut WorkspaceState,
    period: &Period,
    at: Timestamp,
) -> Result<(), AppStateError> {
    state.put_feature_record(&records(), REVIEWED, &period.id(), &at.to_string())
}

/// When the period was closed.
pub(crate) fn closed_at(state: &WorkspaceState, period: &Period) -> Option<Timestamp> {
    timestamp(state, CLOSED, &period.id())
}

pub(crate) fn store_closed(
    state: &mut WorkspaceState,
    period: &Period,
    at: Timestamp,
) -> Result<(), AppStateError> {
    state.put_feature_record(&records(), CLOSED, &period.id(), &at.to_string())
}

/// Whether a row of a sink's table is ticked.
pub(crate) fn is_ticked(state: &WorkspaceState, period: &Period, sink: &str, key: &str) -> bool {
    state
        .feature_record(&records(), TICKS, &tick_id(period, sink, key))
        .is_some()
}

pub(crate) fn set_ticked(
    state: &mut WorkspaceState,
    period: &Period,
    sink: &str,
    key: &str,
    is_ticked: bool,
) -> Result<(), AppStateError> {
    let id = tick_id(period, sink, key);
    if is_ticked {
        state.put_feature_record(&records(), TICKS, &id, "1")
    } else {
        state.remove_feature_record(&records(), TICKS, &id)
    }
}

fn tick_id(period: &Period, sink: &str, key: &str) -> String {
    format!("{}|{sink}|{key}", period.id())
}

fn timestamp(state: &WorkspaceState, collection: &str, id: &str) -> Option<Timestamp> {
    state
        .feature_record(&records(), collection, id)?
        .parse()
        .ok()
}

fn decode_allotments(text: &str) -> Option<Vec<Allotment>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let map = value.as_object()?;
    map.iter()
        .map(|(story, minutes)| {
            Some(Allotment {
                story: StoryNumber::new(story.as_str()),
                minutes: u32::try_from(minutes.as_u64()?).ok()?,
            })
        })
        .collect()
}
