//! Persistent master-key management for encrypted configuration secrets.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use directories::ProjectDirs;
use rand::RngExt;
use zeroize::{Zeroize, Zeroizing};

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const KEY_FILE_VERSION: &str = "cutlass-master-key-v1";
const CIPHERTEXT_VERSION: &str = "v1";

/// A process-local master key loaded from Cutlass's private key file.
///
/// The bytes are erased when the value is dropped. Clone is intentionally not
/// implemented; callers that need shared ownership should wrap it in `Arc`.
pub struct MasterKey {
    bytes: Zeroizing<[u8; KEY_BYTES]>,
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MasterKey([REDACTED])")
    }
}

/// Errors from key creation, storage, encryption, and decryption.
#[derive(Debug)]
pub enum MasterKeyError {
    ConfigDirectoryUnavailable,
    Io(std::io::Error),
    InvalidKeyFile(String),
    InvalidCiphertext(String),
    InvalidPlaintext(String),
    EncryptionFailed,
    DecryptionFailed,
    LockedConfig,
}

impl fmt::Display for MasterKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigDirectoryUnavailable => {
                write!(f, "could not determine the Cutlass config directory")
            }
            Self::Io(error) => write!(f, "master-key IO error: {error}"),
            Self::InvalidKeyFile(message) => write!(f, "invalid master-key file: {message}"),
            Self::InvalidCiphertext(message) => write!(f, "invalid encrypted key: {message}"),
            Self::InvalidPlaintext(message) => write!(f, "invalid plaintext: {message}"),
            Self::EncryptionFailed => write!(f, "could not encrypt API key"),
            Self::DecryptionFailed => write!(f, "could not decrypt API key"),
            Self::LockedConfig => write!(f, "model config has not been unlocked"),
        }
    }
}

impl std::error::Error for MasterKeyError {}

impl From<std::io::Error> for MasterKeyError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl MasterKey {
    /// Load the default key, creating it on first use.
    ///
    /// The default path is Cutlass's platform-specific local-data directory,
    /// kept separate from ordinary configuration files.
    pub fn load_or_create_default() -> Result<Self, MasterKeyError> {
        let directories = ProjectDirs::from("dev", "cutlass", "Cutlass")
            .ok_or(MasterKeyError::ConfigDirectoryUnavailable)?;
        Self::load_or_create(directories.data_local_dir().join("master.key"))
    }

