//! code2prompt is a command-line tool to generate an LLM prompt from one or more codebase directories.
//!
//! Author: Mufeed VH (@mufeedvh)
//! Contributor: Olivier D'Ancona (@ODAncona)

use anyhow::{Context, Result};
use clap::Parser;
use code2prompt::{
    copy_to_clipboard, get_git_diff, get_git_diff_between_branches, get_git_log, get_model_info,
    get_tokenizer, handle_undefined_variables, handlebars_setup, label, read_paths_from_clipboard,
    render_template, traverse_directory, write_to_file,
};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_TEMPLATE_NAME: &str = "default";
const CUSTOM_TEMPLATE_NAME: &str = "custom";

/// CLI Arguments – accepts one or more paths.
#[derive(Parser)]
#[clap(name = "code2prompt", version = "2.0.1", author = "Mufeed VH")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Paths to one or more codebase directories
    #[arg(required_unless_present = "read")]
    paths: Vec<PathBuf>,

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

    /// Optional tokenizer to use for token count (cl100k default)
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

    /// Disable copying to clipboard
    #[clap(long)]
    no_clipboard: bool,

    /// Append to clipboard instead of overwriting
    #[clap(short, long)]
    append: bool,

    /// Optional path to a custom Handlebars template
    #[clap(short, long)]
    template: Option<PathBuf>,

    /// Print output as JSON (ignored – final output is not printed)
    #[clap(long)]
    json: bool,

    /// Read paths from clipboard
    #[clap(long)]
    read: bool,

    /// Sampling rate (defaults to 10 if flag present without a value)
    #[clap(short = 's', long = "sample-rate", default_missing_value = "10")]
    sample_rate: Option<u8>,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::parse();

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
        spinner.finish_with_message("Done!".green().to_string());
        return process_paths(&paths, &args);
    }

    // If not reading from clipboard, do normal CLI path processing
    process_paths(&args.paths, &args)
}

