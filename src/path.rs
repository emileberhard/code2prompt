//! This module contains the functions for traversing the directory and processing the files.

use anyhow::Result;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use log::debug;
use serde_json::json;
use std::{fs, path::Path};
use termtree::Tree;
use regex::Regex;
use lazy_static::lazy_static;
use glob::Pattern;

lazy_static! {
    static ref BASE64_REGEX: Regex = Regex::new(r#"(?P<b64>[A-Za-z0-9+/=]{80,})"#).unwrap();
}

/// Shortens all base64 strings longer than 80 chars
///
/// # Arguments
///
/// * `code` - The file content to search for base64 substrings
///
/// # Returns
///
/// * `String` - The content with shortened base64 strings
pub fn shorten_long_base64_strings(code: &str) -> String {
    BASE64_REGEX.replace_all(code, |caps: &regex::Captures| {
        let b64 = &caps["b64"];
        if b64.len() > 100 {
            let start = &b64[..50];
            let end = &b64[b64.len()-50..];
            format!("{}...{}", start, end)
        } else {
            b64.to_string()
        }
    }).to_string()
}

/// Traverses the directory and returns the string representation of the tree and the vector of JSON file representations.
///
/// # Arguments
///
/// * `root_path` - The path to the root directory.
/// * `include_patterns` - The patterns of files to include.
/// * `exclude_patterns` - The patterns of files to exclude.
/// * `_include_priority` - Whether to give priority to include patterns (deprecated).
/// * `line_number` - Whether to add line numbers to the code.
/// * `relative_paths` - Whether to use relative paths.
/// * `exclude_from_tree` - Whether to exclude files from the tree.
/// * `no_codeblock` - Whether to not wrap the code block with a delimiter.
/// * `_c2pignore_patterns` - Deprecated parameter, no longer used as ignore patterns are handled by the ignore crate.
///
/// # Returns
///
/// A tuple containing the string representation of the directory tree and a vector of JSON representations of the files.
#[allow(clippy::too_many_arguments)]
pub fn traverse_directory(
    root_path: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
    _include_priority: bool,
    line_number: bool,
    relative_paths: bool,
    exclude_from_tree: bool,
    no_codeblock: bool,
    _c2pignore_patterns: &[String], // Deprecated parameter
) -> Result<(String, Vec<serde_json::Value>)> {
    let canonical_root_path = root_path.canonicalize()?;
    let parent_directory = label(&canonical_root_path);

    // Handle single file case
    if canonical_root_path.is_file() {
        let mut files = Vec::new();
        if let Ok(code_bytes) = fs::read(&canonical_root_path) {
            let mut code = String::from_utf8_lossy(&code_bytes).to_string();
            code = code.replace(char::REPLACEMENT_CHARACTER, "[]");
            // Always shorten base64 strings (regardless of extension)
            code = shorten_long_base64_strings(&code);

            let extension = canonical_root_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");
            let code_block = wrap_code_block(&code, extension, line_number, no_codeblock);

            if !code.trim().is_empty() {
                files.push(json!({
                    "path": canonical_root_path.display().to_string(),
                    "extension": extension,
                    "code": code_block,
                }));
            }
        }
        return Ok((canonical_root_path.display().to_string(), files));
    }

    // Directory case: Build WalkBuilder with .c2pignore support
    let mut builder = WalkBuilder::new(&canonical_root_path);
    builder
        .hidden(false)
        .git_ignore(false)
        .ignore(true)
        .add_custom_ignore_filename(".c2pignore");

    // Create override builder for default excludes + user excludes
    let mut override_builder = OverrideBuilder::new(&canonical_root_path);

    // 1) Add default excludes that will always apply
    let default_excludes = vec![
        // General "junk":
        "!**/.git/**",
        "!**/.svn/**",
        "!**/.hg/**",
        "!**/.DS_Store",
        "!**/.idea/**",
        "!**/*.swp",       // vim swap files
        "!**/.history/**",
        "!**/.cache/**",
        "!**/tmp/**",
        "!**/temp/**",

        // Python-related:
        "!**/__pycache__/**",
        "!**/.pytest_cache/**",
        "!**/.mypy_cache/**",
        "!**/.venv/**",
        "!**/venv/**",
        "!**/.virtualenv/**",

        // NodeJS / JS / TS:
        "!**/node_modules/**",
        "!**/npm-debug.log",
        "!**/yarn.lock",
        "!**/pnpm-lock.yaml",
        "!**/package-lock.json",
        "!**/dist/**",
        "!**/build/**",
        "!**/out/**",

        // Rust:
        "!**/target/**",
        "!**/Cargo.lock",
        "!**/.cargo/**",

        // Java / Maven / Gradle:
        "!**/target/**",
        "!**/.gradle/**",
        "!**/build/**",
        "!**/*.class",
        "!**/*.jar",
        "!**/*.war",

        // Dotnet / C#:
        "!**/bin/**",
        "!**/obj/**",

        // Docker & ephemeral:
        "!**/.docker/**",
        "!**/docker-compose.override.yml",
        "!**/docker-compose.override.yaml",

        // Lockfiles:
        "!**/*.lock",
        "!**/Gemfile.lock",
        "!**/Pipfile.lock",

        // Misc:
        "!**/*.log",
        "!**/coverage/**",
        "!**/.nyc_output/**",
        "!**/.serverless/**",
        "!**/.aws-sam/**",
        "!**/.terraform/**",
        "!**/.next/**",
        "!**/.nuxt/**",
        "!**/.angular/**",

        // Binary/Object files:
        "!**/*.pyc",
        "!**/*.pyo",
        "!**/*.pyd",
        "!**/*.so",
        "!**/*.dylib",
        "!**/*.dll",
        "!**/*.exe",
        "!**/*.o",

        // Additional:
        "!**/*.obj",
        "!**/Thumbs.db",
        "!**/*.sqlite",
        "!**/*.db",
        
        // Media files - Images:
        "!**/*.png",
        "!**/*.jpg",
        "!**/*.jpeg",
        "!**/*.gif",
        "!**/*.ico",
        "!**/*.bmp",
        "!**/*.tiff",
        "!**/*.tif",
        "!**/*.webp",
        "!**/*.svg",
        "!**/*.psd",
        "!**/*.ai",
        "!**/*.xcf",
        
        // Media files - Video:
        "!**/*.mp4",
        "!**/*.mov",
        "!**/*.avi",
        "!**/*.mkv",
        "!**/*.wmv",
        "!**/*.flv",
        "!**/*.webm",
        "!**/*.m4v",
        "!**/*.3gp",
        
        // Media files - Audio:
        "!**/*.mp3",
        "!**/*.wav",
        "!**/*.ogg",
        "!**/*.m4a",
        "!**/*.flac",
        "!**/*.aac",
        "!**/*.wma",
        "!**/*.mid",
        "!**/*.midi",
        
        // Media files - Documents and Archives:
        "!**/*.pdf",
        "!**/*.zip",
        "!**/*.rar",
        "!**/*.7z",
        "!**/*.tar",
        "!**/*.gz",
        "!**/*.bz2",
        "!**/*.xz",
        "!**/*.doc",
        "!**/*.docx",
        "!**/*.ppt",
        "!**/*.pptx",
        "!**/*.xls",
        "!**/*.xlsx",
        
        // IDE and Editor:
        "!**/*.swo",
    ];

    for pattern in default_excludes {
        override_builder.add(pattern)?;
    }

    // 2) Handle user excludes - ensure they're prefixed with !
    for exc in exclude_patterns {
        if exc.contains('*') {
            let exclude_pattern = if exc.starts_with('!') {
                exc.to_string()
            } else {
                format!("!{}", exc)
            };
            override_builder.add(&exclude_pattern)?;
        } else {
            override_builder.add(&format!("!**/*.{}", exc))?;
        }
    }

    let overrides = override_builder.build()?;
    builder.overrides(overrides);

    let walker = builder.build();

    // If --include patterns are provided, compile them once for use inside the loop.
    let compiled_includes: Option<Vec<Pattern>> = if !include_patterns.is_empty() {
        Some(
            include_patterns
                .iter()
                .map(|pat| {
                    if pat.eq_ignore_ascii_case("dockerfile") || pat.eq_ignore_ascii_case("docker") {
                        Pattern::new("**/Dockerfile")
                            .unwrap_or_else(|_| Pattern::new("*").unwrap())
                    } else if pat.eq_ignore_ascii_case("env") {
                        Pattern::new("**/.env*")
                            .unwrap_or_else(|_| Pattern::new("*").unwrap())
                    } else if pat.contains('*') || pat.contains('/') {
                        Pattern::new(pat).unwrap_or_else(|_| Pattern::new("*").unwrap())
                    } else {
                        Pattern::new(&format!("**/*.{}", pat))
                            .unwrap_or_else(|_| Pattern::new("*").unwrap())
                    }
                })
                .collect()
        )
    } else {
        None
    };

    let mut root = Tree::new(parent_directory.clone());
    let mut collected_files = Vec::new();

    // 3) Traverse files
    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                debug!("Skipping entry due to error: {:?}", err);
                continue;
            }
        };

        let path = entry.path();
        let relative = match path.strip_prefix(&canonical_root_path) {
            Ok(r) => r,
            Err(_) => path,
        };

        // Check if path matches an --include pattern
        let file_matches_include = if let Some(ref patterns) = compiled_includes {
            let rel_str = relative.to_str().unwrap_or("");
            patterns.iter().any(|p| p.matches(rel_str))
        } else {
            true // If no --include given, everything is included
        };

        // Determine the "depth" by component count
        let depth = relative.components().count();

        // 1) Add item (file or directory) to the tree if:
        //    - It's included, OR
        //    - The depth is <= 3
        if !exclude_from_tree {
            if file_matches_include || depth <= 3 {
                add_path_to_tree(&mut root, relative);
            }
        }

        // 2) If it's a directory, don't read its contents into "collected_files"
        //    We only do that for actual files below:
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }

        // 3) If it's a file that is actually included, read its content
        if file_matches_include {
            if let Ok(code_bytes) = fs::read(path) {
                let mut code = String::from_utf8_lossy(&code_bytes).to_string();
                code = code.replace(char::REPLACEMENT_CHARACTER, "[]");
                // Always shorten base64 strings (regardless of extension)
                code = shorten_long_base64_strings(&code);

                let extension = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("");
                let code_block = wrap_code_block(&code, extension, line_number, no_codeblock);

                if !code.trim().is_empty() {
                    let file_path = if relative_paths {
                        format!("{}/{}", parent_directory, relative.display())
                    } else {
                        path.display().to_string()
                    };

                    collected_files.push(json!({
                        "path": file_path,
                        "extension": extension,
                        "code": code_block,
                    }));
                }
            }
        }
    }

    let tree_str = if exclude_from_tree {
        String::new()
    } else {
        root.to_string()
    };

    Ok((tree_str, collected_files))
}

