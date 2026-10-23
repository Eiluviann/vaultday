//! The check that keeps secrets out of a workspace's `settings.json`, which
//! syncs as plaintext with the vault. Secrets belong in the [`SecretStore`];
//! the settings loader calls this on the parsed file and refuses it when
//! anything is found.
//!
//! [`SecretStore`]: crate::secret_store::SecretStore

use serde_json::Value;

/// Key fragments that name a secret, compared without case, `-` or `_`.
const SECRET_KEY_FRAGMENTS: [&str; 10] = [
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "privatekey",
    "credential",
    "bearer",
    "accesskey",
    "clientsecret",
];
/// Value prefixes of well-known key formats.
const SECRET_VALUE_PREFIXES: [&str; 6] =
    ["sk-", "ghp_", "github_pat_", "xoxb-", "xoxp-", "-----BEGIN"];

/// The JSON paths (such as `features.mail.apiKey`) of every field in
/// `settings` whose key names a secret or whose value has the form of one, sorted.
#[must_use]
pub fn secret_like_fields(settings: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect(settings, "", &mut found);
    found.sort();
    found
}

fn collect(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if is_secret_key(key) {
                    found.push(child_path.clone());
                }
                collect(child, &child_path, found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect(item, &format!("{path}[{index}]"), found);
            }
        }
        Value::String(text) if is_secret_value(text) => found.push(path.to_owned()),
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    let folded: String = key
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    SECRET_KEY_FRAGMENTS
        .iter()
        .any(|fragment| folded.contains(fragment))
}

fn is_secret_value(text: &str) -> bool {
    SECRET_VALUE_PREFIXES
        .iter()
        .any(|prefix| text.trim_start().starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ordinary_settings_pass() {
        let settings = json!({
            "schemaVersion": 1,
            "name": "Acme",
            "ticketPattern": "WEB-\\d+",
            "hosts": ["tickets.example.test"],
            "features": {},
            "hoursPerDay": 8
        });
        assert!(secret_like_fields(&settings).is_empty());
    }

    #[test]
    fn secret_keys_and_secret_shaped_values_are_found() {
        let settings = json!({
            "name": "Acme",
            "features": { "mail": { "api_key": "x", "Password": "y" } },
            "extra": [{ "note": "sk-synthetic000" }],
            "authToken": 1
        });
        assert_eq!(
            secret_like_fields(&settings),
            [
                "authToken",
                "extra[0].note",
                "features.mail.Password",
                "features.mail.api_key"
            ]
        );
    }
}
