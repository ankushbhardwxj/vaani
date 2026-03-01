//! macOS Keychain implementation using security-framework.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use tracing::debug;

use super::{SecretStorage, SERVICE_NAME};
use crate::error::VaaniError;

/// macOS Keychain-backed secret storage.
pub struct MacKeychain;

impl SecretStorage for MacKeychain {
    fn set(&self, key: &str, value: &str) -> Result<(), VaaniError> {
        // Delete existing entry first (set_generic_password may fail if it exists)
        let _ = delete_generic_password(SERVICE_NAME, key);

        set_generic_password(SERVICE_NAME, key, value.as_bytes())
            .map_err(|e| VaaniError::Keychain(format!("Failed to store secret '{key}': {e}")))?;

        debug!(key = key, "Secret stored in Keychain");
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, VaaniError> {
        match get_generic_password(SERVICE_NAME, key) {
            Ok(bytes) => {
                let value = String::from_utf8(bytes).map_err(|e| {
                    VaaniError::Keychain(format!("Invalid UTF-8 in secret '{key}': {e}"))
                })?;
                debug!(key = key, "Secret retrieved from Keychain");
                Ok(Some(value))
            }
            Err(e) if e.code() == -25300 => {
                // errSecItemNotFound — key doesn't exist
                debug!(key = key, "Secret not found in Keychain");
                Ok(None)
            }
            Err(e) => Err(VaaniError::Keychain(format!(
                "Failed to retrieve secret '{key}': {e}"
            ))),
        }
    }

    fn delete(&self, key: &str) -> Result<(), VaaniError> {
        match delete_generic_password(SERVICE_NAME, key) {
            Ok(()) => {
                debug!(key = key, "Secret deleted from Keychain");
                Ok(())
            }
            Err(e) if e.code() == -25300 => {
                // errSecItemNotFound — already gone, that's fine
                Ok(())
            }
            Err(e) => Err(VaaniError::Keychain(format!(
                "Failed to delete secret '{key}': {e}"
            ))),
        }
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;

    const TEST_KEY_PREFIX: &str = "vaani_test_";

    fn test_key(suffix: &str) -> String {
        format!("{TEST_KEY_PREFIX}{suffix}")
    }

    /// Helper to ensure cleanup even if a test panics.
    fn cleanup(key: &str) {
        let kc = MacKeychain;
        let _ = kc.delete(key);
    }

    #[test]
    fn mac_keychain_roundtrip() {
        let key = test_key("roundtrip");
        let kc = MacKeychain;

        // Ensure clean state
        cleanup(&key);

        // Set
        kc.set(&key, "test_secret_value")
            .expect("set should succeed");

        // Get
        let value = kc.get(&key).expect("get should succeed");
        assert_eq!(value, Some("test_secret_value".to_string()));

        // Delete
        kc.delete(&key).expect("delete should succeed");

        // Verify gone
        let value = kc.get(&key).expect("get after delete should succeed");
        assert!(value.is_none());
    }

    #[test]
    fn mac_keychain_get_nonexistent_returns_none() {
        let key = test_key("nonexistent");
        let kc = MacKeychain;

        // Ensure it doesn't exist
        cleanup(&key);

        let value = kc.get(&key).expect("get should succeed");
        assert!(value.is_none());
    }

    #[test]
    fn mac_keychain_delete_nonexistent_is_ok() {
        let key = test_key("delete_missing");
        let kc = MacKeychain;

        // Ensure it doesn't exist
        cleanup(&key);

        // Deleting a non-existent key should be fine
        kc.delete(&key)
            .expect("delete of non-existent key should succeed");
    }

    #[test]
    fn mac_keychain_overwrite_existing() {
        let key = test_key("overwrite");
        let kc = MacKeychain;

        // Ensure clean state
        cleanup(&key);

        // Set initial value
        kc.set(&key, "first_value")
            .expect("first set should succeed");

        // Overwrite with new value
        kc.set(&key, "second_value")
            .expect("second set should succeed");

        // Get should return latest
        let value = kc.get(&key).expect("get should succeed");
        assert_eq!(value, Some("second_value".to_string()));

        // Cleanup
        cleanup(&key);
    }
}
