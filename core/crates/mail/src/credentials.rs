//! Mail credentials in the secret store, named by the account's connection
//! id. Both kinds stay on the device that stored them
//! ([`app_state::Availability::DeviceOnly`]): every device signs in on its own.

use app_state::{
    ConnectionId, MailAccount, MailAuth, SecretBytes, SecretId, SecretItem, SecretKind,
    SecretStore, SecretStoreError,
};

use crate::error::MailError;

/// Stores an account's app password, replacing an older one.
///
/// # Errors
/// The secret store failed.
pub fn store_password(
    secrets: &dyn SecretStore,
    connection: &ConnectionId,
    password: &str,
) -> Result<(), SecretStoreError> {
    secrets.put(SecretItem::new(
        SecretKind::MailPassword,
        connection.as_str(),
        SecretBytes::new(password.as_bytes().to_vec()),
    ))
}

/// Stores an account's OAuth refresh token, replacing an older one.
///
/// # Errors
/// The secret store failed.
pub fn store_refresh_token(
    secrets: &dyn SecretStore,
    connection: &ConnectionId,
    token: &str,
) -> Result<(), SecretStoreError> {
    secrets.put(SecretItem::new(
        SecretKind::MailRefreshToken,
        connection.as_str(),
        SecretBytes::new(token.as_bytes().to_vec()),
    ))
}

/// Deletes every secret of an account on this device.
///
/// # Errors
/// The secret store failed.
pub fn delete_credentials(
    secrets: &dyn SecretStore,
    connection: &ConnectionId,
) -> Result<(), SecretStoreError> {
    for kind in [SecretKind::MailPassword, SecretKind::MailRefreshToken] {
        secrets.delete(&SecretId::new(kind, connection.as_str()))?;
    }
    Ok(())
}

/// Whether this device holds the secret the account signs in with.
///
/// # Errors
/// The secret store failed.
pub fn has_credential(
    secrets: &dyn SecretStore,
    account: &MailAccount,
) -> Result<bool, SecretStoreError> {
    let kind = match account.auth {
        MailAuth::AppPassword => SecretKind::MailPassword,
        MailAuth::XOAuth2 | MailAuth::DeviceCode => SecretKind::MailRefreshToken,
    };
    Ok(secrets
        .get(&SecretId::new(kind, account.connection.as_str()))?
        .is_some())
}

/// The account's app password.
///
/// # Errors
/// [`MailError::MissingCredential`] when none is stored on this device.
pub(crate) fn password(
    secrets: &dyn SecretStore,
    connection: &ConnectionId,
) -> Result<SecretBytes, MailError> {
    stored(secrets, SecretKind::MailPassword, connection)
}

/// The account's refresh token.
///
/// # Errors
/// [`MailError::MissingCredential`] when none is stored on this device.
pub(crate) fn refresh_token(
    secrets: &dyn SecretStore,
    connection: &ConnectionId,
) -> Result<SecretBytes, MailError> {
    stored(secrets, SecretKind::MailRefreshToken, connection)
}

fn stored(
    secrets: &dyn SecretStore,
    kind: SecretKind,
    connection: &ConnectionId,
) -> Result<SecretBytes, MailError> {
    secrets
        .get(&SecretId::new(kind, connection.as_str()))?
        .map(|item| item.value)
        .ok_or_else(|| {
            MailError::MissingCredential(format!(
                "this Mac holds no {} for mail account {connection}; sign in again",
                kind.label()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_state::{Availability, InMemorySecretStore, MailProvider};
    use domain::workspace::WorkspaceId;

    #[test]
    fn credentials_stay_on_this_device_and_go_with_the_account() {
        let secrets = InMemorySecretStore::new();
        let other_device = secrets.for_another_device();
        let connection = ConnectionId::new("con-mail-1");
        store_password(&secrets, &connection, "synthetic-app-password").unwrap();
        let item = secrets
            .get(&SecretId::new(SecretKind::MailPassword, "con-mail-1"))
            .unwrap()
            .unwrap();
        assert_eq!(item.availability, Availability::DeviceOnly);
        assert_eq!(
            password(&secrets, &connection).unwrap().expose(),
            b"synthetic-app-password"
        );
        assert!(matches!(
            password(&other_device, &connection),
            Err(MailError::MissingCredential(_))
        ));
        let account = MailAccount {
            connection: connection.clone(),
            label: "someone@example.test".to_owned(),
            workspace: WorkspaceId::new("ws-1"),
            address: "someone@example.test".to_owned(),
            provider: MailProvider::Imap,
            host: "imap.example.test".to_owned(),
            port: 993,
            auth: MailAuth::AppPassword,
        };
        assert!(has_credential(&secrets, &account).unwrap());
        delete_credentials(&secrets, &connection).unwrap();
        assert!(!has_credential(&secrets, &account).unwrap());
    }
}
