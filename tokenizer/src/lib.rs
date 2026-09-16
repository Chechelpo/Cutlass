//! Local token counting for the model families supported by Cutlass.
//!
//! Tokenizers are selected from a model id, loaded lazily, and reused.  The
//! process-wide [`tokenize`] function looks for assets in
//! `$CITRA_INSTALL_ROOT/tokenizers`; when that variable is unset it uses the
//! `tokenizer_data` directory shipped with this crate.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use rustc_hash::FxHashMap;
use sentencepiece_rs::SentencePieceProcessor;
use tiktoken_rs::{CoreBPE, Rank};
use tokenizers::Tokenizer;

const RESULT_CACHE_CAPACITY: usize = 1024;

/// A tokenizer vocabulary supported by this crate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelFamily {
    Claude,
    DeepseekV4,
    Gemma,
    Glm,
    Jamba,
    Llama3,
    Llama,
    Mistral,
    Nerdstash,
    NerdstashV2,
    Tiktoken,
    Yi,
    Nemotron3,
}

impl ModelFamily {
    /// The tokenizer asset associated with this family.
    pub const fn filename(self) -> &'static str {
        match self {
            Self::Claude => "claude.json",
            Self::DeepseekV4 => "deepseekv4.json",
            Self::Gemma => "gemma.model",
            Self::Glm => "glm.json",
            Self::Jamba => "jamba.model",
            Self::Llama3 => "llama3.json",
            Self::Llama => "llama.model",
            Self::Mistral => "mistral.model",
            Self::Nerdstash => "nerdstash.model",
            Self::NerdstashV2 => "nerdstash_v2.model",
            Self::Tiktoken => "tiktoken.model",
            Self::Yi => "yi.model",
            Self::Nemotron3 => "nemotron3.json",
        }
    }
}

impl fmt::Display for ModelFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Claude => "claude",
            Self::DeepseekV4 => "deepseekv4",
            Self::Gemma => "gemma",
            Self::Glm => "glm",
            Self::Jamba => "jamba",
            Self::Llama3 => "llama3",
            Self::Llama => "llama",
            Self::Mistral => "mistral",
            Self::Nerdstash => "nerdstash",
            Self::NerdstashV2 => "nerdstash_v2",
            Self::Tiktoken => "tiktoken",
            Self::Yi => "yi",
            Self::Nemotron3 => "nemotron3",
        };
        formatter.write_str(name)
    }
}

/// Select the best local tokenizer for a model id.
///
/// Matching is deliberately ordered from specific to general. Unknown models
/// use the Llama 3 tokenizer, matching the behavior of the reference module.
pub fn model_family(model_id: &str) -> ModelFamily {
    let model = model_id.trim().to_lowercase();
    let normalized = normalize_model_id(&model);

    if normalized.contains("claude") || normalized.contains("anthropic") {
        ModelFamily::Claude
    } else if normalized.contains("deepseek") {
        ModelFamily::DeepseekV4
    } else if normalized.contains("gemma") {
        ModelFamily::Gemma
    } else if contains_any(&normalized, &["chatglm", "glm-4", "glm4", "glm-3", "glm3"]) {
        ModelFamily::Glm
    } else if normalized.contains("jamba") {
        ModelFamily::Jamba
    } else if contains_any(
        &normalized,
        &["nerdstash-v2", "nerdstash-v-2", "nerdstash2", "nerdstash-2"],
    ) {
        ModelFamily::NerdstashV2
    } else if normalized.contains("nerdstash") {
        ModelFamily::Nerdstash
    } else if normalized.contains("codellama") || normalized.contains("code-llama") {
        // Names such as CodeLlama-34b contain the substring `llama-3`, but
        // CodeLlama uses the older SentencePiece vocabulary.
        ModelFamily::Llama
    } else if contains_any(
        &normalized,
        &[
            "llama-3",
            "llama3",
            "llama-4",
            "llama4",
            "meta-llama-3",
            "meta-llama-4",
        ],
    ) {
        ModelFamily::Llama3
    } else if normalized.contains("llama") {
        ModelFamily::Llama
    } else if contains_any(&normalized, &["mistral", "mixtral", "ministral"]) {
        ModelFamily::Mistral
    } else if normalized == "yi"
        || normalized.starts_with("yi-")
        || model.contains("/yi-")
        || normalized.contains("01-ai")
    {
        ModelFamily::Yi
    } else if contains_any(
        &normalized,
        &[
            "gpt-",
            "chatgpt",
            "text-davinci",
            "text-embedding",
            "o1",
            "o3",
            "o4",
        ],
    ) {
        ModelFamily::Tiktoken
    } else if normalized.contains("nemotron-3") || normalized.contains("nemotron3") {
        ModelFamily::Nemotron3
    } else {
        ModelFamily::Llama3
    }
}

fn normalize_model_id(model: &str) -> String {
    let mut normalized = String::with_capacity(model.len());
    let mut last_was_separator = true;

    for character in model.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            normalized.push('-');
            last_was_separator = true;
        }
    }

    if normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

/// An error raised while locating, loading, or using a tokenizer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerError {
    message: String,
}

