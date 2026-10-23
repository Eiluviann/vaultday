//! Billing rates and per-feature values in the app-level document.
//!
//! Rates and currencies live here and in the Workspace note only, never in
//! `settings.json` or the vault's synced files: this document is encrypted
//! and signed. Feature values are opaque strings a feature set keeps for
//! itself (the Toggl mapping, its `allow_live` switch), stored as scalars
//! under composite keys so two devices never create competing objects.

use automerge::transaction::Transactable;
use domain::registry::FeatureId;
use domain::workspace::WorkspaceId;

use super::{APP_SCOPE, AppState, WORKSPACE_SCOPE};
use crate::doc::{KEY_SEPARATOR, composite_key, delete_if_present, get_str, get_u64, keys};
use crate::error::AppStateError;

const RATE_AMOUNT: &str = "billing_rate_minor";
const RATE_CURRENCY: &str = "billing_rate_currency";
const FEATURE_VALUE: &str = "feature_value";

/// An hourly rate in a currency's minor unit (cents for EUR), for one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingRate {
    /// The amount per hour, in the currency's minor unit.
    pub amount_minor_per_hour: u64,
    /// The ISO 4217 code, three upper-case letters.
    pub currency: String,
}

/// Where a feature value applies: the whole app or one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureValueScope {
    /// The whole app.
    App,
    /// One workspace.
    Workspace(WorkspaceId),
}

impl BillingRate {
    /// Checks the currency code.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] unless the currency is three upper-case ASCII letters.
    pub fn validate(&self) -> Result<(), AppStateError> {
        let is_iso_code =
            self.currency.len() == 3 && self.currency.chars().all(|c| c.is_ascii_uppercase());
        if is_iso_code {
            Ok(())
        } else {
            Err(AppStateError::InvalidSetting(format!(
                "{:?} is not a currency code such as EUR",
                self.currency
            )))
        }
    }
}

impl AppState {
    /// The hourly rate of a workspace, `None` until one is set.
    #[must_use]
    pub fn billing_rate(&self, workspace: &WorkspaceId) -> Option<BillingRate> {
        let settings = &self.objects.settings;
        let amount = get_u64(
            &self.doc,
            settings,
            &composite_key(&[RATE_AMOUNT, workspace.as_str()]),
        )?;
        let currency = get_str(
            &self.doc,
            settings,
            &composite_key(&[RATE_CURRENCY, workspace.as_str()]),
        )?;
        Some(BillingRate {
            amount_minor_per_hour: amount,
            currency,
        })
    }

    /// Sets or, with `None`, clears the hourly rate of a workspace.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for a malformed currency or workspace
    /// id, or the document refused the edit.
    pub fn set_billing_rate(
        &mut self,
        workspace: &WorkspaceId,
        rate: Option<&BillingRate>,
    ) -> Result<(), AppStateError> {
        check_part(workspace.as_str())?;
        let amount_key = composite_key(&[RATE_AMOUNT, workspace.as_str()]);
        let currency_key = composite_key(&[RATE_CURRENCY, workspace.as_str()]);
        let settings = self.objects.settings.clone();
        match rate {
            Some(rate) => {
                rate.validate()?;
                self.doc
                    .put(&settings, amount_key, rate.amount_minor_per_hour)?;
                self.doc
                    .put(&settings, currency_key, rate.currency.as_str())?;
            }
            None => {
                delete_if_present(&mut self.doc, &settings, &amount_key)?;
                delete_if_present(&mut self.doc, &settings, &currency_key)?;
            }
        }
        Ok(())
    }

    /// One value a feature keeps for itself.
    #[must_use]
    pub fn feature_value(
        &self,
        feature: &FeatureId,
        scope: &FeatureValueScope,
        key: &str,
    ) -> Option<String> {
        get_str(
            &self.doc,
            &self.objects.settings,
            &feature_value_key(feature, scope, key),
        )
    }

    /// Every value a feature keeps in one scope, by key.
    #[must_use]
    pub fn feature_values(
        &self,
        feature: &FeatureId,
        scope: &FeatureValueScope,
    ) -> Vec<(String, String)> {
        let prefix = feature_value_key(feature, scope, "");
        keys(&self.doc, &self.objects.settings)
            .into_iter()
            .filter_map(|stored| {
                let key = stored.strip_prefix(&prefix)?.to_owned();
                let value = get_str(&self.doc, &self.objects.settings, &stored)?;
                Some((key, value))
            })
            .collect()
    }

