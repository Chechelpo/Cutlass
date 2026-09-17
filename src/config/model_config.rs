use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{error, info};
use zeroize::Zeroize;

use super::configs::get_config_location;
use super::{MasterKey, MasterKeyError};

/// Configuration for one model endpoint.
///
/// API keys are always retained as authenticated ciphertext. The master key is
/// process state and is intentionally excluded from serialized configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    name: String,
    host_url: String,
    id: String,
    encrypted_keys: Vec<String>,
    max_input_tokens: usize,
    max_output_tokens: usize,
    #[serde(default = "default_retry_amount")]
    retry_amount: u32,
    #[serde(default = "default_max_backoff")]
    max_backoff: Duration,

    #[serde(skip)]
    master_key: Option<Arc<MasterKey>>,
}

impl ModelConfig {
    /// Create a model config using Cutlass's automatically managed master key.
    pub fn new(
        name: impl Into<String>,
        host_url: impl Into<String>,
        id: impl Into<String>,
        max_input_tokens: usize,
        max_output_tokens: usize,
        retry_amount: u32,
        max_backoff: Duration,
    ) -> Result<Self, MasterKeyError> {
        let master_key = MasterKey::load_or_create_default()?;
        Ok(Self::with_master_key(
            name,
            host_url,
            id,
            max_input_tokens,
            max_output_tokens,
            retry_amount,
            max_backoff,
            master_key,
        ))
    }

    /// Create a model config with a master key at an explicit path.
    ///
    /// This is useful for portable installations and tests. The key file is
    /// created automatically when it does not exist.
    pub fn with_master_key_file(
        name: impl Into<String>,
        host_url: impl Into<String>,
        id: impl Into<String>,
        max_input_tokens: usize,
        max_output_tokens: usize,
        retry_amount: u32,
        max_backoff: Duration,
        master_key_path: impl AsRef<Path>,
    ) -> Result<Self, MasterKeyError> {
        let master_key = MasterKey::load_or_create(master_key_path)?;
        Ok(Self::with_master_key(
            name,
            host_url,
            id,
            max_input_tokens,
            max_output_tokens,
            retry_amount,
            max_backoff,
            master_key,
        ))
    }

    fn with_master_key(
        name: impl Into<String>,
        host_url: impl Into<String>,
        id: impl Into<String>,
        max_input_tokens: usize,
        max_output_tokens: usize,
        retry_amount: u32,
        max_backoff: Duration,
        master_key: MasterKey,
    ) -> Self {
        Self {
            name: name.into(),
            host_url: host_url.into(),
            id: id.into(),
            encrypted_keys: Vec::new(),
            max_input_tokens,
            max_output_tokens,
            retry_amount,
            max_backoff,
            master_key: Some(Arc::new(master_key)),
        }
    }

    /// Attach the automatically managed master key after deserialization.
    pub fn unlock(&mut self) -> Result<(), MasterKeyError> {
        self.master_key = Some(Arc::new(MasterKey::load_or_create_default()?));
        Ok(())
    }

    /// Attach a master key from an explicit file after deserialization.
    pub fn unlock_with_master_key_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), MasterKeyError> {
        self.master_key = Some(Arc::new(MasterKey::load_or_create(path)?));
        Ok(())
    }

    /// Decrypt every configured API key with the config's master key.
    pub fn decrypted_keys(&self) -> Result<Vec<String>, MasterKeyError> {
        let master_key = self.master_key()?;
        self.encrypted_keys
            .iter()
            .map(|key| master_key.decrypt(key))
            .collect()
    }

    /// Encrypt and retain an API key. Plaintext is never stored in the config.
    pub fn add_key(&mut self, key: &str) -> Result<(), MasterKeyError> {
        if key.is_empty() {
            return Err(MasterKeyError::InvalidPlaintext(
                "API key cannot be empty".into(),
            ));
        }
        let encrypted = self.master_key()?.encrypt(key)?;
        self.encrypted_keys.push(encrypted);
        Ok(())
    }

    pub fn host_url(&self) -> &str {
        &self.host_url
    }

    /// Human-readable profile name used to distinguish model configurations.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn max_input_tokens(&self) -> usize {
        self.max_input_tokens
    }

    pub fn max_output_tokens(&self) -> usize {
        self.max_output_tokens
    }

    pub fn retry_amount(&self) -> u32 {
        self.retry_amount
    }

    pub fn max_backoff(&self) -> Duration {
        self.max_backoff
    }

    pub fn encrypted_keys(&self) -> &[String] {
        &self.encrypted_keys
    }

    pub(super) fn remove_last_key(&mut self) {
        self.encrypted_keys.pop();
    }

    fn master_key(&self) -> Result<&MasterKey, MasterKeyError> {
        self.master_key
            .as_deref()
            .ok_or(MasterKeyError::LockedConfig)
    }
}

