# code2prompt

> This is a fork of [code2prompt](https://github.com/mufeedvh/code2prompt) by [emileberhard](https://github.com/emileberhard).

[![crates.io](https://img.shields.io/crates/v/code2prompt.svg)](https://crates.io/crates/code2prompt)
[![LICENSE](https://img.shields.io/github/license/mufeedvh/code2prompt.svg#cache1)](https://github.com/mufeedvh/code2prompt/blob/master/LICENSE)

`code2prompt` is a command-line tool that converts your codebase into a single LLM prompt with:

- **Source tree** (optional)
- **Rich code inclusion** (with line numbering / code blocks)
- **Filtering** (via glob patterns and `.c2pignore`)
- **Custom templating** (via Handlebars)
- **Token counting** (for OpenAI tiktoken-based models)
- **Git diffs / logs** (for commit messages, PR templates, etc.)

The generated prompt can be automatically copied to your clipboard or saved to a file.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Ignoring Files](#ignoring-files-and-folders)
- [Templates](#templates)
- [User Defined Variables](#user-defined-variables)
- [Tokenizers](#tokenizers)
- [License](#license)
- [Contributing](#contribution)

---

## Features

- **Generate a single prompt** from multiple files/directories.
- **Built-in filtering** using default rules (e.g., `node_modules/`, `*.png`, `*.mp4`) and `.c2pignore`.
- **Glob patterns** for includes/excludes (`--include`, `--exclude`).
- **Git support**: pass `--diff` for staged changes, or `--git-diff-branch` and `--git-log-branch` to compare two branches.
- **Line numbers** in code blocks with `--line-number`.
- **Disable code fence** with `--no-codeblock`.
- **Template** your final output with Handlebars (e.g. generate a bug-fix prompt, a PR description, etc.).
- **Token counting** to see how large your final prompt is.
- **Clipboard** integration; optionally append to the existing clipboard content.
- **Supports multiple directories** in a single run or you can read them from the clipboard with `--read`.

> Use it to quickly load your entire codebase into GPT/Claude for:
> - Documenting code
> - Finding bugs or security vulnerabilities
> - Refactoring or rewriting code
> - Generating commit messages and PR descriptions
> - And more!

---

## Installation

### Local Build

Clone the repository and install locally:

```sh
git clone https://github.com/emileberhard/code2prompt.git
cd code2prompt
cargo install --path .
```

### Binary Releases

Download the prebuilt binaries for your OS from [GitHub Releases](https://github.com/emileberhard/code2prompt/releases).

### Other Methods

- **AUR**: `paru -S code2prompt` (Arch Linux)
- **Nix**: `nix profile install nixpkgs#code2prompt`

---

## Usage

```sh
code2prompt [OPTIONS] [PATHS...]
```

- **Basic run** on a folder:

  ```sh
  code2prompt path/to/codebase
  ```

- **Custom Handlebars template**:

  ```sh
  code2prompt path/to/codebase --template=templates/write-git-commit.hbs
  ```

- **Filtering**:

  - **Include only certain files** (e.g., Python files):
    
    ```sh
    code2prompt path/to/codebase --include="*.py"
    ```
    
  - **Exclude certain files** (e.g., `.txt` files):
    
    ```sh
    code2prompt path/to/codebase --exclude="*.txt"
    ```

- **Exclude from the source tree** (the files won't show up in the tree output, but remain in the final code listing if included):
  
  ```sh
  code2prompt path/to/codebase --exclude="*.npy" --exclude-from-tree
  ```

- **Git diff** (for staged files only) and `--diff-branch` or `--log-branch` for comparing branches:

  ```sh
  code2prompt path/to/git/repo --diff
  code2prompt path/to/git/repo --git-diff-branch="main,feature" --git-log-branch="main,feature"
  ```

- **Line numbers**:

  ```sh
  code2prompt path/to/codebase --line-number
  ```

- **Disable wrapping code** in triple-backtick fences:

  ```sh
  code2prompt path/to/codebase --no-codeblock
  ```

- **Use relative paths** in the final listing:

  ```sh
  code2prompt path/to/codebase --relative-paths
  ```

- **Copy to clipboard** is on by default, but you can disable or append:

  ```sh
  code2prompt path/to/codebase --no-clipboard
  code2prompt path/to/codebase --append
  ```

- **Output to a file**:

  ```sh
  code2prompt path/to/codebase --output=output.txt
  ```

- **Token count** (always shown at the end), specify encoding:

  ```sh
  code2prompt path/to/codebase --encoding=cl100k   # (default)
  code2prompt path/to/codebase --encoding=p50k
  ```
  
- **Read paths from clipboard**:

  ```sh
  code2prompt --read
  ```
  
  This will parse the clipboard contents for valid paths and process them instead of requiring them on the command line.

- **JSON output** is currently **placeholder** only (`--json` will not actually print JSON to stdout). This flag is recognized but does not produce a final JSON output in the current version.

- **Sampling rate** (`--sample-rate`) is a spare integer argument in case you want to attach sampling logic (or future features). It defaults to `10` if you omit the value.

---

## Ignoring Files and Folders

`code2prompt` relies on [**the `ignore` crate**](https://docs.rs/ignore/) plus some *built-in* default ignores for typical junk/artifact folders:
- `.git/`, `.svn/`, `.DS_Store`, `node_modules/`, `target/`, `bin/`, `obj/`, etc.
- Common media (images, audio, videos), large binaries, etc.

You can **extend ignoring** by placing a `.c2pignore` file in your project root. Each line can contain a pattern like `**/secret.txt` or `**/*.pdf`. Patterns are standard glob syntax. Comments (`#`) and blank lines are ignored.

You can also supply your own `--exclude` patterns on the command line, like `--exclude="*.lock,*.png"`. If you use `--include`, it takes precedence only if you specify `--include-priority`.

---

## Templates

`code2prompt` uses [**Handlebars**](https://crates.io/crates/handlebars) to populate a template with contextual data. You can specify your own template file with:

```sh
code2prompt path/to/codebase --template=path/to/template.hbs
```

By default (if no `--template` is given), it uses an internal `default_template.hbs`.

### Built-in Templates

Inside [templates/](templates):

- **`document-the-code.hbs`** – for generating docstrings.
- **`find-security-vulnerabilities.hbs`** – for scanning code for vulnerabilities.
- **`write-git-commit.hbs`** – for generating commit messages from staged diffs.
- **`write-github-pull-request.hbs`** – for generating a PR description comparing two branches, etc.
- … and more.

You can further adapt or create new templates for any LLM use-case.

---

## User Defined Variables

Any `{{variable}}` in the template that isn't part of the built-in data (like `files`, `source_tree`, `git_diff`, etc.) is treated as **user-defined**. `code2prompt` will prompt you (in the CLI) for values. This allows you to incorporate free-form user prompts or extra context into the final output.

---

## Tokenizers

Token counting is powered by [`tiktoken-rs`](https://github.com/zurawiki/tiktoken-rs). Supported encodings:

- `cl100k` (default) – ChatGPT models, `text-embedding-ada-002`
- `p50k` – Davinci code models (`text-davinci-002`, `text-davinci-003`)
- `p50k_edit` – For OpenAI edit models
- `r50k` (alias `gpt2`) – GPT-3 `davinci`  
- (More details in [OpenAI docs](https://github.com/openai/openai-cookbook/blob/main/examples/How_to_count_tokens_with_tiktoken.ipynb))

`code2prompt` prints the total token count in the final step, along with the associated model info.

---

## License

[MIT License](https://github.com/mufeedvh/code2prompt/blob/master/LICENSE).

## Contribution

Pull requests, bug reports, and feature suggestions are all welcome! Give the repo a star if you find it helpful.