/// Helper to nest a relative path in the tree structure
fn add_path_to_tree(root: &mut Tree<String>, rel_path: &Path) {
    use std::path::Component;
    let mut current = root;
    for c in rel_path.components() {
        if let Component::Normal(os) = c {
            let name = os.to_string_lossy().to_string();
            if let Some(pos) = current.leaves.iter().position(|child| child.root == name) {
                current = &mut current.leaves[pos];
            } else {
                let new_leaf = Tree::new(name.clone());
                current.leaves.push(new_leaf);
                let last = current.leaves.len() - 1;
                current = &mut current.leaves[last];
            }
        }
    }
}

/// Returns the file name or the string representation of the path.
///
/// # Arguments
///
/// * `p` - The path to label.
///
/// # Returns
///
/// * `String` - The file name or string representation of the path.
pub fn label<P: AsRef<Path>>(p: P) -> String {
    let path = p.as_ref();
    if path.file_name().is_none() {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".")
            .to_owned()
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned()
    }
}

/// Wraps the code block with a delimiter and adds line numbers if required.
///
/// # Arguments
///
/// * `code` - The code block to wrap.
/// * `extension` - The file extension of the code block.
/// * `line_numbers` - Whether to add line numbers to the code.
/// * `no_codeblock` - Whether to not wrap the code block with a delimiter.
///
/// # Returns
///
/// * `String` - The wrapped code block.
pub fn wrap_code_block(code: &str, extension: &str, line_numbers: bool, no_codeblock: bool) -> String {
    let delimiter = "`".repeat(3);
    let mut code_with_line_numbers = String::new();

    if line_numbers {
        for (line_number, line) in code.lines().enumerate() {
            code_with_line_numbers.push_str(&format!("{:4} | {}\n", line_number + 1, line));
        }
    } else {
        code_with_line_numbers = code.to_string();
    }

    if no_codeblock {
        code_with_line_numbers
    } else {
        format!(
            "{}{}\n{}\n{}",
            delimiter, extension, code_with_line_numbers, delimiter
        )
    }
}