impl fmt::Display for ModelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) - {}",
            self.name,
            self.id,
            self.host_url,
        )
    }
}

fn default_retry_amount() -> u32 {
    4
}

fn default_max_backoff() -> Duration {
    Duration::from_secs(30)
}

/// Errors raised while loading or persisting model profiles.
#[derive(Debug)]
pub enum ModelConfigStoreError {
    ConfigDirectoryUnavailable,
    Io(std::io::Error),
    Decode(toml::de::Error),
    Encode(toml::ser::Error),
    Crypto(MasterKeyError),
    UnsupportedVersion(u32),
    InvalidProfileName,
    DuplicateProfile(String),
    ProfileNotFound(String),
    NoActiveProfile,
    SharedKeyDirectory,
}

impl fmt::Display for ModelConfigStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigDirectoryUnavailable => {
                write!(f, "could not determine the Cutlass config directory")
            }
            Self::Io(error) => write!(f, "model-config store IO error: {error}"),
            Self::Decode(error) => write!(f, "invalid model-config store: {error}"),
            Self::Encode(error) => write!(f, "could not encode model-config store: {error}"),
            Self::Crypto(error) => write!(f, "model-config encryption error: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported model-config store version {version}")
            }
            Self::InvalidProfileName => write!(f, "model profile name cannot be empty"),
            Self::DuplicateProfile(name) => write!(f, "model profile {name:?} already exists"),
            Self::ProfileNotFound(name) => write!(f, "model profile {name:?} does not exist"),
            Self::NoActiveProfile => write!(f, "no active model profile is configured"),
            Self::SharedKeyDirectory => write!(
                f,
                "model configs and the master key must use separate directories"
            ),
        }
    }
}

impl std::error::Error for ModelConfigStoreError {}

impl From<std::io::Error> for ModelConfigStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<MasterKeyError> for ModelConfigStoreError {
    fn from(error: MasterKeyError) -> Self {
        Self::Crypto(error)
    }
}

#[derive(Serialize, Deserialize)]
struct StoredModelConfigs {
    version: u32,
    active_config: Option<String>,
    #[serde(default, rename = "model")]
    configs: Vec<ModelConfig>,
}

/// Named model configurations persisted in the orchestrator's Cutlass config folder.
pub struct ModelConfigStore {
    configs: Vec<ModelConfig>,
    active_config: String,
    path: PathBuf,
    master_key_path: PathBuf,
}

/// Values required to create a complete model profile in one operation.
pub struct NewModelProfile {
    pub name: String,
    pub host_url: String,
    pub model_id: String,
    pub api_key: String,
    pub max_input_tokens: usize,
    pub max_output_tokens: usize,
    pub retry_amount: u32,
    pub max_backoff: Duration,
}

impl Drop for NewModelProfile {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

impl ModelConfigStore {
    /// Load the platform-standard store, creating it on first use.
    pub fn new() -> Result<Self, ModelConfigStoreError> {
        let config_directory =
            get_config_location().ok_or(ModelConfigStoreError::ConfigDirectoryUnavailable)?;
        let directories = directories::ProjectDirs::from("dev", "cutlass", "Cutlass")
            .ok_or(ModelConfigStoreError::ConfigDirectoryUnavailable)?;

        Self::open(config_directory, directories.data_local_dir())
    }

