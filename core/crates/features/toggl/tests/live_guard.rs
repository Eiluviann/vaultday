//! The live client is reachable only when the feature is on for the
//! workspace, a token is stored and `allow_live` is on. Constructing it
//! sends nothing; no test here or anywhere calls it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use app_state::{EnablementScope, InMemorySecretStore, SecretStore};
use domain::registry::Registry;
use support::{open_app_state, store_token, workspace_id};
use vaultday_feature_toggl::{
    HttpTogglApi, LIVE_BASE_URL, TogglError, allow_live, feature_id, register, set_allow_live,
};

fn registry() -> Registry {
    let mut registry = Registry::new();
    register(&mut registry).unwrap();
    registry
}

fn refusal(
    registry: &Registry,
    store: &app_state::AppStateStore,
    secrets: &InMemorySecretStore,
) -> Option<&'static str> {
    let secrets: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    match HttpTogglApi::live(registry, store.state(), &workspace_id(), secrets) {
        Err(TogglError::LiveRefused(reason)) => Some(reason),
        Err(other) => panic!("unexpected {other}"),
        Ok(api) => {
            assert_eq!(api.base_url(), LIVE_BASE_URL);
            None
        }
    }
}

#[test]
fn the_live_client_needs_the_feature_a_token_and_allow_live() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = InMemorySecretStore::new();
    let mut store = open_app_state(dir.path(), &secrets);
    let registry = registry();
    assert!(!allow_live(store.state()), "allow_live is off by default");

    assert_eq!(
        refusal(&registry, &store, &secrets),
        Some("the feature is off for this workspace")
    );
    store
        .edit(|state| {
            state.set_feature_enabled(&EnablementScope::App, &feature_id(), true)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        refusal(&registry, &store, &secrets),
        Some("the feature is off for this workspace"),
        "the app switch alone is not enough"
    );
    store
        .edit(|state| {
            state.set_feature_enabled(
                &EnablementScope::Workspace(workspace_id()),
                &feature_id(),
                true,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        refusal(&registry, &store, &secrets),
        Some("no token is stored for this workspace")
    );
    store_token(&secrets);
    assert_eq!(
        refusal(&registry, &store, &secrets),
        Some("allow_live is off")
    );

    // With all three the client is built, aimed at the live service, and
    // never called: this PR contacts no live API.
    store
        .edit(|state| {
            set_allow_live(state, true).map_err(|error| match error {
                TogglError::AppState(error) => error,
                other => panic!("{other}"),
            })
        })
        .unwrap();
    assert!(allow_live(store.state()));
    assert_eq!(refusal(&registry, &store, &secrets), None);

    store
        .edit(|state| {
            set_allow_live(state, false).map_err(|error| match error {
                TogglError::AppState(error) => error,
                other => panic!("{other}"),
            })
        })
        .unwrap();
    assert_eq!(
        refusal(&registry, &store, &secrets),
        Some("allow_live is off")
    );
}

#[test]
fn an_empty_registry_never_hands_out_the_live_client() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = InMemorySecretStore::new();
    store_token(&secrets);
    let mut store = open_app_state(dir.path(), &secrets);
    store
        .edit(|state| {
            state.set_feature_enabled(&EnablementScope::App, &feature_id(), true)?;
            state.set_feature_enabled(
                &EnablementScope::Workspace(workspace_id()),
                &feature_id(),
                true,
            )?;
            set_allow_live(state, true)
                .map_err(|_| app_state::AppStateError::InvalidSetting(String::new()))
        })
        .unwrap();
    assert_eq!(
        refusal(&Registry::new(), &store, &secrets),
        Some("the feature is off for this workspace")
    );
}
