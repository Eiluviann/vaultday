//! Screen-share mode, a per-device setting of the app-level document: whether
//! this Mac is in the mode and the workspace selection to restore when it
//! ends. Neither is ever synced to another device's reading or written to a
//! workspace document.

use crate::CoreError;
use crate::app_state::AppStateHandle;

#[uniffi::export]
impl AppStateHandle {
    /// Whether screen-share mode is on for this device.
    pub fn screen_share_mode(&self) -> bool {
        self.read_state(|state| state.screen_share_mode())
    }

    /// Switches screen-share mode on or off for this device.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_screen_share_mode(&self, is_on: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_screen_share_mode(is_on))
    }

    /// The selection screen-share mode moved this device away from, if any.
    pub fn screen_share_return_to(&self) -> Option<String> {
        self.read_state(|state| state.screen_share_return_to())
    }

    /// Remembers, or with `None` forgets, the selection to restore when the
    /// mode ends.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for an empty or oversized selection; the
    /// document refused the edit.
    pub fn set_screen_share_return_to(&self, selection: Option<String>) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_screen_share_return_to(selection.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::app_state::open_app_state;
    use crate::app_state::tests::Keychain;
    use crate::files::plain_sync_files;

    #[test]
    fn the_mode_and_the_selection_to_return_to_survive_a_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("app").to_string_lossy().into_owned();
        let keychain = Arc::new(Keychain::default());
        let app = open_app_state(plain_sync_files(), dir.clone(), keychain.clone()).unwrap();
        assert!(!app.screen_share_mode());
        assert_eq!(app.screen_share_return_to(), None);

        app.set_screen_share_mode(true).unwrap();
        app.set_screen_share_return_to(Some("workspace:ws-life".to_owned()))
            .unwrap();
        assert!(app.set_screen_share_return_to(Some(String::new())).is_err());
        app.save().unwrap();

        let reopened = open_app_state(plain_sync_files(), dir, keychain).unwrap();
        assert!(reopened.screen_share_mode());
        assert_eq!(
            reopened.screen_share_return_to().as_deref(),
            Some("workspace:ws-life")
        );

        reopened.set_screen_share_mode(false).unwrap();
        reopened.set_screen_share_return_to(None).unwrap();
        assert!(!reopened.screen_share_mode());
        assert_eq!(reopened.screen_share_return_to(), None);
    }
}
