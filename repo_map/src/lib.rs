//! Compact, Aider-style structural maps of source repositories.
//!
//! [`RepoMap`] walks a repository, extracts definitions and references with
//! tree-sitter, ranks definitions using the reference graph, and renders the
//! highest-value source lines that fit in a token budget.
//!
//! TODO:
//!     1. Add single-file symbol scan

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use tree_sitter::{Language, Node, Parser};
use walkdir::{DirEntry, WalkDir};

pub const DEFAULT_MAP_TOKENS: usize = 3_500;
pub const MAX_MAP_TOKENS: usize = 6_000;
pub const MAX_SOURCE_FILE_BYTES: u64 = 1_000_000;
const MAX_RENDERED_LINE_LENGTH: usize = 160;

const NO_FILES: &str = "No source files found. Use glob for raw path discovery.";
const NO_SUPPORTED_FILES: &str =
    "No tree-sitter-supported source files found. Use glob for raw path discovery.";

const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".mypy_cache",
    ".nox",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".venv",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "target",
    "venv",
];

const DEFINITION_NODE_TYPES: &[&str] = &[
    "class_definition",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "enum_item",
    "struct_item",
    "struct_specifier",
    "trait_item",
    "impl_item",
    "function_definition",
    "function_declaration",
    "function_item",
    "method_definition",
    "method_declaration",
    "constructor_declaration",
    "generator_function_declaration",
    "type_alias_declaration",
    "type_definition",
    "type_spec",
    "type_item",
    "module_declaration",
    "namespace_definition",
    "namespace_declaration",
    "mod_item",
    "macro_definition",
    "variable_declarator",
    "var_spec",
    "const_spec",
    "const_item",
    "static_item",
];

