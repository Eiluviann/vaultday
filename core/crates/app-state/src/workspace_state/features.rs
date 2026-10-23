//! Records a feature set keeps in the workspace document: one JSON scalar
//! per record, keyed by feature, collection and record id. The document
//! knows no feature; each feature reads and writes its own collections, so
//! a feature that is switched off leaves its records untouched and unread.

use automerge::transaction::Transactable;
use domain::registry::FeatureId;

use super::WorkspaceState;
use crate::doc::{KEY_SEPARATOR, composite_key, delete_if_present, get_str, keys};
use crate::error::AppStateError;

impl WorkspaceState {
    /// One record of a feature's collection.
    #[must_use]
    pub fn feature_record(
        &self,
        feature: &FeatureId,
        collection: &str,
        id: &str,
    ) -> Option<String> {
        get_str(
            &self.doc,
            &self.objects.feature_records,
            &composite_key(&[feature.as_str(), collection, id]),
        )
    }

    /// Every record of a feature's collection as `(id, value)`, by id.
    #[must_use]
    pub fn feature_records(&self, feature: &FeatureId, collection: &str) -> Vec<(String, String)> {
        let prefix = composite_key(&[feature.as_str(), collection, ""]);
        let mut records: Vec<(String, String)> = keys(&self.doc, &self.objects.feature_records)
            .into_iter()
            .filter_map(|stored| {
                let id = stored.strip_prefix(&prefix)?.to_owned();
                let value = get_str(&self.doc, &self.objects.feature_records, &stored)?;
                Some((id, value))
            })
            .collect();
        records.sort();
        records
    }

    /// Stores a record, replacing one with the same id.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for an empty part or one holding the
    /// separator, or the document refused the edit.
    pub fn put_feature_record(
        &mut self,
        feature: &FeatureId,
        collection: &str,
        id: &str,
        value: &str,
    ) -> Result<(), AppStateError> {
        let key = record_key(feature, collection, id)?;
        self.doc.put(&self.objects.feature_records, key, value)?;
        Ok(())
    }

    /// Removes a record; removing one that does not exist is not an error.
    ///
    /// # Errors
    /// [`AppStateError::InvalidSetting`] for a malformed part, or the document refused the edit.
    pub fn remove_feature_record(
        &mut self,
        feature: &FeatureId,
        collection: &str,
        id: &str,
    ) -> Result<(), AppStateError> {
        let key = record_key(feature, collection, id)?;
        let records = self.objects.feature_records.clone();
        delete_if_present(&mut self.doc, &records, &key)
    }
}

fn record_key(feature: &FeatureId, collection: &str, id: &str) -> Result<String, AppStateError> {
    for part in [feature.as_str(), collection, id] {
        if part.is_empty() || part.contains(KEY_SEPARATOR) {
            return Err(AppStateError::InvalidSetting(format!(
                "{part:?} cannot name a feature record"
            )));
        }
    }
    Ok(composite_key(&[feature.as_str(), collection, id]))
}

#[cfg(test)]
mod tests {
    use crate::device::DeviceId;
    use crate::workspace_state::WorkspaceState;
    use domain::registry::FeatureId;

    #[test]
    fn records_are_kept_per_feature_and_collection_and_merge() {
        let mut mac = WorkspaceState::new(DeviceId::new("mac-a").unwrap()).unwrap();
        let mut other = WorkspaceState::new(DeviceId::new("mac-b").unwrap()).unwrap();
        let toggl = FeatureId::new("toggl-billing");
        let rival = FeatureId::new("rival");
        mac.put_feature_record(&toggl, "queue", "ses-2", r#"{"n":2}"#)
            .unwrap();
        mac.put_feature_record(&toggl, "queue", "ses-1", r#"{"n":1}"#)
            .unwrap();
        mac.put_feature_record(&toggl, "flat", "2026-10-19", "{}")
            .unwrap();
        mac.put_feature_record(&rival, "queue", "ses-1", "{}")
            .unwrap();
        other.doc_mut().merge(mac.doc_mut()).unwrap();
        assert_eq!(
            other.feature_records(&toggl, "queue"),
            [
                ("ses-1".to_owned(), r#"{"n":1}"#.to_owned()),
                ("ses-2".to_owned(), r#"{"n":2}"#.to_owned()),
            ]
        );
        assert_eq!(
            other.feature_record(&toggl, "flat", "2026-10-19"),
            Some("{}".to_owned())
        );
        other
            .remove_feature_record(&toggl, "queue", "ses-1")
            .unwrap();
        other
            .remove_feature_record(&toggl, "queue", "ses-9")
            .unwrap();
        assert_eq!(other.feature_records(&toggl, "queue").len(), 1);
        assert_eq!(other.feature_records(&rival, "queue").len(), 1);
        assert!(mac.put_feature_record(&toggl, "", "x", "{}").is_err());
    }
}