impl TokenizerError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TokenizerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TokenizerError {}

enum Backend {
    HuggingFace(Box<Tokenizer>),
    SentencePiece(Box<SentencePieceProcessor>),
    Tiktoken(CoreBPE),
}

impl Backend {
    fn count(&self, text: &str) -> Result<usize, TokenizerError> {
        match self {
            Self::HuggingFace(tokenizer) => tokenizer
                .encode(text, false)
                .map(|encoding| encoding.len())
                .map_err(|error| TokenizerError::new(format!("could not encode text: {error}"))),
            Self::SentencePiece(tokenizer) => tokenizer
                .encode_to_ids(text)
                .map(|tokens| tokens.len())
                .map_err(|error| TokenizerError::new(format!("could not encode text: {error}"))),
            Self::Tiktoken(tokenizer) => Ok(tokenizer.encode_ordinary(text).len()),
        }
    }
}

#[derive(Default)]
struct ResultCache {
    values: HashMap<(ModelFamily, String), usize>,
    insertion_order: VecDeque<(ModelFamily, String)>,
}

impl ResultCache {
    fn get(&self, family: ModelFamily, text: &str) -> Option<usize> {
        self.values.get(&(family, text.to_owned())).copied()
    }

    fn insert(&mut self, family: ModelFamily, text: &str, count: usize) {
        let key = (family, text.to_owned());
        if self.values.contains_key(&key) {
            return;
        }

        if self.values.len() == RESULT_CACHE_CAPACITY
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.values.remove(&oldest);
        }
        self.values.insert(key.clone(), count);
        self.insertion_order.push_back(key);
    }
}

/// Lazily loaded tokenizer collection rooted at a tokenizer asset directory.
pub struct TokenizerRegistry {
    root: PathBuf,
    tokenizers: Mutex<HashMap<ModelFamily, Arc<Backend>>>,
    results: Mutex<ResultCache>,
}

impl TokenizerRegistry {
    /// Create a registry using tokenizer assets directly inside `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, TokenizerError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(TokenizerError::new(format!(
                "tokenizer directory does not exist: {}",
                root.display()
            )));
        }
        Ok(Self {
            root,
            tokenizers: Mutex::new(HashMap::new()),
            results: Mutex::new(ResultCache::default()),
        })
    }

    /// Build a registry from `$CITRA_INSTALL_ROOT/tokenizers`, falling back to
    /// this crate's bundled `tokenizer_data` directory when the variable is
    /// not set.
    pub fn from_environment() -> Result<Self, TokenizerError> {
        let root = match env::var_os("CITRA_INSTALL_ROOT") {
            Some(install_root) => PathBuf::from(install_root).join("tokenizers"),
            None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tokenizer_data"),
        };
        Self::new(root)
    }

    /// Directory containing this registry's tokenizer assets.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Count tokens after selecting a tokenizer from `model_id`.
    pub fn tokenize(&self, model_id: &str, text: &str) -> Result<usize, TokenizerError> {
        self.tokenize_for_family(model_family(model_id), text)
    }

    /// Count tokens with an explicitly selected tokenizer family.
    pub fn tokenize_for_family(
        &self,
        family: ModelFamily,
        text: &str,
    ) -> Result<usize, TokenizerError> {
        if let Some(count) = self
            .results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(family, text)
        {
            return Ok(count);
        }

        let tokenizer = self.get_or_load(family)?;
        let count = tokenizer.count(text)?;
        self.results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(family, text, count);
        Ok(count)
    }

    fn get_or_load(&self, family: ModelFamily) -> Result<Arc<Backend>, TokenizerError> {
        let mut tokenizers = self
            .tokenizers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(tokenizer) = tokenizers.get(&family) {
            return Ok(Arc::clone(tokenizer));
        }

        let path = self.root.join(family.filename());
        if !path.is_file() {
            return Err(TokenizerError::new(format!(
                "tokenizer file does not exist: {}",
                path.display()
            )));
        }

        let tokenizer = Arc::new(load_backend(family, &path)?);
        tokenizers.insert(family, Arc::clone(&tokenizer));
        Ok(tokenizer)
    }
}

fn load_backend(family: ModelFamily, path: &Path) -> Result<Backend, TokenizerError> {
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return Tokenizer::from_file(path)
            .map(Box::new)
            .map(Backend::HuggingFace)
            .map_err(|error| {
                TokenizerError::new(format!(
                    "could not load {} tokenizer from {}: {error}",
                    family,
                    path.display()
                ))
            });
    }

    if family == ModelFamily::Tiktoken {
        return load_tiktoken(path).map(Backend::Tiktoken);
    }

    SentencePieceProcessor::open(path)
        .map(Box::new)
        .map(Backend::SentencePiece)
        .map_err(|error| {
            TokenizerError::new(format!(
                "could not load {} tokenizer from {}: {error}",
                family,
                path.display()
            ))
        })
}

