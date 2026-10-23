//! Where a workspace's entries go in Toggl, and the app-wide `allow_live`
//! switch, both kept in the signed app-level document.

use std::collections::BTreeMap;

use app_state::{AppState, FeatureValueScope};
use domain::workspace::WorkspaceId;

use crate::error::TogglError;
use crate::feature::{TogglFeature, feature_id};

const ALLOW_LIVE: &str = "allow_live";
const TOGGL_WORKSPACE: &str = "toggl_workspace_id";
const DEFAULT_PROJECT: &str = "default_project_id";
const STORY_PROJECT_PREFIX: &str = "project.";

/// A workspace's Toggl mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TogglMapping {
    /// The Toggl workspace entries go to; nothing is sent until it is set.
    pub toggl_workspace_id: Option<u64>,
    /// The project of an entry whose story has none of its own.
    pub default_project_id: Option<u64>,
    /// Projects by story number.
    pub projects_by_story: BTreeMap<String, u64>,
}

impl TogglMapping {
    /// The project an entry of `story` goes to.
    #[must_use]
    pub fn project_for(&self, story: Option<&str>) -> Option<u64> {
        story
            .and_then(|story| self.projects_by_story.get(story).copied())
            .or(self.default_project_id)
    }

    /// The workspace's mapping; empty until set.
    #[must_use]
    pub fn read(app_state: &AppState, workspace: &WorkspaceId) -> TogglMapping {
        let values = app_state.feature_values(&feature_id(), &workspace_scope(workspace));
        let mut mapping = TogglMapping::default();
        for (key, value) in values {
            let Ok(id) = value.parse::<u64>() else {
                continue;
            };
            match key.as_str() {
                TOGGL_WORKSPACE => mapping.toggl_workspace_id = Some(id),
                DEFAULT_PROJECT => mapping.default_project_id = Some(id),
                other => {
                    if let Some(story) = other.strip_prefix(STORY_PROJECT_PREFIX) {
                        mapping.projects_by_story.insert(story.to_owned(), id);
                    }
                }
            }
        }
        mapping
    }

    /// Replaces the workspace's mapping with this one.
    ///
    /// # Errors
    /// [`TogglError::InvalidInput`] for a blank story number, or the document refused the edit.
    pub fn write(
        &self,
        feature: &TogglFeature,
        app_state: &mut AppState,
    ) -> Result<(), TogglError> {
        if self
            .projects_by_story
            .keys()
            .any(|story| story.trim().is_empty())
        {
            return Err(TogglError::InvalidInput(
                "a project mapping needs a story number".to_owned(),
            ));
        }
        let scope = workspace_scope(feature.workspace());
        let id = feature_id();
        for (key, _) in app_state.feature_values(&id, &scope) {
            if key == TOGGL_WORKSPACE
                || key == DEFAULT_PROJECT
                || key.starts_with(STORY_PROJECT_PREFIX)
            {
                app_state.set_feature_value(&id, &scope, &key, None)?;
            }
        }
        let text = |value: Option<u64>| value.map(|value| value.to_string());
        app_state.set_feature_value(
            &id,
            &scope,
            TOGGL_WORKSPACE,
            text(self.toggl_workspace_id).as_deref(),
        )?;
        app_state.set_feature_value(
            &id,
            &scope,
            DEFAULT_PROJECT,
            text(self.default_project_id).as_deref(),
        )?;
        for (story, project) in &self.projects_by_story {
            app_state.set_feature_value(
                &id,
                &scope,
                &format!("{STORY_PROJECT_PREFIX}{}", story.trim()),
                Some(&project.to_string()),
            )?;
        }
        Ok(())
    }
}

/// Whether the app may reach the live Toggl service; off until switched on.
#[must_use]
pub fn allow_live(app_state: &AppState) -> bool {
    app_state
        .feature_value(&feature_id(), &FeatureValueScope::App, ALLOW_LIVE)
        .is_some_and(|value| value == "true")
}

/// Switches `allow_live`. Switching it off removes the value, so the
/// default is what remains.
///
/// # Errors
/// The document refused the edit.
pub fn set_allow_live(app_state: &mut AppState, is_allowed: bool) -> Result<(), TogglError> {
    app_state.set_feature_value(
        &feature_id(),
        &FeatureValueScope::App,
        ALLOW_LIVE,
        is_allowed.then_some("true"),
    )?;
    Ok(())
}

fn workspace_scope(workspace: &WorkspaceId) -> FeatureValueScope {
    FeatureValueScope::Workspace(workspace.clone())
}
