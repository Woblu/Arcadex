//! Passwords and keys, kept in the operating system's credential store.
//!
//! The ScreenScraper passwords and the TheGamesDB key used to sit in the
//! library database beside everything else, in plain text, in a file that
//! gets copied into backups and synced folders along with the rest of the app
//! data. Each platform has a store made for exactly this - the Credential
//! Manager on Windows, the Keychain on macOS, the Secret Service on Linux -
//! so that is where they go now.
//!
//! The rest of the app does not need to know. Settings are read and written
//! through here, and a secret comes back in the same map as every other
//! setting. Where there is no store to use - a Linux session with no Secret
//! Service running, say - secrets stay in the database as they always did,
//! because credentials that silently fail to save are worse than credentials
//! kept the old way.

use std::collections::HashMap;

use crate::db;
use crate::error::Result;

/// The settings that are secrets. Usernames and the developer id are not:
/// they say who you are, not how to prove it.
pub const SECRET_KEYS: &[&str] = &["ss_devpassword", "ss_password", "tgdb_key"];

pub fn is_secret(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Somewhere secrets can be kept.
pub trait Vault: Send + Sync {
    /// The stored value, or `None` when there is none or the store cannot be
    /// reached. Either way the database is the next place to look.
    fn get(&self, key: &str) -> Option<String>;
    /// True when the value is now stored.
    fn set(&self, key: &str, value: &str) -> bool;
    /// True when nothing is stored under `key` any more.
    fn delete(&self, key: &str) -> bool;
}

/// The operating system's credential store.
pub struct Keychain {
    /// The app's identifier, so a build with another one keeps its own.
    service: String,
}

impl Keychain {
    pub fn new(service: &str) -> Self {
        Keychain {
            service: service.to_string(),
        }
    }

    fn entry(&self, key: &str) -> Option<keyring::Entry> {
        keyring::Entry::new(&self.service, key).ok()
    }
}

impl Vault for Keychain {
    fn get(&self, key: &str) -> Option<String> {
        self.entry(key)?.get_password().ok()
    }

    fn set(&self, key: &str, value: &str) -> bool {
        self.entry(key)
            .map(|e| e.set_password(value).is_ok())
            .unwrap_or(false)
    }

    fn delete(&self, key: &str) -> bool {
        match self.entry(key).map(|e| e.delete_credential()) {
            Some(Ok(())) | Some(Err(keyring::Error::NoEntry)) => true,
            _ => false,
        }
    }
}

/// Every setting, with secrets read from the vault where they live there.
pub fn all_settings(conn: &rusqlite::Connection, vault: &dyn Vault) -> Result<HashMap<String, String>> {
    let mut settings = db::all_settings(conn)?;
    for key in SECRET_KEYS {
        if let Some(value) = vault.get(key) {
            settings.insert(key.to_string(), value);
        }
    }
    Ok(settings)
}

/// Store one setting. A secret goes to the vault, and once it is safely
/// there, any copy in the database is removed rather than left behind.
pub fn save(conn: &rusqlite::Connection, vault: &dyn Vault, key: &str, value: &str) -> Result<()> {
    if is_secret(key) {
        let stored = if value.is_empty() {
            vault.delete(key)
        } else {
            vault.set(key, value)
        };
        if stored {
            return db::delete_setting(conn, key);
        }
    }
    db::set_setting(conn, key, value)
}

/// Move secrets an earlier version kept in the database into the vault.
///
/// Each is only removed from the database once the vault has taken it, so a
/// store that cannot be reached loses nothing: the next start tries again.
pub fn migrate(conn: &rusqlite::Connection, vault: &dyn Vault) -> Result<usize> {
    let mut moved = 0;
    for key in SECRET_KEYS {
        let Some(value) = db::get_setting(conn, key)? else {
            continue;
        };
        if value.is_empty() || vault.set(key, &value) {
            db::delete_setting(conn, key)?;
            moved += usize::from(!value.is_empty());
        }
    }
    Ok(moved)
}

/// Carry secrets saved under an earlier name of the app across to this one.
///
/// The credential store files each secret under the app's identifier, so a
/// rename leaves them behind under the old name, and the new one comes up
/// asking for passwords it was already given. Each secret is copied only when
/// this name has none of its own.
///
/// The old copy is removed only when `retire` says the old name has no
/// library left to use it - the library has moved here - and only once this
/// name holds a value of its own. A test build, or an older version still
/// installed and still holding its library, keeps its copy.
///
/// Returns how many were copied.
pub fn adopt(from: &dyn Vault, to: &dyn Vault, retire: bool) -> usize {
    let mut copied = 0;
    for key in SECRET_KEYS {
        let Some(value) = from.get(key) else {
            continue;
        };
        if to.get(key).is_none() {
            if !to.set(key, &value) || to.get(key).as_deref() != Some(value.as_str()) {
                continue;
            }
            copied += 1;
        }
        if retire {
            from.delete(key);
        }
    }
    copied
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A vault in memory, or one that refuses everything, standing in for a
    /// session with no credential store.
    #[derive(Default)]
    pub(crate) struct MemoryVault {
        pub values: Mutex<HashMap<String, String>>,
        pub broken: bool,
    }

    impl Vault for MemoryVault {
        fn get(&self, key: &str) -> Option<String> {
            if self.broken {
                return None;
            }
            self.values.lock().unwrap().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> bool {
            if self.broken {
                return false;
            }
            self.values.lock().unwrap().insert(key.into(), value.into());
            true
        }
        fn delete(&self, key: &str) -> bool {
            if self.broken {
                return false;
            }
            self.values.lock().unwrap().remove(key);
            true
        }
    }

    fn library(tag: &str) -> (std::path::PathBuf, rusqlite::Connection) {
        let dir = std::env::temp_dir().join(format!("arcadex-secrets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("library.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn a_secret_is_kept_in_the_vault_and_not_the_database() {
        let (dir, conn) = library("vault");
        let vault = MemoryVault::default();

        save(&conn, &vault, "ss_password", "hunter2").unwrap();
        save(&conn, &vault, "ss_user", "woblu").unwrap();

        assert_eq!(db::get_setting(&conn, "ss_password").unwrap(), None);
        assert_eq!(vault.get("ss_password").as_deref(), Some("hunter2"));
        // An ordinary setting is still an ordinary setting.
        assert_eq!(db::get_setting(&conn, "ss_user").unwrap().as_deref(), Some("woblu"));

        // Read back, it is one map like before.
        let all = all_settings(&conn, &vault).unwrap();
        assert_eq!(all.get("ss_password").map(String::as_str), Some("hunter2"));
        assert_eq!(all.get("ss_user").map(String::as_str), Some("woblu"));

        // Clearing the field clears the vault.
        save(&conn, &vault, "ss_password", "").unwrap();
        assert_eq!(vault.get("ss_password"), None);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_no_vault_to_use_secrets_stay_in_the_database() {
        let (dir, conn) = library("novault");
        let vault = MemoryVault {
            broken: true,
            ..Default::default()
        };
        save(&conn, &vault, "tgdb_key", "abc123").unwrap();
        assert_eq!(db::get_setting(&conn, "tgdb_key").unwrap().as_deref(), Some("abc123"));
        let all = all_settings(&conn, &vault).unwrap();
        assert_eq!(all.get("tgdb_key").map(String::as_str), Some("abc123"));

        // Nor does a start-up with no vault lose anything.
        assert_eq!(migrate(&conn, &vault).unwrap(), 0);
        assert_eq!(db::get_setting(&conn, "tgdb_key").unwrap().as_deref(), Some("abc123"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real store, under a service name of its own. Ignored by default
    /// because it writes to the machine's credential store; run it with
    /// `cargo test -- --ignored keychain` to check a platform's store works.
    #[test]
    #[ignore]
    fn the_systems_keychain_round_trips() {
        let vault = Keychain::new("com.boazv.arcadex.selftest");
        assert!(vault.set("tgdb_key", "round trip"), "the store refused a write");
        assert_eq!(vault.get("tgdb_key").as_deref(), Some("round trip"));
        assert!(vault.delete("tgdb_key"));
        assert_eq!(vault.get("tgdb_key"), None);
        // Deleting what is not there is still a success.
        assert!(vault.delete("tgdb_key"));
    }

    /// A library from before this moves its secrets over on the next start.
    #[test]
    fn moves_secrets_an_older_version_left_in_the_database() {
        let (dir, conn) = library("migrate");
        db::set_setting(&conn, "ss_devpassword", "devpass").unwrap();
        db::set_setting(&conn, "tgdb_key", "").unwrap();
        db::set_setting(&conn, "ss_devid", "arcadex").unwrap();
        let vault = MemoryVault::default();

        assert_eq!(migrate(&conn, &vault).unwrap(), 1);
        assert_eq!(db::get_setting(&conn, "ss_devpassword").unwrap(), None);
        assert_eq!(db::get_setting(&conn, "tgdb_key").unwrap(), None);
        assert_eq!(vault.get("ss_devpassword").as_deref(), Some("devpass"));
        // Not a secret, so not moved.
        assert_eq!(db::get_setting(&conn, "ss_devid").unwrap().as_deref(), Some("arcadex"));

        // Running again finds nothing left to do.
        assert_eq!(migrate(&conn, &vault).unwrap(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn holding(pairs: &[(&str, &str)]) -> MemoryVault {
        let vault = MemoryVault::default();
        for (k, v) in pairs {
            vault.set(k, v);
        }
        vault
    }

    /// After a rename, passwords saved under the old name come along, and
    /// the old copies go once the library has moved too.
    #[test]
    fn a_rename_carries_saved_passwords_across() {
        let old = holding(&[("ss_password", "hunter2"), ("tgdb_key", "abc")]);
        let new = MemoryVault::default();

        assert_eq!(adopt(&old, &new, true), 2);
        assert_eq!(new.get("ss_password").as_deref(), Some("hunter2"));
        assert_eq!(new.get("tgdb_key").as_deref(), Some("abc"));
        assert_eq!(old.get("ss_password"), None);
        assert_eq!(old.get("tgdb_key"), None);
    }

    /// One typed in since the rename is newer than the one left behind.
    #[test]
    fn a_password_given_since_the_rename_is_kept() {
        let old = holding(&[("ss_password", "old")]);
        let new = holding(&[("ss_password", "new")]);

        assert_eq!(adopt(&old, &new, true), 0);
        assert_eq!(new.get("ss_password").as_deref(), Some("new"));
        // The stale one is tidied away all the same.
        assert_eq!(old.get("ss_password"), None);
    }

    /// While the old name still has its library - a test build, or the old
    /// version still in use - it keeps its own copy.
    #[test]
    fn the_old_copy_stays_while_the_old_name_still_has_a_library() {
        let old = holding(&[("tgdb_key", "abc")]);
        let new = MemoryVault::default();

        assert_eq!(adopt(&old, &new, false), 1);
        assert_eq!(new.get("tgdb_key").as_deref(), Some("abc"));
        assert_eq!(old.get("tgdb_key").as_deref(), Some("abc"));
    }

    /// A store that will not take the copy leaves the original where it was.
    #[test]
    fn nothing_is_removed_when_the_copy_did_not_take() {
        let old = holding(&[("ss_devpassword", "dev")]);
        let new = MemoryVault {
            broken: true,
            ..Default::default()
        };

        assert_eq!(adopt(&old, &new, true), 0);
        assert_eq!(old.get("ss_devpassword").as_deref(), Some("dev"));
    }
}
