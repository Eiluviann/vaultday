//! The Today query: every story with open work plus the day's log lines, as
//! owned plain data the bindings can copy into their own records.

use std::path::PathBuf;

use crate::date::Date;
use crate::note::{LogLine, StoryStatus};
use crate::scan::Task;
use crate::vault::Vault;

/// What a surface shows for one day: stories with open work and the day log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodayView {
    /// The day the view was built for.
    pub date: Date,
    /// Stories with at least one open Next Step or Pending Input task, in
    /// [`Vault::stories`] order.
    pub stories: Vec<TodayStory>,
    /// The day log's lines for `date`, in path then line order.
    pub log: Vec<LogLine>,
}

/// One story's open work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodayStory {
    /// Path of the story note, relative to the vault root.
    pub relative_path: PathBuf,
    /// The ticket number, when the frontmatter has one.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The story status, when recognised.
    pub status: Option<StoryStatus>,
    /// Open tasks under `## Next Steps`.
    pub next_steps: Vec<Task>,
    /// Open tasks under `## Pending Input`.
    pub pending_input: Vec<Task>,
}

impl Vault {
    /// Builds the Today view for `date`: stories that still have open work and
    /// the log lines written on that day.
    #[must_use]
    pub fn today(&self, date: Date) -> TodayView {
        let stories = self
            .stories()
            .into_iter()
            .filter_map(|entry| {
                let next_steps = open_tasks(&entry.story.next_steps);
                let pending_input = open_tasks(&entry.story.pending_input);
                if next_steps.is_empty() && pending_input.is_empty() {
                    return None;
                }
                Some(TodayStory {
                    relative_path: entry.path.to_path_buf(),
                    number: entry.story.number.clone(),
                    title: entry.story.title.clone(),
                    status: entry.story.status,
                    next_steps,
                    pending_input,
                })
            })
            .collect();
        let log = self
            .log_lines_on(date)
            .into_iter()
            .map(|entry| entry.line.clone())
            .collect();
        TodayView { date, stories, log }
    }
}

fn open_tasks(tasks: &[Task]) -> Vec<Task> {
    tasks
        .iter()
        .filter(|task| task.is_open())
        .cloned()
        .collect()
}
