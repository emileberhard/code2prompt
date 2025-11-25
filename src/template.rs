//! This module contains the functions to set up the Handlebars template engine and render the template with the provided data.
//! It also includes functions for handling user-defined variables, copying the rendered output to the clipboard, and writing it to a file.

use anyhow::{Context, Result};
use arboard::Clipboard;
use colored::*;
use handlebars::{no_escape, Handlebars};
use inquire::Text;
use regex::Regex;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Set up the Handlebars template engine with a template string and a template name.
///
/// # Arguments
///
/// * `template_str` - The Handlebars template string.
/// * `template_name` - The name of the template.
///
/// # Returns
///
/// * `Result<Handlebars<'static>>` - The configured Handlebars instance.
pub fn handlebars_setup(template_str: &str, template_name: &str) -> Result<Handlebars<'static>> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);

    handlebars
        .register_template_string(template_name, template_str)
        .map_err(|e| anyhow::anyhow!("Failed to register template: {}", e))?;

    Ok(handlebars)
}

/// Extracts the undefined variables from the template string.
///
/// # Arguments
///
/// * `template` - The Handlebars template string.
///
/// # Returns
///
/// * `Vec<String>` - A vector of undefined variable names.
pub fn extract_undefined_variables(template: &str) -> Vec<String> {
    let registered_identifiers = ["path", "code", "git_diff"];
    let re = Regex::new(r"\{\{\s*(?P<var>[a-zA-Z_][a-zA-Z_0-9]*)\s*\}\}").unwrap();
    re.captures_iter(template)
        .map(|cap| cap["var"].to_string())
        .filter(|var| !registered_identifiers.contains(&var.as_str()))
        .collect()
}

/// Renders the template with the provided data.
///
/// # Arguments
///
/// * `handlebars` - The configured Handlebars instance.
/// * `template_name` - The name of the template.
/// * `data` - The JSON data object.
///
/// # Returns
///
/// * `Result<String>` - The rendered template as a string.
pub fn render_template(
    handlebars: &Handlebars,
    template_name: &str,
    data: &serde_json::Value,
) -> Result<String> {
    let rendered = handlebars
        .render(template_name, data)
        .map_err(|e| anyhow::anyhow!("Failed to render template: {}", e))?;
    Ok(rendered.trim().to_string())
}

/// Handles user-defined variables in the template and adds them to the data.
///
/// # Arguments
///
/// * `data` - The JSON data object.
/// * `template_content` - The template content string.
///
/// # Returns
///
/// * `Result<()>` - An empty result indicating success or an error.
pub fn handle_undefined_variables(
    data: &mut serde_json::Value,
    template_content: &str,
) -> Result<()> {
    let undefined_variables = extract_undefined_variables(template_content);
    let mut user_defined_vars = serde_json::Map::new();

    for var in undefined_variables.iter() {
        if !data.as_object().unwrap().contains_key(var) {
            let prompt = format!("Enter value for '{}': ", var);
            let answer = Text::new(&prompt)
                .with_help_message("Fill user defined variable in template")
                .prompt()
                .unwrap_or_default();
            user_defined_vars.insert(var.clone(), serde_json::Value::String(answer));
        }
    }

    if let Some(obj) = data.as_object_mut() {
        for (key, value) in user_defined_vars {
            obj.insert(key, value);
        }
    }
    Ok(())
}

/// Copies or appends the rendered template to the clipboard.
///
/// # Arguments
///
/// * `rendered` - The rendered template string.
/// * `append` - Whether to append to existing clipboard content.
///
/// # Returns
///
/// * `Result<()>` - An empty result indicating success or an error.
pub fn copy_to_clipboard(rendered: &str, append: bool) -> Result<()> {
    match Clipboard::new() {
        Ok(mut clipboard) => {
            let content = if append {
                match clipboard.get_text() {
                    Ok(existing) => format!("{}\n\n----------\n\n{}", existing, rendered),
                    Err(_) => rendered.to_string(),
                }
            } else {
                rendered.to_string()
            };

            clipboard
                .set_text(content)
                .context("Failed to copy to clipboard")?;
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to initialize clipboard: {}", e)),
    }
}

/// Copies a file as a file reference to the clipboard where supported.
///
/// On macOS and Windows this behaves like copying the file from the file
/// manager (Finder / Explorer). On other platforms it falls back to copying
/// the file contents as plain text.
pub fn copy_file_to_clipboard(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        copy_file_to_clipboard_macos(path)
    }

    #[cfg(target_os = "windows")]
    {
        copy_file_to_clipboard_windows(path)
    }

    #[cfg(target_os = "linux")]
    {
        copy_file_to_clipboard_linux(path)
    }

    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_os = "linux")
    ))]
    {
        copy_file_to_clipboard_fallback(path)
    }
}

