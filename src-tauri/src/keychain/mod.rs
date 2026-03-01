//! Secure secret storage using the system keychain.
//!
//! On macOS, secrets are stored in the system Keychain via `security-framework`.
//! On other platforms, a stub implementation returns errors guiding the user
//! to set environment variables.

use crate::error::VaaniError;

/// Service name used in the keychain for all Vaani secrets.
pub const SERVICE_NAME: &str = "com.vaani.app";

/// Abstraction over platform-specific secret storage.
pub trait SecretStorage: Send + Sync {
    /// Store a secret for the given key (e.g., "openai_api_key").
    fn set(&self, key: &str, value: &str) -> Result<(), VaaniError>;

    /// Retrieve a secret. Returns `None` if not found.
    fn get(&self, key: &str) -> Result<Option<String>, VaaniError>;

    /// Delete a secret. Returns Ok even if the key didn't exist.
    fn delete(&self, key: &str) -> Result<(), VaaniError>;
}

/// Create the platform-appropriate `SecretStorage` implementation.
pub fn create_secret_storage() -> Box<dyn SecretStorage> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacKeychain)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Box::new(StubStorage)
    }
}

/// Stub storage for platforms without keychain support.
/// Guides the user to use environment variables instead.
#[cfg(any(not(target_os = "macos"), test))]
struct StubStorage;

#[cfg(any(not(target_os = "macos"), test))]
impl SecretStorage for StubStorage {
    fn set(&self, _key: &str, _value: &str) -> Result<(), VaaniError> {
        Err(VaaniError::Keychain(
            "Keychain not supported on this platform. Use environment variables.".into(),
        ))
    }

    fn get(&self, _key: &str) -> Result<Option<String>, VaaniError> {
        Ok(None)
    }

    fn delete(&self, _key: &str) -> Result<(), VaaniError> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_secret_storage_returns_impl() {
        let storage = create_secret_storage();
        // Verify we get a valid boxed trait object (compilation is the real test)
        let _ = storage;
    }

    #[test]
    fn stub_storage_get_returns_none() {
        let stub = StubStorage;
        let result = stub.get("any_key").expect("get should not error");
        assert!(result.is_none());
    }

    #[test]
    fn stub_storage_set_returns_error() {
        let stub = StubStorage;
        let result = stub.set("any_key", "any_value");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Keychain"));
        assert!(err.contains("environment variables"));
    }

    #[test]
    fn service_name_is_set() {
        assert!(!SERVICE_NAME.is_empty());
        assert!(SERVICE_NAME.contains("vaani"));
    }
}
