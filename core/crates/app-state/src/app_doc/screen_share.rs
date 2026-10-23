//! Screen-share mode, a per-device setting.
//!
//! Whether this Mac is in screen-share mode, and the workspace selection to
//! come back to when it ends, live in the document's `device_settings` map
//! under keys prefixed with this device's id. They belong to one Mac: another
//! device that merges the document never reads them, and neither is ever
//! written to a workspace document, which is shared with every device that
//! opens the workspace.

use automerge::transaction::Transactable;

use super::AppState;
use crate::doc::{KEY_SEPARATOR, delete_if_present, get_bool, get_str};
use crate::error::AppStateError;

const SCREEN_SHARE_KEY: &str = "screen_share_mode";
const RETURN_TO_KEY: &str = "screen_share_return_to";

/// The longest selection to come back to, in bytes: a workspace id with its tag.
const MAX_RETURN_TO_BYTES: usize = 128;

impl AppState {
    /// Whether screen-share mode is on for this device; off until switched on.
    #[must_use]
    pub fn screen_share_mode(&self) -> bool {
        get_bool(
            &self.doc,
            &self.objects.device_settings,
            &self.device_key(SCREEN_SHARE_KEY),
        )
        .unwrap_or(false)
    }

    /// Switches screen-share mode on or off for this device only.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_screen_share_mode(&mut self, is_on: bool) -> Result<(), AppStateError> {
        let key = self.device_key(SCREEN_SHARE_KEY);
        self.doc.put(&self.objects.device_settings, key, is_on)?;
        Ok(())
    }

    /// The workspace selection screen-share mode moved this device away from,
    /// as the app wrote it; `None` when nothing waits to be restored.
    #[must_use]
    pub fn screen_share_return_to(&self) -> Option<String> {
        get_str(
            &self.doc,
            &self.objects.device_settings,
            &self.device_key(RETURN_TO_KEY),
        )
    }

    /// Remembers, or with `None` forgets, the selection to restore when
    /// screen-share mode ends. The text is opaque to the core.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty selection, one longer
    /// than 128 bytes or one with a control separator; the document refused
    /// the edit.
    pub fn set_screen_share_return_to(
        &mut self,
        selection: Option<&str>,
    ) -> Result<(), AppStateError> {
        let key = self.device_key(RETURN_TO_KEY);
        match selection {
            Some(selection) => {
                if selection.is_empty()
                    || selection.len() > MAX_RETURN_TO_BYTES
                    || selection.contains(KEY_SEPARATOR)
                {
                    return Err(AppStateError::InvalidSetting(format!(
                        "selection {selection:?} must be 1 to {MAX_RETURN_TO_BYTES} bytes without control separators"
                    )));
                }
                self.doc
                    .put(&self.objects.device_settings, key, selection)?;
            }
            None => delete_if_present(&mut self.doc, &self.objects.device_settings, &key)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use automerge::ReadDoc;

    use super::*;
    use crate::device::DeviceId;
    use crate::workspace_state::WorkspaceState;

    fn state(device: &str) -> AppState {
        AppState::new(DeviceId::new(device).unwrap()).unwrap()
    }

    #[test]
    fn it_is_off_and_forgets_nothing_until_set() {
        let state = state("mac-a");
        assert!(!state.screen_share_mode());
        assert_eq!(state.screen_share_return_to(), None);
    }

    #[test]
    fn the_mode_and_the_selection_to_return_to_round_trip() {
        let mut mac = state("mac-a");
        mac.set_screen_share_mode(true).unwrap();
        mac.set_screen_share_return_to(Some("workspace:ws-life"))
            .unwrap();
        assert!(mac.screen_share_mode());
        assert_eq!(
            mac.screen_share_return_to().as_deref(),
            Some("workspace:ws-life")
        );

        mac.set_screen_share_mode(false).unwrap();
        mac.set_screen_share_return_to(None).unwrap();
        mac.set_screen_share_return_to(None).unwrap();
        assert!(!mac.screen_share_mode());
        assert_eq!(mac.screen_share_return_to(), None);
    }

    #[test]
    fn a_selection_to_return_to_must_be_a_plain_short_text() {
        let mut mac = state("mac-a");
        for refused in ["", "a\u{1f}b", &"x".repeat(MAX_RETURN_TO_BYTES + 1)] {
            assert!(
                matches!(
                    mac.set_screen_share_return_to(Some(refused)),
                    Err(AppStateError::InvalidSetting(_))
                ),
                "{refused:?}"
            );
        }
        assert_eq!(mac.screen_share_return_to(), None);
    }

    #[test]
    fn the_mode_is_per_device_and_never_read_from_another_device() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        mac.set_screen_share_mode(true).unwrap();
        mac.set_screen_share_return_to(Some("all")).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert!(mac.screen_share_mode());
        assert!(
            !other.screen_share_mode(),
            "the other device reads its own switch"
        );
        assert_eq!(other.screen_share_return_to(), None);

        other.set_screen_share_mode(true).unwrap();
        mac.set_screen_share_mode(false).unwrap();
        mac.doc.merge(&mut other.doc).unwrap();
        assert!(
            !mac.screen_share_mode(),
            "the other's switch never turns ours on"
        );
    }

    #[test]
    fn the_mode_sits_in_the_device_settings_and_never_in_the_shared_settings() {
        let mut mac = state("mac-a");
        mac.set_screen_share_mode(true).unwrap();
        mac.set_screen_share_return_to(Some("all")).unwrap();
        let device_keys: Vec<String> = mac
            .doc
            .keys(&mac.objects.device_settings)
            .filter(|key| key.contains("screen_share"))
            .collect();
        assert_eq!(device_keys.len(), 2);
        assert!(device_keys.iter().all(|key| key.starts_with("mac-a")));
        assert_eq!(
            mac.doc
                .keys(&mac.objects.settings)
                .filter(|key| key.contains("screen_share"))
                .count(),
            0
        );
    }

    #[test]
    fn the_mode_never_enters_a_workspace_document() {
        let mut mac = state("mac-a");
        mac.set_screen_share_mode(true).unwrap();
        mac.set_screen_share_return_to(Some("workspace:ws-1"))
            .unwrap();
        let mut workspace = WorkspaceState::new(DeviceId::new("mac-a").unwrap()).unwrap();
        let bytes = workspace.doc_mut().save();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("screen_share"));
        assert!(!text.contains("workspace:ws-1"));
    }
}
