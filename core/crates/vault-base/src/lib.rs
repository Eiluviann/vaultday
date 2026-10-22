//! Vault base: the one markdown parser, writer and workspace model of Vaultday.
//!
//! The bootstrap only proves that the workspace builds, lints and tests on
//! Linux and macOS; the parser, writer and workspace resolution arrive next.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// The hidden folder that marks a workspace root. A folder is a workspace when
/// it holds `.vaultday/settings.json`.
pub const WORKSPACE_DIR_NAME: &str = ".vaultday";

#[cfg(test)]
mod tests {
    use super::WORKSPACE_DIR_NAME;

    #[test]
    fn workspace_marker_is_a_hidden_folder() {
        assert!(WORKSPACE_DIR_NAME.starts_with('.'));
    }
}