    /// Load a store from explicit, separate config and private-data
    /// directories, primarily for portable setups and tests.
    pub fn open(
        config_directory: impl AsRef<Path>,
        key_directory: impl AsRef<Path>,
    ) -> Result<Self, ModelConfigStoreError> {
        info!("Opening model config store");

        let config_directory = config_directory.as_ref();
        let key_directory = key_directory.as_ref();

        if config_directory == key_directory {
            error!("Config directory and key directory are the same");
            return Err(ModelConfigStoreError::SharedKeyDirectory);
        }

        create_private_directory(config_directory)?;
        create_private_directory(key_directory)?;

        let path = config_directory.join("models.toml");
        let master_key_path = key_directory.join("master.key");

        let (mut configs, active_config) = if path.exists() {
            let source = fs::read_to_string(&path)?;

            let stored: StoredModelConfigs =
                toml::from_str(&source).map_err(ModelConfigStoreError::Decode)?;

            if stored.version != 1 {
                error!(
                version = stored.version,
                "Unsupported model config version"
            );
                return Err(ModelConfigStoreError::UnsupportedVersion(stored.version));
            }

            validate_loaded_profiles(&stored.configs, stored.active_config.as_deref())?;

            (stored.configs, stored.active_config.unwrap_or_default())
        } else {
            (Vec::new(), String::new())
        };

        MasterKey::load_or_create(&master_key_path)?;

        for config in &mut configs {
            config.unlock_with_master_key_file(&master_key_path)?;
        }

        let store = Self {
            configs,
            active_config,
            path,
            master_key_path,
        };

        if !store.path.exists() {
            store.persist()?;
        }

        info!(
            profiles = store.configs.len(),
            "Model config store opened"
        );

        Ok(store)
    }

    /// Returns currently active configuration
    pub fn active_config(&self) -> Result<&ModelConfig, ModelConfigStoreError> {
        self.configs
            .iter()
            .find(|c| c.name() == self.active_config)
            .ok_or(ModelConfigStoreError::NoActiveProfile)
    }

    pub fn configs(&self) -> &[ModelConfig] {
        self.configs.as_slice()
    }

