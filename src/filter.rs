//! This module is deprecated and will be removed in a future version.
//! All filtering is now handled by the ignore crate.

use colored::*;
use glob::Pattern;
use log::{debug, error};
use std::fs;
use std::path::Path;

#[deprecated(
    since = "2.0.2",
    note = "Use ignore crate's built-in filtering instead. This function will be removed in a future version."
)]
pub fn should_include_file(
    path: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
    include_priority: bool,
    c2pignore_patterns: &[String],
) -> bool {
    // ~~~ Clean path ~~~
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to canonicalize path: {}", e);
            return false;
        }
    };
    let path_str = canonical_path.to_str().unwrap();

    // ~~~ Check c2pignore patterns first ~~~
    for pat in c2pignore_patterns {
        if Pattern::new(pat).unwrap().matches(path_str) {
            debug!(
                "Path '{}' matched c2pignore pattern '{}'; excluded.",
                path_str, pat
            );
            return false;
        }
    }

    // ~~~ Check glob patterns ~~~
    let included = include_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));
    let excluded = exclude_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));

    // ~~~ Decision ~~~
    let result = match (included, excluded) {
        (true, true) => include_priority, // If both include and exclude patterns match, use the include_priority flag
        (true, false) => true,            // If the path is included and not excluded, include it
        (false, true) => false,           // If the path is excluded, exclude it
        (false, false) => include_patterns.is_empty(), // If no include patterns are provided, include everything
    };

    debug!(
        "Checking path: {:?}, {}: {}, {}: {}, decision: {}",
        path_str,
        "included".bold().green(),
        included,
        "excluded".bold().red(),
        excluded,
        result
    );
    result
}