const REFERENCE_NODE_TYPES: &[&str] = &[
    "identifier",
    "type_identifier",
    "field_identifier",
    "property_identifier",
    "namespace_identifier",
    "module_name",
];

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Definition {
    pub path: String,
    /// Zero-based source line.
    pub start_line: usize,
    pub end_line: usize,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileIndex {
    pub definitions: Vec<Definition>,
    pub references: BTreeMap<String, usize>,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidPath(String),
    InvalidTokenBudget(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidPath(message) => f.write_str(message),
            Self::InvalidTokenBudget(0) => f.write_str("max_tokens must be greater than zero"),
            Self::InvalidTokenBudget(value) => {
                write!(f, "max_tokens cannot exceed {MAX_MAP_TOKENS} (got {value})")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
struct CacheEntry {
    modified: Option<SystemTime>,
    size: u64,
    index: FileIndex,
}

#[derive(Clone, Debug)]
struct Edge {
    source: String,
    destination: String,
    identifier: String,
    weight: f64,
}

/// A reusable repository index. File indexes are retained between renders and
/// invalidated from file size and modification time.
#[derive(Debug)]
pub struct RepoMap {
    workspace: PathBuf,
    tmp: Option<PathBuf>,
    cache: HashMap<PathBuf, CacheEntry>,
}

impl RepoMap {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            tmp: None,
            cache: HashMap::new(),
        }
    }

    /// Configure the root addressed by `@tmp` paths.
    pub fn with_tmp(mut self, tmp: impl Into<PathBuf>) -> Self {
        self.tmp = Some(tmp.into());
        self
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Render a map rooted at `path`. Focus terms may be symbol names, file
    /// names, path components, or repository-relative paths.
    ///
    /// The built-in token estimate is deliberately conservative and requires
    /// no model-specific tokenizer. Use [`Self::render_with_counter`] when an
    /// exact tokenizer is available to the caller.
    pub fn render<I, S>(&mut self, path: &str, focus: I, max_tokens: usize) -> Result<String, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.render_with_counter(path, focus, max_tokens, estimated_tokens)
    }

    pub fn render_default(&mut self) -> Result<String, Error> {
        self.render(".", std::iter::empty::<&str>(), DEFAULT_MAP_TOKENS)
    }

    pub fn render_with_counter<I, S, F>(
        &mut self,
        path: &str,
        focus: I,
        max_tokens: usize,
        count_tokens: F,
    ) -> Result<String, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        F: Fn(&str) -> usize,
    {
        validate_budget(max_tokens)?;
        let files = self.effective_files(path)?;
        if files.is_empty() {
            return Ok(NO_FILES.to_owned());
        }

        let mut indexes = BTreeMap::new();
        let mut physical_paths = BTreeMap::new();
        for (relative, physical) in files {
            if let Some(index) = self.index_file(&relative, &physical) {
                indexes.insert(relative.clone(), index);
                physical_paths.insert(relative, physical);
            }
        }
        if indexes.is_empty() {
            return Ok(NO_SUPPORTED_FILES.to_owned());
        }

        let focus = focus
            .into_iter()
            .map(|term| term.as_ref().trim().to_owned())
            .filter(|term| !term.is_empty())
            .collect::<BTreeSet<_>>();
        let ranked = rank_definitions(&indexes, &focus);
        if ranked.is_empty() {
            return Ok(fallback_file_map(&indexes, max_tokens, &count_tokens));
        }
        Ok(fit_ranked_map(
            &ranked,
            &physical_paths,
            max_tokens,
            &count_tokens,
        ))
    }

    /// Index one file using the same parser and cache as [`Self::render`].
    pub fn index(&mut self, path: impl AsRef<Path>) -> Result<Option<FileIndex>, Error> {
        let relative = normalize_relative_path(path.as_ref(), "path must be project-relative")?;
        let root = canonical_root(&self.workspace)?;
        let physical = root.join(&relative);
        if !safe_regular_file(&physical, &root) {
            return Ok(None);
        }
        Ok(self.index_file(&path_text(&relative), &physical))
    }

    fn effective_files(&self, raw_path: &str) -> Result<BTreeMap<String, PathBuf>, Error> {
        let value = raw_path.trim();
        let value = if value.is_empty() { "." } else { value };
        if value == "@tmp" || value.starts_with("@tmp/") {
            let root = self.tmp.as_ref().ok_or_else(|| {
                Error::InvalidPath("@tmp path used without configuring a temporary root".into())
            })?;
            let suffix = value.strip_prefix("@tmp").unwrap().trim_start_matches('/');
            let subtree = normalize_relative_path(
                Path::new(if suffix.is_empty() { "." } else { suffix }),
                "tree @tmp path must stay within @tmp",
            )?;
            return walk_files(root, &subtree, Some("@tmp"));
        }
        let subtree = normalize_relative_path(
            Path::new(value),
            "tree path must be a project-relative subtree",
        )?;
        walk_files(&self.workspace, &subtree, None)
    }

    fn index_file(&mut self, relative: &str, physical: &Path) -> Option<FileIndex> {
        let language = language_for_path(physical)?;
        let metadata = fs::metadata(physical).ok()?;
        if metadata.len() > MAX_SOURCE_FILE_BYTES {
            return None;
        }
        let modified = metadata.modified().ok();
        if let Some(cached) = self.cache.get(physical)
            && cached.size == metadata.len()
            && cached.modified == modified
        {
            return Some(cached.index.clone());
        }

        let source = fs::read(physical).ok()?;
        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        let tree = parser.parse(&source, None)?;
        let index = extract_tags(tree.root_node(), &source, relative);
        self.cache.insert(
            physical.to_path_buf(),
            CacheEntry {
                modified,
                size: metadata.len(),
                index: index.clone(),
            },
        );
        Some(index)
    }
}

fn validate_budget(max_tokens: usize) -> Result<(), Error> {
    if max_tokens == 0 || max_tokens > MAX_MAP_TOKENS {
        Err(Error::InvalidTokenBudget(max_tokens))
    } else {
        Ok(())
    }
}

fn estimated_tokens(text: &str) -> usize {
    // Code tends to tokenize more densely than prose. Counting Unicode scalar
    // values also avoids treating non-ASCII paths as four bytes per character.
    text.chars().count().div_ceil(3)
}

fn normalize_relative_path(path: &Path, message: &str) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        return Err(Error::InvalidPath(message.into()));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::InvalidPath(message.into()));
            }
        }
    }
    Ok(normalized)
}