fn process_paths(paths: &[PathBuf], args: &Cli) -> Result<()> {
    if paths.is_empty() {
        return Err(anyhow::anyhow!("No paths provided."));
    }

    let include_patterns = parse_patterns(&args.include);
    let exclude_patterns = parse_patterns(&args.exclude);

    let (template_content, template_name) = get_template(args)?;
    let handlebars = handlebars_setup(&template_content, template_name)?;

    let mut folder_outputs = Vec::new();
    for folder in paths {
        if !folder.exists() {
            eprintln!(
                "{}{}{} {}",
                "[".bold().white(),
                "!".bold().red(),
                "]".bold().white(),
                format!("Path does not exist: {}", folder.display()).red()
            );
            continue;
        }

        let spinner = setup_spinner(&format!("Processing {}...", folder.display()));
        let c2pignore_patterns = load_c2pignore_patterns(folder)?;

        let (full_tree, all_files) = traverse_directory(
            folder,
            &include_patterns,
            &exclude_patterns,
            args.include_priority,
            args.line_number,
            args.relative_paths,
            args.exclude_from_tree,
            args.no_codeblock,
            &c2pignore_patterns,
        )?;

        let (git_diff, git_diff_branch, git_log_branch) = if folder.is_dir() {
            let git_diff = if args.diff {
                spinner.set_message("Generating git diff...");
                get_git_diff(folder).unwrap_or_default()
            } else {
                String::new()
            };

            let mut git_diff_branch = String::new();
            if let Some(branches) = &args.git_diff_branch {
                spinner.set_message("Generating git diff between branches...");
                let branches = parse_patterns(&Some(branches.to_string()));
                if branches.len() != 2 {
                    error!("Please provide exactly two branches separated by a comma.");
                    std::process::exit(1);
                }
                git_diff_branch =
                    get_git_diff_between_branches(folder, &branches[0], &branches[1])
                        .unwrap_or_default();
            }

            let mut git_log_branch = String::new();
            if let Some(branches) = &args.git_log_branch {
                spinner.set_message("Generating git log between branches...");
                let branches = parse_patterns(&Some(branches.to_string()));
                if branches.len() != 2 {
                    error!("Please provide exactly two branches separated by a comma.");
                    std::process::exit(1);
                }
                git_log_branch = get_git_log(folder, &branches[0], &branches[1]).unwrap_or_default();
            }
            (git_diff, git_diff_branch, git_log_branch)
        } else {
            (String::new(), String::new(), String::new())
        };

        spinner.finish_with_message("Done!".green().to_string());

        let mut data = json!({
            "absolute_code_path": label(folder),
            "source_tree": full_tree,
            "files": all_files,
            "git_diff": git_diff,
            "git_diff_branch": git_diff_branch,
            "git_log_branch": git_log_branch
        });

        debug!(
            "JSON Data for {}: {}",
            folder.display(),
            serde_json::to_string_pretty(&data)?
        );

        handle_undefined_variables(&mut data, &template_content)?;
        let rendered = render_template(&handlebars, template_name, &data)?;

        let folder_tag = code2prompt::path::label(folder);
        let wrapped = format!(
            "<{tag}>\n{indented}\n</{tag}>",
            tag = folder_tag,
            indented = indent(&rendered, 2)
        );
        folder_outputs.push(wrapped);
    }

    let final_output = format!(
        "<context>\n{}\n</context>",
        folder_outputs.join("\n\n")
    );

    let token_count = {
        let bpe = get_tokenizer(&args.encoding);
        bpe.encode_with_special_tokens(&final_output).len()
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

    // Do not print the final output; only copy to clipboard.
    if !args.no_clipboard {
        if let Err(e) = copy_to_clipboard(&final_output, args.append) {
            eprintln!(
                "{}{}{} {}",
                "[".bold().white(),
                "!".bold().red(),
                "]".bold().white(),
                format!("Failed to copy to clipboard: {}", e).red()
            );
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
        write_to_file(output_path, &final_output)?;
    }

    Ok(())
}

/// Indents each line of a multiline string by a given number of spaces.
fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{}{}", pad, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sets up a progress spinner with a given message.
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

/// Reads patterns from a `c2pignore` file if it exists in the given folder.
fn load_c2pignore_patterns(root_path: &Path) -> Result<Vec<String>> {
    let c2pignore_path = root_path.join("c2pignore");
    if !c2pignore_path.exists() {
        return Ok(vec![]);
    }
    let contents = fs::read_to_string(&c2pignore_path)
        .context("Failed to read c2pignore file")?;
    let mut patterns = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        patterns.push(trimmed.to_string());
    }
    Ok(patterns)
}

/// Parses comma-separated patterns into a vector of strings.
fn parse_patterns(patterns: &Option<String>) -> Vec<String> {
    match patterns {
        Some(patterns) if !patterns.is_empty() => {
            let mut out = Vec::new();
            for item in patterns.split(',') {
                let trimmed = item.trim();
                if trimmed.eq_ignore_ascii_case("docker") {
                    out.push("**/Dockerfile".to_string());
                    out.push("**/docker-compose.yml".to_string());
                    out.push("**/docker-compose.yaml".to_string());
                } else if trimmed.eq_ignore_ascii_case("env") {
                    out.push("**/.env".to_string());
                    out.push("**/.env.*".to_string());
                } else if trimmed.contains('*') {
                    out.push(trimmed.to_string());
                } else {
                    out.push(format!("**/*.{}", trimmed));
                }
            }
            out
        }
        _ => vec![],
    }
}

/// Retrieves the template content and name based on CLI arguments.
fn get_template(args: &Cli) -> Result<(String, &str)> {
    if let Some(template_path) = &args.template {
        let content = fs::read_to_string(template_path)
            .context("Failed to read custom template file")?;
        Ok((content, CUSTOM_TEMPLATE_NAME))
    } else {
        Ok((
            include_str!("default_template.hbs").to_string(),
            DEFAULT_TEMPLATE_NAME,
        ))
    }
}