    /// Load a master key from `path`, atomically creating one when absent.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, MasterKeyError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            create_private_directory(parent)?;
        }

        match create_key_file(path) {
            Ok(bytes) => Ok(Self {
                bytes: Zeroizing::new(bytes),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Self::load(path),
            Err(error) => Err(MasterKeyError::Io(error)),
        }
    }

    fn load(path: &Path) -> Result<Self, MasterKeyError> {
        ensure_private_permissions(path)?;
        let mut encoded = String::new();
        fs::File::open(path)?.read_to_string(&mut encoded)?;
        let (version, payload) = encoded
            .trim()
            .split_once(':')
            .ok_or_else(|| MasterKeyError::InvalidKeyFile("missing format version".into()))?;
        if version != KEY_FILE_VERSION {
            return Err(MasterKeyError::InvalidKeyFile(format!(
                "unsupported version {version:?}"
            )));
        }
        let mut decoded = URL_SAFE_NO_PAD.decode(payload).map_err(|_| {
            MasterKeyError::InvalidKeyFile("key payload is not valid base64".into())
        })?;
        if decoded.len() != KEY_BYTES {
            decoded.zeroize();
            return Err(MasterKeyError::InvalidKeyFile(format!(
                "expected {KEY_BYTES} key bytes"
            )));
        }
        let mut bytes = [0_u8; KEY_BYTES];
        bytes.copy_from_slice(&decoded);
        decoded.zeroize();
        Ok(Self {
            bytes: Zeroizing::new(bytes),
        })
    }

    /// Encrypt one secret using AES-256-GCM and a fresh random nonce.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, MasterKeyError> {
        let cipher = Aes256Gcm::new_from_slice(self.bytes.as_ref())
            .map_err(|_| MasterKeyError::EncryptionFailed)?;
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        rand::rng().fill(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .map_err(|_| MasterKeyError::EncryptionFailed)?;
        Ok(format!(
            "{CIPHERTEXT_VERSION}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce_bytes),
            URL_SAFE_NO_PAD.encode(ciphertext),
        ))
    }

    /// Authenticate and decrypt one ciphertext created by [`Self::encrypt`].
    pub fn decrypt(&self, encrypted: &str) -> Result<String, MasterKeyError> {
        let mut parts = encrypted.split('.');
        let version = parts.next();
        let nonce = parts.next();
        let ciphertext = parts.next();
        if version != Some(CIPHERTEXT_VERSION)
            || nonce.is_none()
            || ciphertext.is_none()
            || parts.next().is_some()
        {
            return Err(MasterKeyError::InvalidCiphertext(
                "expected v1.<nonce>.<ciphertext>".into(),
            ));
        }
        let nonce = URL_SAFE_NO_PAD
            .decode(nonce.unwrap())
            .map_err(|_| MasterKeyError::InvalidCiphertext("nonce is not valid base64".into()))?;
        if nonce.len() != NONCE_BYTES {
            return Err(MasterKeyError::InvalidCiphertext(format!(
                "expected a {NONCE_BYTES}-byte nonce"
            )));
        }
        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext.unwrap()).map_err(|_| {
            MasterKeyError::InvalidCiphertext("ciphertext is not valid base64".into())
        })?;
        let cipher = Aes256Gcm::new_from_slice(self.bytes.as_ref())
            .map_err(|_| MasterKeyError::DecryptionFailed)?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| MasterKeyError::DecryptionFailed)?;
        String::from_utf8(plaintext).map_err(|error| {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            MasterKeyError::DecryptionFailed
        })
    }
}

fn create_key_file(path: &Path) -> Result<[u8; KEY_BYTES], std::io::Error> {
    let mut bytes = [0_u8; KEY_BYTES];
    rand::rng().fill(&mut bytes);
    let encoded = format!("{KEY_FILE_VERSION}:{}\n", URL_SAFE_NO_PAD.encode(bytes));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    set_private_file_mode(&mut options);
    match options.open(path).and_then(|mut file| {
        file.write_all(encoded.as_bytes())?;
        file.sync_all()
    }) {
        Ok(()) => Ok(bytes),
        Err(error) => {
            bytes.zeroize();
            Err(error)
        }
    }
}

fn create_private_directory(path: &Path) -> Result<(), MasterKeyError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    set_private_directory_mode(&mut builder);
    builder.create(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_file_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_directory_mode(builder: &mut fs::DirBuilder) {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
}

#[cfg(not(unix))]
fn set_private_directory_mode(_builder: &mut fs::DirBuilder) {}

#[cfg(unix)]
fn ensure_private_permissions(path: &Path) -> Result<(), MasterKeyError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(MasterKeyError::InvalidKeyFile(
            "permissions must not grant group or other access".into(),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_permissions(_path: &Path) -> Result<(), MasterKeyError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_key_file(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "cutlass-{test_name}-{}-{unique}",
                std::process::id()
            ))
            .join("master.key")
    }

    #[test]
    fn creates_and_reuses_the_same_master_key() {
        let path = temporary_key_file("master-key");
        let first = MasterKey::load_or_create(&path).unwrap();
        let ciphertext = first.encrypt("secret").unwrap();
        let second = MasterKey::load_or_create(&path).unwrap();

        assert_eq!(second.decrypt(&ciphertext).unwrap(), "secret");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn authenticated_encryption_rejects_tampering() {
        let path = temporary_key_file("tamper");
        let key = MasterKey::load_or_create(&path).unwrap();
        let mut ciphertext = key.encrypt("secret").unwrap();
        ciphertext.push('A');

        assert!(matches!(
            key.decrypt(&ciphertext),
            Err(MasterKeyError::DecryptionFailed)
        ));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