fn canonical_root(root: &Path) -> Result<PathBuf, Error> {
    root.canonicalize().map_err(Error::Io)
}

fn walk_files(
    root: &Path,
    subtree: &Path,
    display_prefix: Option<&str>,
) -> Result<BTreeMap<String, PathBuf>, Error> {
    let root = canonical_root(root)?;
    let target = root.join(subtree);
    let target = match target.canonicalize() {
        Ok(path) if path.starts_with(&root) => path,
        _ => return Ok(BTreeMap::new()),
    };

    let mut files = BTreeMap::new();
    if safe_regular_file(&target, &root) {
        insert_file(&mut files, &root, &target, display_prefix);
        return Ok(files);
    }
    if !target.is_dir() {
        return Ok(files);
    }

    let walker = WalkDir::new(&target)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !should_skip_entry(entry));
    for entry in walker.filter_map(Result::ok) {
        if entry.file_type().is_file() && safe_regular_file(entry.path(), &root) {
            insert_file(&mut files, &root, entry.path(), display_prefix);
        }
    }
    Ok(files)
}

fn should_skip_entry(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && SKIP_DIRECTORIES
            .iter()
            .any(|name| entry.file_name() == *name)
}

fn safe_regular_file(path: &Path, root: &Path) -> bool {
    path.canonicalize()
        .is_ok_and(|resolved| resolved.starts_with(root) && resolved.is_file())
}

fn insert_file(
    files: &mut BTreeMap<String, PathBuf>,
    root: &Path,
    physical: &Path,
    prefix: Option<&str>,
) {
    let Ok(relative) = physical.strip_prefix(root) else {
        return;
    };
    if relative.components().any(|part| {
        SKIP_DIRECTORIES
            .iter()
            .any(|name| part.as_os_str() == *name)
    }) {
        return;
    }
    let relative = path_text(relative);
    let display = prefix.map_or_else(|| relative.clone(), |prefix| format!("{prefix}/{relative}"));
    files.insert(display, physical.to_path_buf());
}

fn path_text(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn language_for_path(path: &Path) -> Option<Language> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "py" | "pyw" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        _ => return None,
    })
}

fn extract_tags(root: Node<'_>, source: &[u8], relative: &str) -> FileIndex {
    let mut definitions = Vec::new();
    let mut references = BTreeMap::new();
    let mut definition_ranges = HashSet::new();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        let children = node.named_children(&mut cursor).collect::<Vec<_>>();
        stack.extend(children.iter().rev().copied());

        if is_definition_node(node.kind())
            && let Some(name_node) = definition_name_node(node)
            && let Some(name) = node_identifier(name_node, source)
        {
            definitions.push(Definition {
                path: relative.to_owned(),
                start_line: node.start_position().row,
                end_line: node.end_position().row,
                name,
            });
            definition_ranges.insert((name_node.start_byte(), name_node.end_byte()));
        }

        if children.is_empty()
            && REFERENCE_NODE_TYPES.contains(&node.kind())
            && !definition_ranges.contains(&(node.start_byte(), node.end_byte()))
            && let Some(name) = node_identifier(node, source)
        {
            *references.entry(name).or_default() += 1;
        }
    }

    FileIndex {
        definitions,
        references,
    }
}

fn is_definition_node(kind: &str) -> bool {
    if DEFINITION_NODE_TYPES.contains(&kind) {
        return true;
    }
    (kind.ends_with("_definition") || kind.ends_with("_declaration"))
        && [
            "class",
            "enum",
            "function",
            "interface",
            "method",
            "module",
            "namespace",
            "struct",
            "trait",
            "type",
        ]
        .iter()
        .any(|marker| kind.contains(marker))
}

fn definition_name_node(node: Node<'_>) -> Option<Node<'_>> {
    for field in ["name", "declarator", "pattern", "left"] {
        if let Some(candidate) = node.child_by_field_name(field)
            && let Some(identifier) = first_identifier_node(candidate)
        {
            return Some(identifier);
        }
    }
    first_identifier_node(node)
}

fn first_identifier_node(node: Node<'_>) -> Option<Node<'_>> {
    if REFERENCE_NODE_TYPES.contains(&node.kind()) {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(identifier) = first_identifier_node(child) {
            return Some(identifier);
        }
    }
    None
}

fn node_identifier(node: Node<'_>, source: &[u8]) -> Option<String> {
    let value = std::str::from_utf8(source.get(node.byte_range())?)
        .ok()?
        .trim();
    if value.len() < 2 || value.len() > 128 {
        return None;
    }
    let mut chars = value.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '-'))
    {
        return None;
    }
    Some(value.to_owned())
}