    /// Sets or, with `None`, clears one value a feature keeps for itself.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty key or a part holding
    /// the separator, or the document refused the edit.
    pub fn set_feature_value(
        &mut self,
        feature: &FeatureId,
        scope: &FeatureValueScope,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), AppStateError> {
        if key.is_empty() {
            return Err(AppStateError::InvalidSetting(
                "a feature value needs a key".to_owned(),
            ));
        }
        check_part(feature.as_str())?;
        check_part(key)?;
        if let FeatureValueScope::Workspace(workspace) = scope {
            check_part(workspace.as_str())?;
        }
        let stored = feature_value_key(feature, scope, key);
        let settings = self.objects.settings.clone();
        match value {
            Some(value) => {
                self.doc.put(&settings, stored, value)?;
            }
            None => delete_if_present(&mut self.doc, &settings, &stored)?,
        }
        Ok(())
    }
}

fn feature_value_key(feature: &FeatureId, scope: &FeatureValueScope, key: &str) -> String {
    match scope {
        FeatureValueScope::App => composite_key(&[FEATURE_VALUE, feature.as_str(), APP_SCOPE, key]),
        FeatureValueScope::Workspace(workspace) => composite_key(&[
            FEATURE_VALUE,
            feature.as_str(),
            WORKSPACE_SCOPE,
            workspace.as_str(),
            key,
        ]),
    }
}

fn check_part(part: &str) -> Result<(), AppStateError> {
    if part.is_empty() || part.contains(KEY_SEPARATOR) {
        return Err(AppStateError::InvalidSetting(format!(
            "{part:?} cannot name a stored setting"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;

    fn state(device: &str) -> AppState {
        AppState::new(DeviceId::new(device).unwrap()).unwrap()
    }

    #[test]
    fn rates_are_kept_per_workspace_and_merge_to_other_devices() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        let acme = WorkspaceId::new("acme");
        assert_eq!(mac.billing_rate(&acme), None);
        let rate = BillingRate {
            amount_minor_per_hour: 9_500,
            currency: "EUR".to_owned(),
        };
        mac.set_billing_rate(&acme, Some(&rate)).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert_eq!(other.billing_rate(&acme), Some(rate));
        assert_eq!(other.billing_rate(&WorkspaceId::new("home")), None);
        mac.set_billing_rate(&acme, None).unwrap();
        assert_eq!(mac.billing_rate(&acme), None);
        let bad = BillingRate {
            amount_minor_per_hour: 1,
            currency: "euro".to_owned(),
        };
        assert!(matches!(
            mac.set_billing_rate(&acme, Some(&bad)),
            Err(AppStateError::InvalidSetting(_))
        ));
    }

    #[test]
    fn feature_values_are_scoped_and_listed() {
        let mut mac = state("mac-a");
        let toggl = FeatureId::new("toggl-billing");
        let acme = FeatureValueScope::Workspace(WorkspaceId::new("acme"));
        let home = FeatureValueScope::Workspace(WorkspaceId::new("home"));
        mac.set_feature_value(&toggl, &FeatureValueScope::App, "allow_live", Some("false"))
            .unwrap();
        mac.set_feature_value(&toggl, &acme, "workspace_id", Some("42"))
            .unwrap();
        mac.set_feature_value(&toggl, &acme, "project.WEB-1", Some("7"))
            .unwrap();
        mac.set_feature_value(&toggl, &home, "workspace_id", Some("9"))
            .unwrap();
        assert_eq!(
            mac.feature_value(&toggl, &FeatureValueScope::App, "allow_live"),
            Some("false".to_owned())
        );
        assert_eq!(
            mac.feature_values(&toggl, &acme),
            [
                ("project.WEB-1".to_owned(), "7".to_owned()),
                ("workspace_id".to_owned(), "42".to_owned()),
            ]
        );
        mac.set_feature_value(&toggl, &acme, "project.WEB-1", None)
            .unwrap();
        assert_eq!(mac.feature_values(&toggl, &acme).len(), 1);
        assert!(mac.set_feature_value(&toggl, &acme, "", Some("x")).is_err());
        assert!(
            mac.set_feature_value(&toggl, &acme, "a\u{1f}b", Some("x"))
                .is_err()
        );
    }
}
