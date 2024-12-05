//! code2prompt is a command-line tool to generate an LLM prompt from a codebase directory.
//!
//! Author: Mufeed VH (@mufeedvh)
//! Contributor: Olivier D'Ancona (@ODAncona)

use anyhow::{Context, Result};
use clap::Parser;
use code2prompt::{
    copy_to_clipboard, get_git_diff, get_git_diff_between_branches, get_git_log, get_model_info,
    get_tokenizer, handle_undefined_variables, handlebars_setup, label, read_paths_from_clipboard,
    render_template, traverse_directory, wrap_code_block, write_to_file,
};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error};
use serde_json::json;
use std::path::PathBuf;
use std::fs;

// Constants
const DEFAULT_TEMPLATE_NAME: &str = "default";
const CUSTOM_TEMPLATE_NAME: &str = "custom";

// CLI Arguments
#[derive(Parser)]
#[clap(name = "code2prompt", version = "2.0.1", author = "Mufeed VH")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Path to the codebase directory
    #[arg(required_unless_present = "read")]
    path: Option<PathBuf>,

    /// File extensions to include (comma-separated)
    #[clap(short = 'i', long)]
    include: Option<String>,

    /// Patterns to exclude
    #[clap(long)]
    exclude: Option<String>,

    /// Include files in case of conflict between include and exclude patterns
    #[clap(long)]
    include_priority: bool,

    /// Exclude files/folders from the source tree based on exclude patterns
    #[clap(long)]
    exclude_from_tree: bool,

    /// Optional tokenizer to use for token count
    ///
    /// Supported tokenizers: cl100k (default), p50k, p50k_edit, r50k, gpt2
    #[clap(short = 'c', long)]
    encoding: Option<String>,

    /// Optional output file path
    #[clap(short, long)]
    output: Option<String>,

    /// Include git diff
    #[clap(short, long)]
    diff: bool,

    /// Generate git diff between two branches
    #[clap(long, value_name = "BRANCHES")]
    git_diff_branch: Option<String>,

    /// Retrieve git log between two branches
    #[clap(long, value_name = "BRANCHES")]
    git_log_branch: Option<String>,

    /// Add line numbers to the source code
    #[clap(short, long)]
    line_number: bool,

    /// Disable wrapping code inside markdown code blocks
    #[clap(long)]
    no_codeblock: bool,

    /// Use relative paths instead of absolute paths, including the parent directory
    #[clap(long)]
    relative_paths: bool,

    /// Optional Disable copying to clipboard
    #[clap(long)]
    no_clipboard: bool,

    /// Append to clipboard instead of overwriting
    #[clap(short, long)]
    append: bool,

    /// Optional Path to a custom Handlebars template
    #[clap(short, long)]
    template: Option<PathBuf>,

    /// Print output as JSON
    #[clap(long)]
    json: bool,

    /// Read paths from clipboard
    #[clap(long)]
    read: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::parse();

    // Handle reading from clipboard if --read flag is present
    if args.read {
        let spinner = setup_spinner("Reading paths from clipboard...");
        
        let paths = match read_paths_from_clipboard() {
            Ok(paths) => paths,
            Err(e) => {
                spinner.finish_with_message("Failed!".red().to_string());
                eprintln!(
                    "{}{}{} {}",
                    "[".bold().white(),
                    "!".bold().red(),
                    "]".bold().white(),
                    format!("Failed to read paths from clipboard: {}", e).red()
                );
                std::process::exit(1);
            }
        };

        spinner.set_message("Processing paths...");

        let mut files = Vec::new();
        for path in paths {
            if path.is_file() {
                // Process single file
                if let Ok(code_bytes) = fs::read(&path) {
                    let code = String::from_utf8_lossy(&code_bytes);
                    if !code.trim().is_empty() && !code.contains(char::REPLACEMENT_CHARACTER) {
                        files.push(json!({
                            "path": path.display().to_string(),
                            "extension": path.extension().and_then(|ext| ext.to_str()).unwrap_or(""),
                            "code": wrap_code_block(&code, path.extension().and_then(|ext| ext.to_str()).unwrap_or(""), args.line_number, args.no_codeblock),
                        }));
                    }
                }
            } else if path.is_dir() {
                // Process directory using existing traverse_directory function
                match traverse_directory(
                    &path,
                    &Vec::new(), // No include patterns
                    &Vec::new(), // No exclude patterns
                    false,       // include_priority
                    args.line_number,
                    args.relative_paths,
                    false,      // exclude_from_tree
                    args.no_codeblock,
                ) {
                    Ok((tree, mut dir_files)) => {
                        // Add directory tree as a special file
                        files.push(json!({
                            "path": format!("{} (Directory Structure)", path.display()),
                            "extension": "tree",
                            "code": wrap_code_block(&tree, "", false, args.no_codeblock),
                        }));
                        // Add all files from the directory
                        files.append(&mut dir_files);
                    }
                    Err(e) => {
                        eprintln!(
                            "{}{}{} {}",
                            "[".bold().white(),
                            "!".bold().red(),
                            "]".bold().white(),
                            format!("Failed to process directory {}: {}", path.display(), e).red()
                        );
                    }
                }
            }
        }

        // Prepare data for template
        let data = json!({
            "files": files
        });

        // Render template and handle output
        let (template_content, template_name) = get_template(&args)?;
        let handlebars = handlebars_setup(&template_content, template_name)?;
        let rendered = render_template(&handlebars, template_name, &data)?;

        spinner.finish_with_message("Done!".green().to_string());

        // Display Token Count
        let token_count = {
            let bpe = get_tokenizer(&args.encoding);
            bpe.encode_with_special_tokens(&rendered).len()
        };

        let model_info = get_model_info(&args.encoding);

        println!(
            "{}{}{} Token count: {}, Model info: {}",
            "[".bold().white(),
            "i".bold().blue(),
            "]".bold().white(),
            token_count.to_string().bold().yellow(),
            model_info
        );

        // Handle output options
        if !args.no_clipboard {
            if let Err(e) = copy_to_clipboard(&rendered, args.append) {
                eprintln!(
                    "{}{}{} {}",
                    "[".bold().white(),
                    "!".bold().red(),
                    "]".bold().white(),
                    format!("Failed to copy to clipboard: {}", e).red()
                );
                println!("{}", &rendered);
            } else {
                println!(
                    "{}{}{} {}",
                    "[".bold().white(),
                    "✓".bold().green(),
                    "]".bold().white(),
                    if args.append {
                        "Appended to clipboard successfully.".green()
                    } else {
                        "Copied to clipboard successfully.".green()
                    }
                );
            }
        }

        if let Some(output_path) = &args.output {
            write_to_file(output_path, &rendered)?;
        }

        return Ok(());
    }

    // Handlebars Template Setup
    let (template_content, template_name) = get_template(&args)?;
    let handlebars = handlebars_setup(&template_content, template_name)?;

    // Progress Bar Setup
    let spinner = setup_spinner("Processing path...");

    // Parse Patterns
    let include_patterns = if let Some(ref include) = args.include {
        if include.contains('*') {
            // Handle as glob pattern
            parse_patterns(&Some(include.to_string()))
        } else {
            // Handle as file extensions
            include
                .split(',')
                .map(|ext| format!("*.{}", ext.trim()))
                .collect()
        }
    } else {
        vec![]
    };
    let exclude_patterns = parse_patterns(&args.exclude);

    // Get the path and check if it exists
    let path = args.path.as_ref().expect("Path is required when not using --read");
    if !path.exists() {
        spinner.finish_with_message("Failed!".red().to_string());
        return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
    }

    // Update spinner message based on path type
    if path.is_file() {
        spinner.set_message("Processing file...");
    } else {
        spinner.set_message("Traversing directory and building tree...");
    }

    // Process the path
    let (tree, files) = traverse_directory(
        path,
        &include_patterns,
        &exclude_patterns,
        args.include_priority,
        args.line_number,
        args.relative_paths,
        args.exclude_from_tree,
        args.no_codeblock,
    )?;

    // Git operations (only for directories)
    let (git_diff, git_diff_branch, git_log_branch) = if path.is_dir() {
        // Git Diff
        let git_diff = if args.diff {
            spinner.set_message("Generating git diff...");
            get_git_diff(path).unwrap_or_default()
        } else {
            String::new()
        };

        // Git diff between branches
        let mut git_diff_branch = String::new();
        if let Some(branches) = &args.git_diff_branch {
            spinner.set_message("Generating git diff between two branches...");
            let branches = parse_patterns(&Some(branches.to_string()));
            if branches.len() != 2 {
                error!("Please provide exactly two branches separated by a comma.");
                std::process::exit(1);
            }
            git_diff_branch = get_git_diff_between_branches(path, &branches[0], &branches[1])
                .unwrap_or_default()
        }

        // Git log between branches
        let mut git_log_branch = String::new();
        if let Some(branches) = &args.git_log_branch {
            spinner.set_message("Generating git log between two branches...");
            let branches = parse_patterns(&Some(branches.to_string()));
            if branches.len() != 2 {
                error!("Please provide exactly two branches separated by a comma.");
                std::process::exit(1);
            }
            git_log_branch = get_git_log(path, &branches[0], &branches[1]).unwrap_or_default()
        }

        (git_diff, git_diff_branch, git_log_branch)
    } else {
        (String::new(), String::new(), String::new())
    };

    spinner.finish_with_message("Done!".green().to_string());

    // Prepare JSON Data
    let mut data = json!({
        "absolute_code_path": label(path),
        "source_tree": tree,
        "files": files,
        "git_diff": git_diff,
        "git_diff_branch": git_diff_branch,
        "git_log_branch": git_log_branch
    });

    debug!(
        "JSON Data: {}",
        serde_json::to_string_pretty(&data).unwrap()
    );

    // Handle undefined variables
    handle_undefined_variables(&mut data, &template_content)?;

    // Render the template
    let rendered = render_template(&handlebars, template_name, &data)?;

    // Display Token Count
    let token_count = {
        let bpe = get_tokenizer(&args.encoding);
        bpe.encode_with_special_tokens(&rendered).len()
    };

    let paths: Vec<String> = files
        .iter()
        .filter_map(|file| {
            file.get("path")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    let model_info = get_model_info(&args.encoding);

    if args.json {
        let json_output = json!({
            "prompt": rendered,
            "directory_name": label(path),
            "token_count": token_count,
            "model_info": model_info,
            "files": paths,
        });
        println!("{}", serde_json::to_string_pretty(&json_output)?);
        return Ok(());
    } else {
        println!(
            "{}{}{} Token count: {}, Model info: {}",
            "[".bold().white(),
            "i".bold().blue(),
            "]".bold().white(),
            token_count.to_string().bold().yellow(),
            model_info
        );
    }

    // Copy to Clipboard
    if !args.no_clipboard {
        match copy_to_clipboard(&rendered, args.append) {
            Ok(_) => {
                println!(
                    "{}{}{} {}",
                    "[".bold().white(),
                    "✓".bold().green(),
                    "]".bold().white(),
                    if args.append {
                        "Appended to clipboard successfully.".green()
                    } else {
                        "Copied to clipboard successfully.".green()
                    }
                );
            }
            Err(e) => {
                eprintln!(
                    "{}{}{} {}",
                    "[".bold().white(),
                    "!".bold().red(),
                    "]".bold().white(),
                    format!("Failed to copy to clipboard: {}", e).red()
                );
                println!("{}", &rendered);
            }
        }
    }

    // Output File
    if let Some(output_path) = &args.output {
        write_to_file(output_path, &rendered)?;
    }

    Ok(())
}

/// Sets up a progress spinner with a given message
///
/// # Arguments
///
/// * `message` - A message to display with the spinner
///
/// # Returns
///
/// * `ProgressBar` - The configured progress spinner
fn setup_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(std::time::Duration::from_millis(120));
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["▹▹▹▹▹", "▸▹▹▹▹", "▹▸▹▹▹", "▹▹▸▹▹", "▹▹▹▸▹", "▹▹▹▹▸"])
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    spinner.set_message(message.to_string());
    spinner
}

/// Parses comma-separated patterns into a vector of strings
///
/// # Arguments
///
/// * `patterns` - An optional string containing comma-separated patterns
///
/// # Returns
///
/// * `Vec<String>` - A vector of parsed patterns
fn parse_patterns(patterns: &Option<String>) -> Vec<String> {
    match patterns {
        Some(patterns) if !patterns.is_empty() => {
            patterns.split(',').map(|s| s.trim().to_string()).collect()
        }
        _ => vec![],
    }
}

/// Retrieves the template content and name based on the CLI arguments
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
///
/// # Returns
///
/// * `Result<(String, &str)>` - A tuple containing the template content and name
fn get_template(args: &Cli) -> Result<(String, &str)> {
    if let Some(template_path) = &args.template {
        let content = std::fs::read_to_string(template_path)
            .context("Failed to read custom template file")?;
        Ok((content, CUSTOM_TEMPLATE_NAME))
    } else {
        Ok((
            include_str!("default_template.hbs").to_string(),
            DEFAULT_TEMPLATE_NAME,
        ))
    }
}
