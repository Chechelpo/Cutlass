use std::{
    collections::HashSet,
    fs,
    io,
    path::{Path, PathBuf},
};

use globset::{Glob, GlobSet, GlobSetBuilder};


const DEFAULT_MAX_DEPTH: usize = 3;
const DEFAULT_LIMIT: usize = 200;

const MAX_MAX_DEPTH: usize = 20;
const MAX_LIMIT: usize = 5000;


const DEFAULT_SKIPS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "venv",
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".nox",
    ".idea",
    ".vscode",
    "dist",
    "build",
    "coverage",
    ".coverage",
];


#[derive(Debug)]
struct TreeEntry {
    path: PathBuf,
    relative: PathBuf,
    is_directory: bool,
    is_symlink: bool,
}


#[derive(Debug)]
struct TreeState {
    limit: usize,
    emitted: usize,
    files: usize,
    directories: usize,
    skipped: usize,
    truncated: bool,
}


impl TreeState {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            emitted: 0,
            files: 0,
            directories: 0,
            skipped: 0,
            truncated: false,
        }
    }
}


pub fn render_tree(
    root: impl AsRef<Path>,
    max_depth: usize,
    directories_only: bool,
    skip: &[String],
    hidden: bool,
    limit: usize,
    use_default_skips: bool,
) -> io::Result<String> {
    validate_options(max_depth, limit)?;

    let root = root.as_ref();

    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Tree root does not exist: {}", root.display()),
        ));
    }

    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Tree root is not a directory: {}", root.display()),
        ));
    }


    let mut patterns = normalize_skip_patterns(skip)?;

    if use_default_skips {
        patterns.extend(
            DEFAULT_SKIPS
                .iter()
                .map(|x| x.to_string()),
        );
    }

    let skip_matcher = build_skip_matcher(&patterns);


    let mut lines = vec![
        root_label(root),
    ];

    let mut state = TreeState::new(limit);


    walk(
        root,
        Path::new(""),
        "",
        0,
        max_depth,
        directories_only,
        hidden,
        &skip_matcher,
        &mut lines,
        &mut state,
    )?;


    if state.truncated {
        lines.push(String::new());
        lines.push(format!(
            "[truncated after {} entries]",
            state.emitted
        ));
    }


    lines.push(String::new());


    let mut summary = format!(
        "{} {}",
        state.directories,
        if state.directories == 1 {
            "directory"
        } else {
            "directories"
        }
    );


    if !directories_only {
        summary.push_str(&format!(
            ", {} {}",
            state.files,
            if state.files == 1 {
                "file"
            } else {
                "files"
            }
        ));
    }


    if state.skipped > 0 {
        summary.push_str(&format!(
            ", {} skipped",
            state.skipped
        ));
    }


    lines.push(summary);

    Ok(lines.join("\n"))
}


fn walk(
    directory: &Path,
    relative_directory: &Path,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    directories_only: bool,
    hidden: bool,
    skip: &GlobSet,
    lines: &mut Vec<String>,
    state: &mut TreeState,
) -> io::Result<()> {

    if state.truncated || depth >= max_depth {
        return Ok(());
    }


    let mut entries = Vec::new();


    let iterator = match fs::read_dir(directory) {
        Ok(v) => v,
        Err(error) => {
            lines.push(format!(
                "{}[error reading directory: {}]",
                prefix,
                error
            ));

            return Ok(());
        }
    };


    for item in iterator {
        let item = item?;

        let path = item.path();

        let name = item.file_name()
            .to_string_lossy()
            .to_string();


        if !hidden && name.starts_with('.') {
            state.skipped += 1;
            continue;
        }


        let relative = relative_directory.join(&name);


        if skip.is_match(relative.to_string_lossy().as_ref())
            || skip.is_match(&name)
        {
            state.skipped += 1;
            continue;
        }


        let metadata = fs::symlink_metadata(&path)?;

        let is_symlink = metadata.file_type().is_symlink();

        let is_directory =
            if is_symlink {
                path.is_dir()
            } else {
                metadata.is_dir()
            };


        if directories_only && !is_directory {
            continue;
        }


        entries.push(TreeEntry {
            path,
            relative,
            is_directory,
            is_symlink,
        });
    }


    entries.sort_by(|a, b| {
        (
            !a.is_directory,
            a.path.file_name()
                .unwrap()
                .to_string_lossy()
                .to_lowercase(),
        )
            .cmp(&(
                !b.is_directory,
                b.path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_lowercase(),
            ))
    });



    for (index, entry) in entries.iter().enumerate() {

        if state.emitted >= state.limit {
            state.truncated = true;
            return Ok(());
        }


        let is_last = index == entries.len() - 1;

        let connector =
            if is_last {
                "└── "
            } else {
                "├── "
            };


        lines.push(format!(
            "{}{}{}{}",
            prefix,
            connector,
            entry.path.file_name()
                .unwrap()
                .to_string_lossy(),
            entry_suffix(entry),
        ));


        state.emitted += 1;


        if entry.is_directory {
            state.directories += 1;
        } else {
            state.files += 1;
        }


        if entry.is_directory && !entry.is_symlink {

            let child_prefix = format!(
                "{}{}",
                prefix,
                if is_last {
                    "    "
                } else {
                    "│   "
                }
            );


            walk(
                &entry.path,
                &entry.relative,
                &child_prefix,
                depth + 1,
                max_depth,
                directories_only,
                hidden,
                skip,
                lines,
                state,
            )?;


            if state.truncated {
                return Ok(());
            }
        }
    }


    Ok(())
}


fn entry_suffix(entry: &TreeEntry) -> String {

    let mut suffix = String::new();

    if entry.is_directory {
        suffix.push('/');
    }


    if entry.is_symlink {

        let target = fs::read_link(&entry.path)
            .map(|x| x.display().to_string())
            .unwrap_or_else(|_| "?".into());


        suffix.push_str(" -> ");
        suffix.push_str(&target);
    }


    suffix
}



fn root_label(path: &Path) -> String {

    let value = path.display().to_string();

    if value == "." {
        ".".into()
    } else if value.ends_with('/') {
        value
    } else {
        format!("{}/", value)
    }
}



fn normalize_skip_patterns(
    values: &[String],
) -> io::Result<HashSet<String>> {

    let mut result = HashSet::new();


    for value in values {

        let mut value = value.trim()
            .replace('\\', "/");


        while value.starts_with("./") {
            value = value[2..].to_string();
        }


        value = value.trim_end_matches('/')
            .to_string();


        if value.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "skip cannot contain empty patterns",
            ));
        }


        result.insert(value);
    }


    Ok(result)
}



fn build_skip_matcher(
    patterns: &HashSet<String>,
) -> GlobSet {

    let mut builder = GlobSetBuilder::new();


    for pattern in patterns {

        let glob = if pattern.contains('/') {
            pattern.clone()
        } else {
            format!("**/{}", pattern)
        };


        builder.add(
            Glob::new(&glob)
                .expect("invalid glob pattern")
        );
    }


    builder.build()
        .expect("invalid glob set")
}



fn validate_options(
    max_depth: usize,
    limit: usize,
) -> io::Result<()> {

    if max_depth > MAX_MAX_DEPTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "max_depth cannot exceed {}",
                MAX_MAX_DEPTH
            ),
        ));
    }


    if limit == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "limit must be greater than zero",
        ));
    }


    if limit > MAX_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "limit cannot exceed {}",
                MAX_LIMIT
            ),
        ));
    }


    Ok(())
}