# GitHub Grab (ghgrab)

> A fast, modern tool with both a **Graphical User Interface (GUI)** and a **Terminal User Interface (TUI)** to browse, preview, and cherry-pick specific files, folders, and release binaries from GitHub and other Git forges without full cloning.

---

## 🐙 GitHub Grab GUI

The **GitHub Grab GUI** provides a cross-platform desktop application built with pure Rust (`egui` and `tokio`).

### Key GUI Features:
- 🌙 **High-Contrast Themes**: Built-in **GitHub Dark** (default) and **GitHub Light** themes with high-contrast, crystal-clear typography (WCAG AAA readability), plus an optional **Vaporwave High-Contrast** theme.
- 📁 **Enhanced File Browser**: Directories sorted first alphabetically, folder navigation with parent (`⬆ Up`) navigation, and clickable breadcrumb paths.
- 📄 **Live Source Code Viewer**: Integrated code viewer with line numbers, copy-to-clipboard, monospace formatting, and automatic binary file detection.
- 🚀 **Release Vault**: Browse releases, inspect release notes and binary assets, and download assets directly.
- 🔍 **Repository Radar**: Search GitHub repositories by keyword, inspect star counts and primary languages, and open repositories directly into the file browser.
- ⚡ **Asynchronous Downloads**: Concurrent parallel streams with configurable worker limits (1–16) and live progress bars.
- 🔑 **Authentication & Rate Limits**: Personal Access Token configuration with show/hide password toggle and automatic 1-click token import from GitHub CLI (`gh auth token`).

### Running the GUI:
```bash
cargo run --release --bin ghgrab-gui
```

### Pre-built Releases:
Download ready-to-run release packages from the [GitHub Releases](https://github.com/Vonmark90/GitHubGrabGUI/releases) page:
- **macOS** (`.zip` with `.app` bundle for Apple Silicon and Intel)
- **Linux** (`.tar.gz` with desktop entry and icon)
- **Windows** (`.zip` with standalone `.exe`)

---

## 💻 ghgrab CLI & TUI

### Quick Start (Terminal):

```bash
# Launch interactive TUI
ghgrab

# Browse a repository directly
ghgrab https://github.com/ratatui/ratatui

# Download directly to current directory
ghgrab https://github.com/rust-lang/rust --cwd --no-folder
```

### Key CLI/TUI Features:
- **No more clone-and-delete**: Grab exactly what you need without waiting for a full `git clone`.
- **Clean terminal interface**: Built with `ratatui` for an interactive, fast keyboard-driven workflow.
- **LFS Support**: Built-in detection and downloading for GitHub Large File Storage.
- **Batch mode**: Select multiple items and folders to download concurrently.

---

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for details.
