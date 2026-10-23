//! The Toggl Track API v9 as a trait, and the plain records it speaks in.
//! [`crate::HttpTogglApi`] implements it; tests run it against a mock server.
//! No record carries money: Toggl gets durations, projects and tags only.

use jiff::Timestamp;

use crate::error::TogglError;

/// The signed-in Toggl user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglMe {
    /// The user's Toggl id.
    pub id: u64,
    /// The user's full name, when set.
    pub full_name: Option<String>,
    /// The Toggl workspace new entries go to by default.
    pub default_workspace_id: Option<u64>,
}

/// A Toggl workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglWorkspace {
    /// Its id.
    pub id: u64,
    /// Its name.
    pub name: String,
}

/// A Toggl project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglProject {
    /// Its id.
    pub id: u64,
    /// The Toggl workspace it is in.
    pub workspace_id: u64,
    /// Its name.
    pub name: String,
    /// The client it belongs to, when any.
    pub client_id: Option<u64>,
    /// Whether it is active.
    pub is_active: bool,
}

/// A time entry Vaultday asks Toggl to create or to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTimeEntry {
    /// The Toggl workspace.
    pub workspace_id: u64,
    /// The Toggl project, when mapped.
    pub project_id: Option<u64>,
    /// The description: the task's text, or empty for a flat FTE day.
    pub description: String,
    /// When the time starts.
    pub start: Timestamp,
    /// How long it lasts, in whole seconds.
    pub duration_secs: u64,
    /// Tags from the workspace.
    pub tags: Vec<String>,
    /// Whether Toggl marks it billable.
    pub is_billable: bool,
}

/// A time entry as Toggl holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeEntry {
    /// Its Toggl id.
    pub id: u64,
    /// The Toggl workspace.
    pub workspace_id: u64,
    /// The Toggl project, when any.
    pub project_id: Option<u64>,
    /// The description.
    pub description: String,
    /// When it starts.
    pub start: Timestamp,
    /// How long it lasts in seconds; negative while a Toggl timer runs.
    pub duration_secs: i64,
    /// Its tags.
    pub tags: Vec<String>,
    /// Whether it is billable.
    pub is_billable: bool,
}

/// The endpoints Vaultday uses. Implementations block on the network.
pub trait TogglApi {
    /// `GET /me`.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn me(&self) -> Result<TogglMe, TogglError>;

    /// `GET /me/workspaces`.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn workspaces(&self) -> Result<Vec<TogglWorkspace>, TogglError>;

    /// `GET /workspaces/{workspace_id}/projects`.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn projects(&self, workspace_id: u64) -> Result<Vec<TogglProject>, TogglError>;

    /// `POST /workspaces/{workspace_id}/time_entries`. Never retried on a
    /// server failure by the client itself: the entry may exist already, so
    /// the queue looks before it posts again.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn create_time_entry(&self, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError>;

    /// `PUT /workspaces/{workspace_id}/time_entries/{id}`.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn update_time_entry(&self, id: u64, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError>;

    /// `DELETE /workspaces/{workspace_id}/time_entries/{id}`; an entry that is
    /// already gone counts as deleted.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn delete_time_entry(&self, workspace_id: u64, id: u64) -> Result<(), TogglError>;

    /// `GET /me/time_entries?start_date=…&end_date=…`: the user's entries
    /// starting in `[start, end)`.
    ///
    /// # Errors
    /// A [`TogglError`] from the service or the network.
    fn time_entries(&self, start: Timestamp, end: Timestamp) -> Result<Vec<TimeEntry>, TogglError>;
}

impl TimeEntry {
    /// Whether this entry is the one `wanted` describes: same Toggl
    /// workspace, start to the second and duration. How the queue recognises
    /// an entry a failed attempt may have created after all. The key holds
    /// only values Vaultday chose and Toggl returns as sent; description and
    /// project are left out, since the user may edit them in Toggl and the
    /// project mapping may change between attempts.
    ///
    /// The key misses when the user edits the entry's start or duration in
    /// Toggl after a post whose answer was lost and before the retry: the
    /// lookup finds nothing and the retry posts a second entry. Vaultday
    /// cannot tell that edited entry from one the user made by hand.
    #[must_use]
    pub fn matches(&self, wanted: &NewTimeEntry) -> bool {
        self.workspace_id == wanted.workspace_id
            && self.start.as_second() == wanted.start.as_second()
            && u64::try_from(self.duration_secs).ok() == Some(wanted.duration_secs)
    }
}
