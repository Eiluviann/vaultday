//! Helpers shared by the two documents: the schema change every device
//! starts from, and typed reads of scalar fields.
//!
//! Each document's top-level objects come from one schema change written
//! under a fixed actor at time zero, so the change hashes alike everywhere
//! and every device edits the same objects. Below the top level the
//! documents avoid objects that two devices could create concurrently under
//! one key (Automerge would keep one and hide the other): records are created
//! only by the device that minted their id, and everything else is a scalar.

use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, AutoCommit, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value};

use crate::error::AppStateError;

/// Separates the parts of a composite map key; never typed by a user.
pub(crate) const KEY_SEPARATOR: char = '\u{1f}';

/// Builds the schema change: `maps` and `lists` as top-level objects. The
/// actor contains a `:`, which no device id may, so it never writes user changes.
pub(crate) fn schema_document(
    actor: &str,
    maps: &[&str],
    lists: &[&str],
) -> Result<AutoCommit, AppStateError> {
    let mut doc = AutoCommit::new().with_actor(ActorId::from(actor.as_bytes()));
    for key in maps {
        doc.put_object(ROOT, *key, ObjType::Map)?;
    }
    for key in lists {
        doc.put_object(ROOT, *key, ObjType::List)?;
    }
    doc.commit_with(CommitOptions::default().with_time(0));
    Ok(doc)
}

/// A top-level object the schema change created.
pub(crate) fn root_object(doc: &AutoCommit, key: &str) -> Result<ObjId, AppStateError> {
    match doc.get(ROOT, key)? {
        Some((Value::Object(_), id)) => Ok(id),
        _ => Err(AppStateError::MissingSchemaObject(key.to_owned())),
    }
}

/// Joins composite key parts with [`KEY_SEPARATOR`].
pub(crate) fn composite_key(parts: &[&str]) -> String {
    parts.join(&KEY_SEPARATOR.to_string())
}

/// The winning string at `key`, `None` when absent or not a string.
pub(crate) fn get_str(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<String> {
    match doc.get(obj, key).ok()?? {
        (Value::Scalar(scalar), _) => match scalar.as_ref() {
            ScalarValue::Str(text) => Some(text.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Every concurrent string at `key`, the winning one first.
pub(crate) fn get_all_str(doc: &AutoCommit, obj: &ObjId, key: &str) -> Vec<String> {
    let Some(winner) = get_str(doc, obj, key) else {
        return Vec::new();
    };
    let mut values = vec![winner.clone()];
    for (value, _) in doc.get_all(obj, key).unwrap_or_default() {
        if let Value::Scalar(scalar) = value
            && let ScalarValue::Str(text) = scalar.as_ref()
            && text.as_str() != winner
        {
            values.push(text.to_string());
        }
    }
    values
}

/// The winning unsigned integer at `key`.
pub(crate) fn get_u64(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<u64> {
    match doc.get(obj, key).ok()?? {
        (Value::Scalar(scalar), _) => match scalar.as_ref() {
            ScalarValue::Uint(number) => Some(*number),
            ScalarValue::Int(number) => u64::try_from(*number).ok(),
            _ => None,
        },
        _ => None,
    }
}

/// Every concurrent unsigned integer at `key`.
pub(crate) fn get_all_u64(doc: &AutoCommit, obj: &ObjId, key: &str) -> Vec<u64> {
    doc.get_all(obj, key)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(value, _)| match value {
            Value::Scalar(scalar) => match scalar.as_ref() {
                ScalarValue::Uint(number) => Some(*number),
                ScalarValue::Int(number) => u64::try_from(*number).ok(),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The winning boolean at `key`.
pub(crate) fn get_bool(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<bool> {
    match doc.get(obj, key).ok()?? {
        (Value::Scalar(scalar), _) => match scalar.as_ref() {
            ScalarValue::Boolean(flag) => Some(*flag),
            _ => None,
        },
        _ => None,
    }
}

/// The winning byte string at `key`.
pub(crate) fn get_bytes(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<Vec<u8>> {
    match doc.get(obj, key).ok()?? {
        (Value::Scalar(scalar), _) => match scalar.as_ref() {
            ScalarValue::Bytes(bytes) => Some(bytes.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// The map object at `key`, `None` when absent or not a map.
pub(crate) fn get_map(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<ObjId> {
    match doc.get(obj, key).ok()?? {
        (Value::Object(ObjType::Map), id) => Some(id),
        _ => None,
    }
}

/// The keys of a map, sorted.
pub(crate) fn keys(doc: &AutoCommit, obj: &ObjId) -> Vec<String> {
    doc.keys(obj).collect()
}

/// Sets `key` to `value`, or deletes it for `None`.
pub(crate) fn put_optional_str(
    doc: &mut AutoCommit,
    obj: &ObjId,
    key: &str,
    value: Option<&str>,
) -> Result<(), AppStateError> {
    match value {
        Some(value) => doc.put(obj, key, value)?,
        None => delete_if_present(doc, obj, key)?,
    }
    Ok(())
}

/// Deletes `key` when it is there; deleting an absent key is a no-op.
pub(crate) fn delete_if_present(
    doc: &mut AutoCommit,
    obj: &ObjId,
    key: &str,
) -> Result<(), AppStateError> {
    if doc.get(obj, key)?.is_some() {
        doc.delete(obj, key)?;
    }
    Ok(())
}