fn rank_definitions(
    indexes: &BTreeMap<String, FileIndex>,
    focus: &BTreeSet<String>,
) -> Vec<Definition> {
    let mut defines: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut definitions: BTreeMap<(String, String), Vec<Definition>> = BTreeMap::new();
    let mut references: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (path, index) in indexes {
        for definition in &index.definitions {
            defines
                .entry(definition.name.clone())
                .or_default()
                .insert(path.clone());
            definitions
                .entry((path.clone(), definition.name.clone()))
                .or_default()
                .push(definition.clone());
        }
        for (identifier, count) in &index.references {
            *references
                .entry(identifier.clone())
                .or_default()
                .entry(path.clone())
                .or_default() += count;
        }
    }
    if defines.is_empty() {
        return Vec::new();
    }

    let mut edges = Vec::new();
    for (identifier, definers) in &defines {
        let Some(referencers) = references.get(identifier) else {
            continue;
        };
        let mut multiplier = 1.0;
        let is_snake = identifier.contains('_') && identifier.chars().any(char::is_alphabetic);
        let is_kebab = identifier.contains('-') && identifier.chars().any(char::is_alphabetic);
        let is_camel = identifier.chars().any(char::is_uppercase)
            && identifier.chars().any(char::is_lowercase);
        if focus.contains(identifier) {
            multiplier *= 10.0;
        }
        if (is_snake || is_kebab || is_camel) && identifier.len() >= 8 {
            multiplier *= 10.0;
        }
        if identifier.starts_with('_') {
            multiplier *= 0.1;
        }
        if definers.len() > 5 {
            multiplier *= 0.1;
        }
        for (referencer, count) in referencers {
            for definer in definers {
                let mut weight = multiplier * (*count as f64).sqrt();
                if path_matches_focus(referencer, focus) {
                    weight *= 50.0;
                }
                edges.push(Edge {
                    source: referencer.clone(),
                    destination: definer.clone(),
                    identifier: identifier.clone(),
                    weight,
                });
            }
        }
    }
    for (identifier, definers) in &defines {
        if references.contains_key(identifier) {
            continue;
        }
        for definer in definers {
            edges.push(Edge {
                source: definer.clone(),
                destination: definer.clone(),
                identifier: identifier.clone(),
                weight: 0.1,
            });
        }
    }

    let nodes = indexes.keys().cloned().collect::<BTreeSet<_>>();
    let personalization = nodes
        .iter()
        .filter(|path| path_matches_focus(path, focus))
        .cloned()
        .collect::<BTreeSet<_>>();
    let ranks = pagerank(&nodes, &edges, &personalization);

    let mut outgoing: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
    for edge in &edges {
        outgoing.entry(&edge.source).or_default().push(edge);
    }
    let mut scores: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (source, source_edges) in outgoing {
        let total = source_edges.iter().map(|edge| edge.weight).sum::<f64>();
        if total <= 0.0 {
            continue;
        }
        let source_rank = ranks.get(source).copied().unwrap_or(0.0);
        for edge in source_edges {
            *scores
                .entry((edge.destination.clone(), edge.identifier.clone()))
                .or_default() += source_rank * edge.weight / total;
        }
    }
    let mut scored = scores.into_iter().collect::<Vec<_>>();
    scored.sort_by(|(left_key, left_score), (right_key, right_score)| {
        right_score
            .partial_cmp(left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left_key.cmp(right_key))
    });

    let mut result = Vec::new();
    for (key, _) in scored {
        if let Some(items) = definitions.get(&key) {
            result.extend(items.iter().cloned());
        }
    }
    let included = result
        .iter()
        .map(|item| (item.path.clone(), item.start_line, item.name.clone()))
        .collect::<HashSet<_>>();
    let mut remaining = indexes
        .values()
        .flat_map(|index| index.definitions.iter().cloned())
        .filter(|item| !included.contains(&(item.path.clone(), item.start_line, item.name.clone())))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|item| {
        (
            !path_matches_focus(&item.path, focus),
            !focus.contains(&item.name),
            item.path.clone(),
            item.start_line,
        )
    });
    result.extend(remaining);
    result
}