    pub fn get(&self, name: &str) -> Option<&ModelConfig> {
        self.configs.iter().find(|config| config.name() == name)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create a profile and persist it immediately. The first profile becomes
    /// active automatically.
    pub fn create(
        &mut self,
        name: impl Into<String>,
        host_url: impl Into<String>,
        id: impl Into<String>,
        max_input_tokens: usize,
        max_output_tokens: usize,
        retry_amount: u32,
        max_backoff: Duration,
    ) -> Result<&ModelConfig, ModelConfigStoreError> {
        let name = name.into();
        validate_new_profile(&self.configs, &name)?;
        let config = ModelConfig::with_master_key_file(
            &name,
            host_url,
            id,
            max_input_tokens,
            max_output_tokens,
            retry_amount,
            max_backoff,
            &self.master_key_path,
        )?;
        info!("New connection profile {} created: \n{}", config.name, config);
        let previous_active = self.active_config.clone();
        if self.configs.is_empty() {
            self.active_config = name;
        }
        self.configs.push(config);
        if let Err(error) = self.persist() {
            self.configs.pop();
            self.active_config = previous_active;
            return Err(error);
        }
        Ok(self.configs.last().expect("a config was just inserted"))
    }

    /// Creates and persists a profile and its encrypted API key atomically.
    pub fn create_profile(
        &mut self,
        profile: NewModelProfile,
    ) -> Result<&ModelConfig, ModelConfigStoreError> {
        info!(profile = %profile.name, "Creating model profile");

        validate_new_profile(&self.configs, &profile.name)?;

        let mut config = ModelConfig::with_master_key_file(
            &profile.name,
            &profile.host_url,
            &profile.model_id,
            profile.max_input_tokens,
            profile.max_output_tokens,
            profile.retry_amount,
            profile.max_backoff,
            &self.master_key_path,
        )?;

        config.add_key(&profile.api_key)?;

        let previous_active = self.active_config.clone();

        if self.configs.is_empty() {
            self.active_config = profile.name.clone();
        }

        self.configs.push(config);

        if let Err(error) = self.persist() {
            error!(
                profile = %profile.name,
                error = %error,
                "Failed to persist model profile, rolling back"
            );

            self.configs.pop();
            self.active_config = previous_active;

            return Err(error);
        }

        info!(profile = %profile.name, "Model profile created");

        Ok(self.configs.last().expect("a config was just inserted"))
    }

    /// Select a profile and persist the selection immediately.
    pub fn set_active(&mut self, name: &str) -> Result<(), ModelConfigStoreError> {
        self.profile_index(name)?;
        let previous = std::mem::replace(&mut self.active_config, name.into());
        if let Err(error) = self.persist() {
            self.active_config = previous;
            error!("No profile with name {} to set active", name);
            return Err(error);
        }
        info!("New active connection is {}", name);
        Ok(())
    }

    /// Encrypt a new API key into a profile and persist it immediately.
    pub fn add_key(&mut self, profile: &str, key: &str) -> Result<(), ModelConfigStoreError> {
        let index = self.profile_index(profile)?;
        self.configs[index].add_key(key)?;
        if let Err(error) = self.persist() {
            self.configs[index].remove_last_key();
            error!("No profile with name {} when adding key", profile);
            return Err(error);
        }
        info!("Added new key to profile {}", profile);
        Ok(())
    }

    /// Remove a profile and persist the deletion immediately.
    pub fn remove(&mut self, name: &str) -> Result<ModelConfig, ModelConfigStoreError> {
        let index = self.profile_index(name)?;
        let removed = self.configs.remove(index);
        let previous_active = self.active_config.clone();
        if previous_active == name {
            self.active_config = self
                .configs
                .first()
                .map(|config| config.name().to_string())
                .unwrap_or_default();
        }
        if let Err(error) = self.persist() {
            self.configs.insert(index, removed);
            self.active_config = previous_active;
            error!("No connection with name {} to remove", name);
            return Err(error);
        }
        info!("Removed connection profile {}", removed.name);
        Ok(removed)
    }

    /// Persist the complete current store as versioned TOML.
    pub fn persist(&self) -> Result<(), ModelConfigStoreError> {
        let stored = StoredModelConfigs {
            version: 1,
            active_config: (!self.active_config.is_empty()).then(|| self.active_config.clone()),
            configs: self.configs.clone(),
        };
        let encoded = toml::to_string_pretty(&stored).map_err(ModelConfigStoreError::Encode)?;
        write_private_file(&self.path, encoded.as_bytes())?;
        Ok(())
    }

    fn profile_index(&self, name: &str) -> Result<usize, ModelConfigStoreError> {
        self.configs
            .iter()
            .position(|config| config.name() == name)
            .ok_or_else(|| ModelConfigStoreError::ProfileNotFound(name.into()))
    }
}

fn validate_new_profile(configs: &[ModelConfig], name: &str) -> Result<(), ModelConfigStoreError> {
    if name.trim().is_empty() {
        return Err(ModelConfigStoreError::InvalidProfileName);
    }
    if configs.iter().any(|config| config.name() == name) {
        return Err(ModelConfigStoreError::DuplicateProfile(name.into()));
    }
    Ok(())
}

fn validate_loaded_profiles(
    configs: &[ModelConfig],
    active: Option<&str>,
) -> Result<(), ModelConfigStoreError> {
    for (index, config) in configs.iter().enumerate() {
        validate_new_profile(&configs[..index], config.name())?;
    }
    if let Some(active) = active {
        if !configs.iter().any(|config| config.name() == active) {
            return Err(ModelConfigStoreError::ProfileNotFound(active.into()));
        }
    }
    Ok(())
}

fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    set_private_file_mode(&mut options);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn create_private_directory(path: &Path) -> Result<(), std::io::Error> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    set_private_directory_mode(&mut builder);
    builder.create(path)
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_key_file(test_name: &str) -> std::path::PathBuf {
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
    fn model_keys_round_trip_without_serializing_plaintext() {
        let key_file = temporary_key_file("model-config");
        let mut config = ModelConfig::with_master_key_file(
            "primary",
            "https://example.test/v1/chat/completions",
            "model-id",
            1000,
            200,
            4,
            Duration::from_secs(30),
            &key_file,
        )
        .unwrap();
        config.add_key("secret-api-key").unwrap();

        let serialized = serde_json::to_string(&config).unwrap();

        assert!(!serialized.contains("secret-api-key"));
        assert_eq!(config.name(), "primary");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serialized).unwrap()["name"],
            "primary"
        );
        assert_eq!(config.decrypted_keys().unwrap(), ["secret-api-key"]);
        fs::remove_dir_all(key_file.parent().unwrap()).unwrap();
    }