#[cfg(target_os = "linux")]
fn copy_file_to_clipboard_linux(path: &Path) -> Result<()> {
    use std::process::{Command, Stdio};

    let abs = path.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize context file path: {}",
            path.display()
        )
    })?;

    // Linux clipboard utilities expect a URI when copying files.
    let uri = format!("file://{}", abs.display());

    // Attempt Wayland clipboard first.
    if let Ok(status) = Command::new("wl-copy")
        .arg("--type")
        .arg("text/uri-list")
        .arg(&uri)
        .status()
    {
        if status.success() {
            return Ok(());
        }
    }

    // Fallback to X11 clipboard via xclip.
    let mut child = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("text/uri-list")
        .stdin(Stdio::piped())
        .spawn()
        .context(
            "Failed to spawn clipboard utility. Please install 'wl-clipboard' (Wayland) or 'xclip' (X11).",
        )?;

    if let Some(mut stdin) = child.stdin.take() {
        write!(stdin, "{}", uri).context("Failed to write to xclip stdin")?;
    }

    let status = child.wait().context("Failed to wait for xclip")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Clipboard utility exited with non-zero status"
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_file_to_clipboard_macos(path: &Path) -> Result<()> {
    use std::process::Command;

    let abs = path.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize context file path: {}",
            path.display()
        )
    })?;

    let path_str = abs.to_str().ok_or_else(|| {
        anyhow::anyhow!("Context file path is not valid UTF-8: {}", abs.display())
    })?;

    let escaped = escape_osascript_string(path_str);

    // Equivalent to manually doing: osascript -e 'tell app "Finder" to set the clipboard
    // to ( POSIX file "/absolute/path/context.txt" )'
    let script = format!(
        r#"tell application "Finder" to set the clipboard to (POSIX file "{}")"#,
        escaped
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .context("Failed to execute osascript to copy context.txt as file")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "osascript exited with non-zero status: {}",
            status
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn escape_osascript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn copy_file_to_clipboard_windows(path: &Path) -> Result<()> {
    use std::process::Command;

    let abs = path.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize context file path: {}",
            path.display()
        )
    })?;

    let path_str = abs.to_str().ok_or_else(|| {
        anyhow::anyhow!("Context file path is not valid UTF-8: {}", abs.display())
    })?;

    // Use PowerShell's Set-Clipboard with a FileDropList so Explorer and other
    // apps see this as a file copied to the clipboard.
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("Get-Item -LiteralPath $env:CODE2PROMPT_CONTEXT_FILE | Set-Clipboard")
        .env("CODE2PROMPT_CONTEXT_FILE", path_str);

    let status = cmd
        .status()
        .context("Failed to execute PowerShell to copy context.txt as file")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "PowerShell exited with non-zero status: {}",
            status
        ));
    }

    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn copy_file_to_clipboard_fallback(path: &Path) -> Result<()> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read context file: {}", path.display()))?;
    // Best-effort: fall back to copying the file contents as text
    copy_to_clipboard(&contents, false)
}

/// Writes the rendered template to a specified output file.
///
/// # Arguments
///
/// * `output_path` - The path to the output file.
/// * `rendered` - The rendered template string.
///
/// # Returns
///
/// * `Result<()>` - An empty result indicating success or an error.
pub fn write_to_file(output_path: &str, rendered: &str) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = std::io::BufWriter::new(file);
    write!(writer, "{}", rendered)?;
    println!(
        "{}{}{} {}",
        "[".bold().white(),
        "✓".bold().green(),
        "]".bold().white(),
        format!("Prompt written to file: {}", output_path).green()
    );
    Ok(())
}

/// Reads and parses paths from clipboard content
///
/// # Arguments
///
/// * `content` - String content from clipboard
///
/// # Returns
///
/// * `Result<Vec<PathBuf>>` - Vector of parsed paths
pub fn parse_paths_from_clipboard(content: &str) -> Result<Vec<PathBuf>> {
    // Use .split_whitespace() to handle all spacing consistently
    let paths: Vec<PathBuf> = content
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists()) // Only include paths that actually exist
        .collect();

    if paths.is_empty() {
        return Err(anyhow::anyhow!("No valid paths found in clipboard"));
    }

    Ok(paths)
}

/// Reads paths from clipboard
///
/// # Returns
/// * `Result<Vec<PathBuf>>` - Vector of paths read from clipboard
pub fn read_paths_from_clipboard() -> Result<Vec<PathBuf>> {
    let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;

    let content = clipboard
        .get_text()
        .context("Failed to get text from clipboard")?;

    parse_paths_from_clipboard(&content)
}