fn path_matches_focus(path: &str, focus: &BTreeSet<String>) -> bool {
    if focus.is_empty() {
        return false;
    }
    let path = Path::new(path);
    let components = path
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect::<HashSet<_>>();
    focus.iter().any(|term| {
        components.contains(term.as_str())
            || path.file_name().is_some_and(|name| name == term.as_str())
            || path.file_stem().is_some_and(|name| name == term.as_str())
            || (term.contains('/') && path_text(path).contains(term))
    })
}

fn pagerank(
    nodes: &BTreeSet<String>,
    edges: &[Edge],
    personalization: &BTreeSet<String>,
) -> BTreeMap<String, f64> {
    if nodes.is_empty() {
        return BTreeMap::new();
    }
    let uniform = 1.0 / nodes.len() as f64;
    let mut rank = nodes
        .iter()
        .map(|node| (node.clone(), uniform))
        .collect::<BTreeMap<_, _>>();
    let teleport = nodes
        .iter()
        .map(|node| {
            let value = if personalization.is_empty() {
                uniform
            } else if personalization.contains(node) {
                1.0 / personalization.len() as f64
            } else {
                0.0
            };
            (node.clone(), value)
        })
        .collect::<BTreeMap<_, _>>();
    let mut outgoing: BTreeMap<&str, Vec<(&str, f64)>> = BTreeMap::new();
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for edge in edges.iter().filter(|edge| edge.weight > 0.0) {
        outgoing
            .entry(&edge.source)
            .or_default()
            .push((&edge.destination, edge.weight));
        *totals.entry(&edge.source).or_default() += edge.weight;
    }

    const DAMPING: f64 = 0.85;
    for _ in 0..100 {
        let mut next = nodes
            .iter()
            .map(|node| (node.clone(), (1.0 - DAMPING) * teleport[node]))
            .collect::<BTreeMap<_, _>>();
        let dangling = nodes
            .iter()
            .filter(|node| totals.get(node.as_str()).copied().unwrap_or(0.0) <= 0.0)
            .map(|node| rank[node])
            .sum::<f64>();
        if dangling > 0.0 {
            for node in nodes {
                next.entry(node.clone())
                    .and_modify(|value| *value += DAMPING * dangling * teleport[node]);
            }
        }
        for (source, destinations) in &outgoing {
            let contribution = DAMPING * rank[*source] / totals[source];
            for (destination, weight) in destinations {
                *next.entry((*destination).to_owned()).or_default() += contribution * weight;
            }
        }
        let delta = nodes
            .iter()
            .map(|node| (next[node] - rank[node]).abs())
            .sum::<f64>();
        rank = next;
        if delta <= 1e-8 {
            break;
        }
    }
    rank
}

fn fit_ranked_map<F>(
    ranked: &[Definition],
    physical_paths: &BTreeMap<String, PathBuf>,
    max_tokens: usize,
    count_tokens: &F,
) -> String
where
    F: Fn(&str) -> usize,
{
    let mut low = 1;
    let mut high = ranked.len();
    let mut best = String::new();
    while low <= high {
        let middle = low + (high - low) / 2;
        let rendered = render_definitions(&ranked[..middle], physical_paths);
        if count_tokens(&rendered) <= max_tokens {
            best = rendered;
            low = middle + 1;
        } else {
            high = middle - 1;
        }
    }
    if best.is_empty() {
        let first = &ranked[0];
        format!(
            "{}:{}-{}: {}",
            first.path,
            first.start_line + 1,
            first.end_line + 1,
            first.name
        )
    } else {
        best.trim_end().to_owned()
    }
}