    #[test]
    fn deserialized_config_must_be_unlocked() {
        let key_file = temporary_key_file("model-unlock");
        let mut original = ModelConfig::with_master_key_file(
            "fallback",
            "https://example.test",
            "model-id",
            1000,
            200,
            4,
            Duration::from_secs(30),
            &key_file,
        )
        .unwrap();
        original.add_key("secret-api-key").unwrap();
        let serialized = serde_json::to_string(&original).unwrap();
        let mut restored: ModelConfig = serde_json::from_str(&serialized).unwrap();

        assert!(matches!(
            restored.decrypted_keys(),
            Err(MasterKeyError::LockedConfig)
        ));
        restored.unlock_with_master_key_file(&key_file).unwrap();
        assert_eq!(restored.name(), "fallback");
        assert_eq!(restored.decrypted_keys().unwrap(), ["secret-api-key"]);
        fs::remove_dir_all(key_file.parent().unwrap()).unwrap();
    }

    #[test]
    fn store_persists_profiles_and_keys_in_separate_directories() {
        let root = temporary_key_file("config-store")
            .parent()
            .unwrap()
            .to_path_buf();
        let config_directory = root.join("config");
        let key_directory = root.join("private-data");
        let mut store = ModelConfigStore::open(&config_directory, &key_directory).unwrap();

        store
            .create(
                "primary",
                "https://example.test/v1/chat/completions",
                "model-id",
                10_000,
                1_000,
                5,
                Duration::from_secs(45),
            )
            .unwrap();
        store.add_key("primary", "secret-api-key").unwrap();
        drop(store);

        assert!(config_directory.join("models.toml").is_file());
        assert!(!config_directory.join("master.key").exists());
        assert!(key_directory.join("master.key").is_file());
        assert!(!key_directory.join("models.toml").exists());

        let restored = ModelConfigStore::open(&config_directory, &key_directory).unwrap();
        assert_eq!(restored.active_config().unwrap().name(), "primary");
        assert_eq!(restored.active_config().unwrap().retry_amount(), 5);
        assert_eq!(
            restored.active_config().unwrap().max_backoff(),
            Duration::from_secs(45)
        );
        assert_eq!(
            restored.get("primary").unwrap().decrypted_keys().unwrap(),
            ["secret-api-key"]
        );
        let serialized = fs::read_to_string(restored.path()).unwrap();
        assert!(!serialized.contains("secret-api-key"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_creates_a_complete_encrypted_profile_atomically() {
        let root = temporary_key_file("atomic-profile")
            .parent()
            .unwrap()
            .to_path_buf();
        let config_directory = root.join("config");
        let key_directory = root.join("private-data");
        let mut store = ModelConfigStore::open(&config_directory, &key_directory).unwrap();

        store
            .create_profile(NewModelProfile {
                name: "primary".into(),
                host_url: "https://example.test/v1".into(),
                model_id: "model-id".into(),
                api_key: "secret-api-key".into(),
                max_input_tokens: 10_000,
                max_output_tokens: 1_000,
                retry_amount: 4,
                max_backoff: Duration::from_secs(30),
            })
            .unwrap();

        assert_eq!(
            store.active_config().unwrap().decrypted_keys().unwrap(),
            ["secret-api-key"]
        );
        assert!(
            !fs::read_to_string(store.path())
                .unwrap()
                .contains("secret-api-key")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_persists_active_profile_changes_and_removals() {
        let root = temporary_key_file("config-store-mutations")
            .parent()
            .unwrap()
            .to_path_buf();
        let config_directory = root.join("config");
        let key_directory = root.join("private-data");
        let mut store = ModelConfigStore::open(&config_directory, &key_directory).unwrap();
        store
            .create(
                "first",
                "https://one.test",
                "model-one",
                100,
                10,
                4,
                Duration::from_secs(30),
            )
            .unwrap();
        store
            .create(
                "second",
                "https://two.test",
                "model-two",
                200,
                20,
                4,
                Duration::from_secs(30),
            )
            .unwrap();
        store.set_active("second").unwrap();
        store.remove("first").unwrap();
        drop(store);

        let restored = ModelConfigStore::open(&config_directory, &key_directory).unwrap();
        assert_eq!(restored.configs().len(), 1);
        assert_eq!(restored.active_config().unwrap().name(), "second");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_rejects_a_shared_config_and_key_directory() {
        let directory = temporary_key_file("shared-store-directory")
            .parent()
            .unwrap()
            .to_path_buf();

        assert!(matches!(
            ModelConfigStore::open(&directory, &directory),
            Err(ModelConfigStoreError::SharedKeyDirectory)
        ));
    }
}
