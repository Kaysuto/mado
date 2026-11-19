//! Secrets (API keys…) live in the operating system's keychain: Windows Credential Manager,
//! macOS Keychain, Secret Service on Linux. Never in the database, never in a log.

use keyring::{Entry, Error};

const SERVICE: &str = "madow";

fn entry(name: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, name).map_err(explain)
}

fn explain(error: Error) -> String {
    format!("The system keychain refused the operation: {error}")
}

pub fn get(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(explain(error)),
    }
}

pub fn set(name: &str, secret: &str) -> Result<(), String> {
    entry(name)?.set_password(secret).map_err(explain)
}

/// Removing a secret that is not there is not an error.
pub fn delete(name: &str) -> Result<(), String> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(explain(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real keychain, so it only runs on demand:
    /// `cargo test -- --ignored keychain`.
    #[test]
    #[ignore]
    fn keychain_round_trip() {
        let name = "test-round-trip";
        delete(name).unwrap();
        assert_eq!(get(name).unwrap(), None);
        set(name, "s3cret").unwrap();
        assert_eq!(get(name).unwrap().as_deref(), Some("s3cret"));
        delete(name).unwrap();
        assert_eq!(get(name).unwrap(), None);
    }
}