fn load_tiktoken(path: &Path) -> Result<CoreBPE, TokenizerError> {
    let ranks_file = fs::read_to_string(path).map_err(|error| {
        TokenizerError::new(format!(
            "could not read tiktoken tokenizer from {}: {error}",
            path.display()
        ))
    })?;
    let mut ranks: FxHashMap<Vec<u8>, Rank> = FxHashMap::default();

    for (line_index, line) in ranks_file.lines().enumerate() {
        let mut fields = line.split_ascii_whitespace();
        let encoded_token = fields.next().ok_or_else(|| {
            TokenizerError::new(format!("invalid tiktoken data on line {}", line_index + 1))
        })?;
        let rank = fields
            .next()
            .ok_or_else(|| TokenizerError::new(format!("missing rank on line {}", line_index + 1)))?
            .parse::<Rank>()
            .map_err(|error| {
                TokenizerError::new(format!("invalid rank on line {}: {error}", line_index + 1))
            })?;
        if fields.next().is_some() {
            return Err(TokenizerError::new(format!(
                "too many fields in tiktoken data on line {}",
                line_index + 1
            )));
        }
        let token = base64::engine::general_purpose::STANDARD
            .decode(encoded_token)
            .map_err(|error| {
                TokenizerError::new(format!(
                    "invalid base64 token on line {}: {error}",
                    line_index + 1
                ))
            })?;
        if ranks.insert(token, rank).is_some() {
            return Err(TokenizerError::new(format!(
                "duplicate token in tiktoken data on line {}",
                line_index + 1
            )));
        }
    }

    // A rank file contains the vocabulary and merges, but not its splitting
    // expression. This is the cl100k expression used by the reference module.
    const CL100K_PATTERN: &str = "'(?i:[sdmt]|ll|ve|re)|[^\\r\\n\\p{L}\\p{N}]?+\\p{L}++|\\p{N}{1,3}+| ?[^\\s\\p{L}\\p{N}]++[\\r\\n]*+|\\s++$|\\s*[\\r\\n]|\\s+(?!\\S)|\\s";

    CoreBPE::new(ranks, FxHashMap::default(), CL100K_PATTERN).map_err(|error| {
        TokenizerError::new(format!(
            "could not construct tiktoken tokenizer from {}: {error}",
            path.display()
        ))
    })
}

static GLOBAL_REGISTRY: OnceLock<Result<TokenizerRegistry, TokenizerError>> = OnceLock::new();

/// Count the number of tokens represented by `text` for `model_id`.
///
/// The selected tokenizer and up to 1024 distinct `(family, text)` results are
/// cached for the lifetime of the process.
pub fn tokenize(model_id: &str, text: &str) -> Result<usize, TokenizerError> {
    match GLOBAL_REGISTRY.get_or_init(TokenizerRegistry::from_environment) {
        Ok(registry) => registry.tokenize(model_id, text),
        Err(error) => Err(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_map_to_specific_families_before_general_ones() {
        let cases = [
            ("anthropic/claude-sonnet-4", ModelFamily::Claude),
            ("deepseek/deepseek-v4", ModelFamily::DeepseekV4),
            ("google/gemma-3-27b", ModelFamily::Gemma),
            ("THUDM/chatglm3-6b", ModelFamily::Glm),
            ("ai21/jamba-1.5", ModelFamily::Jamba),
            ("Nerdstash-v2", ModelFamily::NerdstashV2),
            ("Nerdstash", ModelFamily::Nerdstash),
            ("meta-llama/Llama-3.3-70B", ModelFamily::Llama3),
            ("codellama/CodeLlama-34b", ModelFamily::Llama),
            ("mistralai/Mixtral-8x7B", ModelFamily::Mistral),
            ("01-ai/Yi-34B", ModelFamily::Yi),
            ("openai/gpt-4.1", ModelFamily::Tiktoken),
            ("nvidia/Nemotron-3-Nano", ModelFamily::Nemotron3),
            ("nvidia/Llama-3.1-Nemotron", ModelFamily::Llama3),
            ("unknown/new-model", ModelFamily::Llama3),
        ];

        for (model, expected) in cases {
            assert_eq!(model_family(model), expected, "model id: {model}");
        }
    }

    #[test]
    fn normalization_collapses_and_trims_separators() {
        assert_eq!(normalize_model_id(" /Meta__Llama--3/ "), "meta-llama-3");
    }

    #[test]
    fn every_bundled_family_can_tokenize() {
        let registry = TokenizerRegistry::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tokenizer_data"),
        )
        .unwrap();
        let families = [
            ModelFamily::Claude,
            ModelFamily::DeepseekV4,
            ModelFamily::Gemma,
            ModelFamily::Glm,
            ModelFamily::Jamba,
            ModelFamily::Llama3,
            ModelFamily::Llama,
            ModelFamily::Mistral,
            ModelFamily::Nerdstash,
            ModelFamily::NerdstashV2,
            ModelFamily::Tiktoken,
            ModelFamily::Yi,
            ModelFamily::Nemotron3,
        ];

        for family in families {
            let count = registry
                .tokenize_for_family(family, "Hello, world! 👋")
                .unwrap_or_else(|error| panic!("{family} failed: {error}"));
            assert!(count > 0, "{family} returned no tokens");
        }
    }
}