fn render_definitions(
    definitions: &[Definition],
    physical_paths: &BTreeMap<String, PathBuf>,
) -> String {
    let mut parts = Vec::new();
    let mut definitions = definitions.to_vec();

    definitions.sort_by_key(|d| {
        (d.path.clone(), d.start_line)
    });
    for definition in definitions {
        let Some(physical) = physical_paths.get(&definition.path) else {
            continue;
        };

        let Ok(code) = fs::read_to_string(physical) else {
            continue;
        };

        let source_lines = code.lines().collect::<Vec<_>>();

        let rendered = (definition.start_line..=definition.end_line)
            .filter_map(|line| source_lines.get(line))
            .map(|line| truncate_line(line, MAX_RENDERED_LINE_LENGTH))
            .collect::<Vec<_>>()
            .join("\n");

        parts.push(format!(
            "{}:{}-{} {}:\n{}",
            definition.path,
            definition.start_line + 1,
            definition.end_line + 1,
            definition.name,
            rendered
        ));
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("\n\n") + "\n"
    }
}

fn truncate_line(line: &str, maximum: usize) -> String {
    line.chars().take(maximum).collect()
}

fn fallback_file_map<F>(
    indexes: &BTreeMap<String, FileIndex>,
    max_tokens: usize,
    count_tokens: &F,
) -> String
where
    F: Fn(&str) -> usize,
{
    let mut result = Vec::new();
    for path in indexes.keys() {
        let candidate = result
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(path.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        if count_tokens(&candidate) > max_tokens {
            break;
        }
        result.push(path.clone());
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("repo-map-test-{}-{sequence}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn renders_and_ranks_cross_file_definitions() {
        let root = TestDir::new();
        root.write(
            "src/service.rs",
            "pub struct ImportantService;\nimpl ImportantService {\n    pub fn run_task(&self) {}\n}\n",
        );
        root.write(
            "src/main.rs",
            "fn main() {\n    let service = ImportantService;\n    service.run_task();\n}\n",
        );
        root.write("target/ignored.rs", "fn should_not_appear() {}\n");

        let mut map = RepoMap::new(&root.0);
        let rendered = map.render(".", ["ImportantService"], 3_500).unwrap();
        assert!(rendered.contains("src/service.rs:"));
        assert!(rendered.contains("pub struct ImportantService;"));
        assert!(!rendered.contains("should_not_appear"));
    }

    #[test]
    fn supports_python_typescript_javascript_and_go() {
        let root = TestDir::new();
        root.write(
            "lib.py",
            "class PythonThing:\n    def method_name(self):\n        pass\n",
        );
        root.write("web.ts", "export function typescriptThing(): void {}\n");
        root.write("browser.js", "function javascriptThing() {}\n");
        root.write("server.go", "package server\nfunc GoFunction() {}\n");
        let mut map = RepoMap::new(&root.0);
        let rendered = map.render_default().unwrap();
        for expected in [
            "PythonThing",
            "typescriptThing",
            "javascriptThing",
            "GoFunction",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
    }

    #[test]
    fn rejects_escaping_and_invalid_budgets() {
        let root = TestDir::new();
        root.write("main.rs", "fn main() {}\n");
        let mut map = RepoMap::new(&root.0);
        assert!(matches!(
            map.render("../outside", ["main"], 100),
            Err(Error::InvalidPath(_))
        ));
        assert!(matches!(
            map.render(".", ["main"], 0),
            Err(Error::InvalidTokenBudget(0))
        ));
    }

    #[test]
    fn tiny_budget_returns_a_location_without_partial_source() {
        let root = TestDir::new();
        root.write("main.rs", "fn unusually_long_function_name() {}\n");
        let mut map = RepoMap::new(&root.0);
        let rendered = map
            .render_with_counter(".", ["unusually_long_function_name"], 1, str::len)
            .unwrap();
        assert_eq!(
            rendered,
            "main.rs:1-1: unusually_long_function_name"
        );
    }

    #[test]
    fn files_without_definitions_fall_back_to_paths() {
        let root = TestDir::new();
        root.write("constants.js", "42;\n");
        let mut map = RepoMap::new(&root.0);
        assert_eq!(map.render_default().unwrap(), "constants.js");
    }
}
