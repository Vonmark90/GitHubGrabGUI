use eframe::egui::{self, Color32, Margin, RichText, Stroke, Vec2};
use ghgrab::config::Config;
use ghgrab::download::Downloader;
use ghgrab::github::{
    GitHubClient, GitHubRelease, GitHubReleaseAsset, GitHubUrl, RepoItem, SearchItem,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// Multi-Theme System (GitHub Dark, GitHub Light, Vaporwave High-Contrast)
// ============================================================================

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    GitHubDark,
    GitHubLight,
    Vaporwave,
}

#[derive(Debug, Clone)]
pub struct AppTheme {
    pub is_dark: bool,

    // Backgrounds
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_panel: Color32,
    pub bg_card: Color32,
    pub bg_card_alt: Color32,
    pub bg_input: Color32,
    pub bg_code: Color32,

    // Borders
    pub border: Color32,
    pub border_subtle: Color32,
    pub border_focus: Color32,

    // Text hierarchy
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_on_accent: Color32,

    // Accents & Actions
    pub accent: Color32,
    pub accent_hover: Color32,
    pub success: Color32,
    pub success_hover: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub folder: Color32,
    pub row_selected: Color32,
    pub row_hover: Color32,
}

impl ThemeMode {
    pub fn get_theme(self) -> AppTheme {
        match self {
            ThemeMode::GitHubDark => AppTheme {
                is_dark: true,
                bg_base: Color32::from_rgb(13, 17, 23), // #0d1117 (GitHub canvas)
                bg_surface: Color32::from_rgb(22, 27, 34), // #161b22 (GitHub header/sidebar)
                bg_panel: Color32::from_rgb(22, 27, 34), // #161b22
                bg_card: Color32::from_rgb(33, 38, 45), // #21262d (GitHub card)
                bg_card_alt: Color32::from_rgb(48, 54, 61), // #30363d
                bg_input: Color32::from_rgb(13, 17, 23), // #0d1117
                bg_code: Color32::from_rgb(10, 12, 16), // #0a0c10 (Deep code editor)
                border: Color32::from_rgb(48, 54, 61),  // #30363d
                border_subtle: Color32::from_rgb(33, 38, 45), // #21262d
                border_focus: Color32::from_rgb(88, 166, 255), // #58a6ff
                text_primary: Color32::from_rgb(240, 246, 252), // #f0f6fc (High-contrast white)
                text_secondary: Color32::from_rgb(201, 209, 217), // #c9d1d9
                text_muted: Color32::from_rgb(139, 148, 158), // #8b949e
                text_on_accent: Color32::WHITE,
                accent: Color32::from_rgb(88, 166, 255), // #58a6ff (GitHub Blue)
                accent_hover: Color32::from_rgb(121, 192, 255),
                success: Color32::from_rgb(35, 134, 54), // #238636 (GitHub Green)
                success_hover: Color32::from_rgb(46, 160, 67), // #2ea043
                warning: Color32::from_rgb(210, 153, 34), // #d29922 (GitHub Gold)
                error: Color32::from_rgb(248, 81, 73),   // #f85149 (GitHub Red)
                folder: Color32::from_rgb(121, 192, 255), // #79c0ff (Directory cyan-blue)
                row_selected: Color32::from_rgb(30, 48, 75), // Dark blue row selection
                row_hover: Color32::from_rgb(28, 33, 40),
            },
            ThemeMode::GitHubLight => AppTheme {
                is_dark: false,
                bg_base: Color32::from_rgb(255, 255, 255),
                bg_surface: Color32::from_rgb(246, 248, 250),
                bg_panel: Color32::from_rgb(246, 248, 250),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_card_alt: Color32::from_rgb(234, 238, 242),
                bg_input: Color32::from_rgb(255, 255, 255),
                bg_code: Color32::from_rgb(246, 248, 250),
                border: Color32::from_rgb(208, 215, 222),
                border_subtle: Color32::from_rgb(234, 238, 242),
                border_focus: Color32::from_rgb(9, 105, 218),
                text_primary: Color32::from_rgb(31, 35, 40),
                text_secondary: Color32::from_rgb(101, 109, 118),
                text_muted: Color32::from_rgb(125, 133, 144),
                text_on_accent: Color32::WHITE,
                accent: Color32::from_rgb(9, 105, 218),
                accent_hover: Color32::from_rgb(4, 82, 178),
                success: Color32::from_rgb(31, 136, 61),
                success_hover: Color32::from_rgb(26, 127, 55),
                warning: Color32::from_rgb(154, 103, 0),
                error: Color32::from_rgb(207, 34, 46),
                folder: Color32::from_rgb(9, 105, 218),
                row_selected: Color32::from_rgb(221, 234, 254),
                row_hover: Color32::from_rgb(240, 243, 246),
            },
            ThemeMode::Vaporwave => AppTheme {
                is_dark: true,
                bg_base: Color32::from_rgb(18, 12, 34), // #120c22 Deep nocturnal purple
                bg_surface: Color32::from_rgb(28, 18, 52), // #1c1234
                bg_panel: Color32::from_rgb(28, 18, 52),
                bg_card: Color32::from_rgb(44, 26, 80), // #2c1a50
                bg_card_alt: Color32::from_rgb(60, 36, 108), // #3c246c
                bg_input: Color32::from_rgb(24, 14, 44),
                bg_code: Color32::from_rgb(12, 8, 24), // Pitch synth console
                border: Color32::from_rgb(90, 50, 150),
                border_subtle: Color32::from_rgb(66, 34, 118),
                border_focus: Color32::from_rgb(1, 205, 254),
                text_primary: Color32::from_rgb(255, 255, 255), // Pure crisp white for readability
                text_secondary: Color32::from_rgb(230, 220, 250), // High-contrast pale lavender
                text_muted: Color32::from_rgb(180, 160, 215),   // Readable bright lavender
                text_on_accent: Color32::from_rgb(18, 12, 34),
                accent: Color32::from_rgb(1, 205, 254), // Neon Cyan
                accent_hover: Color32::from_rgb(90, 225, 255),
                success: Color32::from_rgb(5, 255, 161), // Neon Mint
                success_hover: Color32::from_rgb(50, 255, 180),
                warning: Color32::from_rgb(255, 251, 150), // Sunset Gold
                error: Color32::from_rgb(255, 113, 206),   // Hot Pink
                folder: Color32::from_rgb(1, 205, 254),
                row_selected: Color32::from_rgb(55, 32, 100),
                row_hover: Color32::from_rgb(38, 22, 70),
            },
        }
    }

    pub fn apply(self, ctx: &egui::Context) {
        let theme = self.get_theme();
        let mut visuals = if theme.is_dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        visuals.dark_mode = theme.is_dark;
        visuals.override_text_color = None; // Never hard-override; preserves RichText styling!
        visuals.panel_fill = theme.bg_surface;
        visuals.window_fill = theme.bg_base;
        visuals.extreme_bg_color = theme.bg_input;
        visuals.faint_bg_color = theme.bg_card;
        visuals.code_bg_color = theme.bg_code;
        visuals.hyperlink_color = theme.accent;
        visuals.warn_fg_color = theme.warning;
        visuals.error_fg_color = theme.error;

        visuals.selection.bg_fill = if theme.is_dark {
            Color32::from_rgb(40, 75, 120)
        } else {
            Color32::from_rgb(180, 215, 255)
        };
        visuals.selection.stroke = Stroke::new(1.0, theme.accent);

        // Non-interactive widgets
        visuals.widgets.noninteractive.bg_fill = theme.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.text_secondary);

        // Inactive widgets
        visuals.widgets.inactive.bg_fill = theme.bg_card;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.text_primary);

        // Hovered widgets
        visuals.widgets.hovered.bg_fill = theme.bg_card_alt;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, theme.border_focus);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, theme.text_primary);

        // Active widgets
        visuals.widgets.active.bg_fill = theme.border_focus.gamma_multiply(0.25);
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, theme.border_focus);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, theme.text_primary);

        // Open widgets
        visuals.widgets.open.bg_fill = theme.bg_card_alt;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, theme.border_focus);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, theme.text_primary);

        ctx.set_visuals(visuals);
    }
}

// ============================================================================
// Cross-Platform Helper Functions
// ============================================================================

pub fn open_path_in_file_manager(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

pub fn open_url_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
}

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "ico"
            | "webp"
            | "svg"
            | "bmp"
            | "tiff"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "pdf"
            | "exe"
            | "bin"
            | "dll"
            | "so"
            | "dylib"
            | "class"
            | "wasm"
            | "mp3"
            | "mp4"
            | "wav"
            | "flac"
            | "ogg"
            | "webm"
            | "mkv"
            | "avi"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "eot"
            | "iso"
            | "dmg"
            | "pkg"
            | "deb"
            | "rpm"
    )
}

// ============================================================================
// Data Models & Worker Messages
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Browser,
    Releases,
    Search,
    Settings,
}

enum GuiCommand {
    LoadRepo {
        url: String,
        token: Option<String>,
    },
    NavigateTo {
        path: String,
    },
    FetchFilePreview {
        item: RepoItem,
    },
    FetchReleases,
    SearchRepositories {
        query: String,
    },
    DownloadBatch {
        items: Vec<RepoItem>,
        dest: PathBuf,
        token: Option<String>,
        jobs: usize,
    },
    DownloadReleaseAsset {
        asset: GitHubReleaseAsset,
        dest: PathBuf,
        token: Option<String>,
    },
}

enum WorkerEvent {
    RepoLoaded {
        gh_url: GitHubUrl,
        items: Vec<RepoItem>,
    },
    FolderLoaded {
        path: String,
        items: Vec<RepoItem>,
    },
    FilePreviewLoaded {
        #[allow(dead_code)]
        path: String,
        content: String,
    },
    ReleasesLoaded(Vec<GitHubRelease>),
    SearchResultsLoaded(Vec<SearchItem>),
    DownloadProgress {
        message: String,
        completed: usize,
        total: usize,
    },
    DownloadFinished {
        dest: PathBuf,
        count: usize,
        errors: Vec<String>,
    },
    Error(String),
}

// ============================================================================
// Main Application State
// ============================================================================

struct GhGrabGuiApp {
    // Theme
    theme_mode: ThemeMode,

    // Inputs & Options
    repo_url_input: String,
    token_input: String,
    show_token: bool,
    search_query_input: String,
    filter_input: String,
    dest_path: PathBuf,
    concurrency: usize,

    // Navigation & Repo State
    current_gh_url: Option<GitHubUrl>,
    current_path: String,
    path_breadcrumbs: Vec<String>,
    items: Vec<RepoItem>,
    selected_paths: HashSet<String>,

    // Preview
    selected_preview_item: Option<RepoItem>,
    preview_content: Option<String>,
    preview_loading: bool,

    // Releases & Search
    releases: Vec<GitHubRelease>,
    search_results: Vec<SearchItem>,

    // UI state
    active_tab: AppTab,
    status_message: Option<(String, bool, Instant)>,
    is_busy: bool,
    busy_message: String,
    download_progress: Option<(usize, usize, String)>,
    download_logs: Vec<String>,

    // Async worker channels
    cmd_tx: Sender<GuiCommand>,
    event_rx: Receiver<WorkerEvent>,
}

impl GhGrabGuiApp {
    fn new(cc: &eframe::CreationContext) -> Self {
        let theme_mode = ThemeMode::GitHubDark;
        theme_mode.apply(&cc.egui_ctx);

        let (cmd_tx, cmd_rx) = channel::<GuiCommand>();
        let (event_tx, event_rx) = channel::<WorkerEvent>();

        // Spawn Tokio worker thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");

            rt.block_on(async move {
                let mut current_client: Option<GitHubClient> = None;
                let mut current_url: Option<GitHubUrl> = None;

                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        GuiCommand::LoadRepo { url, token } => match GitHubUrl::parse(&url) {
                            Ok(gh_url) => match GitHubClient::new_for_url(token, &gh_url) {
                                Ok(client) => {
                                    let api_url = gh_url.api_url();
                                    match client.fetch_contents(&api_url).await {
                                        Ok(mut items) => {
                                            client
                                                .resolve_lfs_files(
                                                    &mut items,
                                                    &gh_url.owner,
                                                    &gh_url.repo,
                                                    &gh_url.branch,
                                                )
                                                .await;
                                            current_client = Some(client);
                                            current_url = Some(gh_url.clone());
                                            let _ = event_tx
                                                .send(WorkerEvent::RepoLoaded { gh_url, items });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(WorkerEvent::Error(format!(
                                                "Failed to fetch repository contents: {}",
                                                e
                                            )));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::Error(format!(
                                        "Failed to initialize GitHub client: {}",
                                        e
                                    )));
                                }
                            },
                            Err(e) => {
                                let _ = event_tx.send(WorkerEvent::Error(format!(
                                    "Invalid repository URL: {}",
                                    e
                                )));
                            }
                        },

                        GuiCommand::NavigateTo { path } => {
                            if let (Some(client), Some(gh_url)) = (&current_client, &current_url) {
                                let api_url = gh_url.contents_api_url_for_path(&path);
                                match client.fetch_contents(&api_url).await {
                                    Ok(mut items) => {
                                        client
                                            .resolve_lfs_files(
                                                &mut items,
                                                &gh_url.owner,
                                                &gh_url.repo,
                                                &gh_url.branch,
                                            )
                                            .await;
                                        let _ = event_tx
                                            .send(WorkerEvent::FolderLoaded { path, items });
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::Error(format!(
                                            "Failed to open folder: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                        }

                        GuiCommand::FetchFilePreview { item } => {
                            if let (Some(client), Some(gh_url)) = (&current_client, &current_url) {
                                let raw_url = gh_url.raw_file_url_for_path(&item.path);
                                match client.fetch_partial_content(&raw_url, 120_000).await {
                                    Ok(content) => {
                                        let _ = event_tx.send(WorkerEvent::FilePreviewLoaded {
                                            path: item.path,
                                            content,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::FilePreviewLoaded {
                                            path: item.path,
                                            content: format!("[Could not load preview: {}]", e),
                                        });
                                    }
                                }
                            }
                        }

                        GuiCommand::FetchReleases => {
                            if let (Some(client), Some(gh_url)) = (&current_client, &current_url) {
                                match client.fetch_releases(&gh_url.owner, &gh_url.repo).await {
                                    Ok(releases) => {
                                        let _ =
                                            event_tx.send(WorkerEvent::ReleasesLoaded(releases));
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::Error(format!(
                                            "Failed to fetch releases: {}",
                                            e
                                        )));
                                    }
                                }
                            } else {
                                let _ = event_tx.send(WorkerEvent::Error(
                                    "No repository currently loaded.".to_string(),
                                ));
                            }
                        }

                        GuiCommand::SearchRepositories { query } => {
                            let client = current_client
                                .clone()
                                .unwrap_or_else(|| GitHubClient::new(None).unwrap());
                            match client.search_repositories(&query).await {
                                Ok(results) => {
                                    let _ =
                                        event_tx.send(WorkerEvent::SearchResultsLoaded(results));
                                }
                                Err(e) => {
                                    let _ = event_tx
                                        .send(WorkerEvent::Error(format!("Search failed: {}", e)));
                                }
                            }
                        }

                        GuiCommand::DownloadBatch {
                            items,
                            dest,
                            token,
                            jobs,
                        } => {
                            let total = items.len();
                            let client = if let Some(ref c) = current_client {
                                c.clone()
                            } else {
                                match GitHubClient::new(token) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::Error(format!(
                                            "Failed to initialize client: {}",
                                            e
                                        )));
                                        continue;
                                    }
                                }
                            };

                            match Downloader::new(dest.clone(), client, jobs) {
                                Ok(downloader) => {
                                    let progress_tx = event_tx.clone();
                                    let completed_counter = Arc::new(AtomicUsize::new(0));
                                    let errors = match downloader
                                        .download_items(&items, "", move |msg| {
                                            let count = completed_counter
                                                .fetch_add(1, Ordering::SeqCst)
                                                + 1;
                                            let _ =
                                                progress_tx.send(WorkerEvent::DownloadProgress {
                                                    message: msg,
                                                    completed: count,
                                                    total,
                                                });
                                        })
                                        .await
                                    {
                                        Ok(errs) => errs,
                                        Err(e) => vec![e.to_string()],
                                    };

                                    let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                        dest,
                                        count: total,
                                        errors,
                                    });
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::Error(format!(
                                        "Failed to create downloader: {}",
                                        e
                                    )));
                                }
                            }
                        }

                        GuiCommand::DownloadReleaseAsset { asset, dest, token } => {
                            let client = if let Some(ref c) = current_client {
                                c.clone()
                            } else {
                                match GitHubClient::new(token) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::Error(format!(
                                            "Failed to initialize client: {}",
                                            e
                                        )));
                                        continue;
                                    }
                                }
                            };

                            let _ = event_tx.send(WorkerEvent::DownloadProgress {
                                message: format!("Downloading release asset: {}", asset.name),
                                completed: 0,
                                total: 1,
                            });

                            let _ = std::fs::create_dir_all(&dest);
                            match client.fetch_bytes(&asset.browser_download_url).await {
                                Ok(bytes) => {
                                    let target_file = dest.join(&asset.name);
                                    match std::fs::write(&target_file, bytes) {
                                        Ok(_) => {
                                            let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                                dest: target_file,
                                                count: 1,
                                                errors: vec![],
                                            });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                                dest,
                                                count: 0,
                                                errors: vec![format!(
                                                    "Failed to write file: {}",
                                                    e
                                                )],
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                        dest,
                                        count: 0,
                                        errors: vec![format!("Failed to download asset: {}", e)],
                                    });
                                }
                            }
                        }
                    }
                }
            });
        });

        // Initialize default configuration
        let cfg = Config::load().unwrap_or_default();
        let default_dest = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ghgrab");

        // Try reading gh cli token if available
        let mut initial_token = cfg.github_token.unwrap_or_default();
        if initial_token.is_empty() {
            if let Ok(output) = Command::new("gh").args(["auth", "token"]).output() {
                if output.status.success() {
                    let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !tok.is_empty() {
                        initial_token = tok;
                    }
                }
            }
        }

        Self {
            theme_mode,
            repo_url_input: "https://github.com/ratatui/ratatui".to_string(),
            token_input: initial_token,
            show_token: false,
            search_query_input: String::new(),
            filter_input: String::new(),
            dest_path: default_dest,
            concurrency: 4,

            current_gh_url: None,
            current_path: String::new(),
            path_breadcrumbs: Vec::new(),
            items: Vec::new(),
            selected_paths: HashSet::new(),

            selected_preview_item: None,
            preview_content: None,
            preview_loading: false,

            releases: Vec::new(),
            search_results: Vec::new(),

            active_tab: AppTab::Browser,
            status_message: None,
            is_busy: false,
            busy_message: String::new(),
            download_progress: None,
            download_logs: Vec::new(),

            cmd_tx,
            event_rx,
        }
    }

    fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_message = Some((msg.into(), is_error, Instant::now()));
    }

    fn sort_items(&mut self) {
        self.items.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                WorkerEvent::RepoLoaded { gh_url, items } => {
                    self.is_busy = false;
                    self.current_gh_url = Some(gh_url);
                    self.current_path = String::new();
                    self.path_breadcrumbs.clear();
                    self.items = items;
                    self.sort_items();
                    self.selected_paths.clear();
                    self.selected_preview_item = None;
                    self.preview_content = None;
                    self.set_status("Repository loaded successfully", false);
                }
                WorkerEvent::FolderLoaded { path, items } => {
                    self.is_busy = false;
                    self.current_path = path.clone();
                    self.path_breadcrumbs = if path.is_empty() {
                        Vec::new()
                    } else {
                        path.split('/').map(|s| s.to_string()).collect()
                    };
                    self.items = items;
                    self.sort_items();
                    self.selected_preview_item = None;
                    self.preview_content = None;
                    self.set_status(format!("Opened folder: /{}", path), false);
                }
                WorkerEvent::FilePreviewLoaded { content, .. } => {
                    self.preview_loading = false;
                    self.preview_content = Some(content);
                }
                WorkerEvent::ReleasesLoaded(releases) => {
                    self.is_busy = false;
                    let count = releases.len();
                    self.releases = releases;
                    self.set_status(format!("Loaded {} release(s)", count), false);
                }
                WorkerEvent::SearchResultsLoaded(results) => {
                    self.is_busy = false;
                    let count = results.len();
                    self.search_results = results;
                    self.set_status(format!("Found {} repositories", count), false);
                }
                WorkerEvent::DownloadProgress {
                    message,
                    completed,
                    total,
                } => {
                    self.download_progress = Some((completed, total, message.clone()));
                    self.download_logs.push(message);
                    if self.download_logs.len() > 100 {
                        self.download_logs.remove(0);
                    }
                }
                WorkerEvent::DownloadFinished {
                    dest,
                    count,
                    errors,
                } => {
                    self.is_busy = false;
                    self.download_progress = None;
                    if errors.is_empty() {
                        self.set_status(
                            format!("Successfully downloaded {} item(s) to {:?}", count, dest),
                            false,
                        );
                        self.download_logs
                            .push(format!("Success: {} item(s) saved to {:?}", count, dest));
                    } else {
                        let err_msg = errors.join(", ");
                        self.set_status(format!("Download errors: {}", err_msg), true);
                        self.download_logs.push(format!("Errors: {}", err_msg));
                    }
                }
                WorkerEvent::Error(err) => {
                    self.is_busy = false;
                    self.preview_loading = false;
                    self.set_status(err, true);
                }
            }
        }
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * 1024;
        const GB: u64 = 1024 * 1024 * 1024;
        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn get_file_icon(name: &str, is_dir: bool, is_lfs: bool) -> &'static str {
        if is_dir {
            return "📁";
        }
        if is_lfs {
            return "📦";
        }
        let ext = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "rs" => "🦀",
            "py" => "🐍",
            "js" | "jsx" | "ts" | "tsx" => "📜",
            "json" | "toml" | "yaml" | "yml" => "⚙️",
            "md" | "txt" | "rst" => "📝",
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "🖼️",
            "zip" | "tar" | "gz" | "bz2" | "7z" => "📦",
            "sh" | "bash" | "zsh" => "🐚",
            "html" | "css" | "scss" => "🌐",
            "lock" => "🔒",
            _ => "📄",
        }
    }

    // ========================================================================
    // Top Bar & Header
    // ========================================================================

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        egui::Frame::new()
            .fill(theme.bg_surface)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Title & Logo
                    ui.label(RichText::new("🐙").size(24.0));
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("GitHub Grab")
                                    .size(18.0)
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.label(RichText::new("GUI").size(16.0).strong().color(theme.accent));
                            ui.add_space(4.0);
                            ui.label(RichText::new("v2.2.0").size(10.0).color(theme.text_muted));
                        });
                        ui.label(
                            RichText::new(
                                "Fast repository files, folders & release asset downloader",
                            )
                            .size(11.0)
                            .color(theme.text_secondary),
                        );
                    });

                    ui.add_space(20.0);

                    // Navigation Tabs
                    let tabs = [
                        (AppTab::Browser, "📁 File Browser"),
                        (AppTab::Releases, "🚀 Releases & Assets"),
                        (AppTab::Search, "🔍 Search Repos"),
                        (AppTab::Settings, "⚙️ Settings"),
                    ];

                    for (tab, label) in tabs {
                        let is_active = self.active_tab == tab;
                        let text = RichText::new(label).size(13.0);
                        let text = if is_active {
                            text.strong().color(theme.accent)
                        } else {
                            text.color(theme.text_secondary)
                        };

                        if ui.selectable_label(is_active, text).clicked() {
                            self.active_tab = tab;
                            if tab == AppTab::Releases
                                && self.releases.is_empty()
                                && self.current_gh_url.is_some()
                            {
                                self.is_busy = true;
                                self.busy_message = "Fetching repository releases...".to_string();
                                let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                            }
                        }
                    }

                    // Right Side: Busy status & Theme Switcher
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Theme Switcher Quick Toggle
                        let prev_theme = self.theme_mode;
                        egui::ComboBox::from_id_salt("theme_selector_top")
                            .selected_text(match self.theme_mode {
                                ThemeMode::GitHubDark => "🌙 Dark",
                                ThemeMode::GitHubLight => "☀️ Light",
                                ThemeMode::Vaporwave => "🌴 Vaporwave",
                            })
                            .width(115.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.theme_mode,
                                    ThemeMode::GitHubDark,
                                    "🌙 Dark",
                                );
                                ui.selectable_value(
                                    &mut self.theme_mode,
                                    ThemeMode::GitHubLight,
                                    "☀️ Light",
                                );
                                ui.selectable_value(
                                    &mut self.theme_mode,
                                    ThemeMode::Vaporwave,
                                    "🌴 Vaporwave",
                                );
                            });

                        if self.theme_mode != prev_theme {
                            self.theme_mode.apply(ui.ctx());
                        }

                        ui.add_space(10.0);

                        // Status message / Busy Spinner
                        if self.is_busy {
                            ui.spinner();
                            ui.label(
                                RichText::new(&self.busy_message)
                                    .size(12.0)
                                    .color(theme.warning),
                            );
                        } else if let Some((msg, is_err, time)) = &self.status_message {
                            if time.elapsed().as_secs() < 8 {
                                let color = if *is_err { theme.error } else { theme.success };
                                let icon = if *is_err { "❌ " } else { "✓ " };
                                ui.label(
                                    RichText::new(format!("{}{}", icon, msg))
                                        .size(12.0)
                                        .color(color),
                                );
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // URL Input Row
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Repo URL:")
                            .size(13.0)
                            .strong()
                            .color(theme.text_primary),
                    );

                    let text_edit = ui.add_sized(
                        [ui.available_width() - 250.0, 28.0],
                        egui::TextEdit::singleline(&mut self.repo_url_input)
                            .hint_text("https://github.com/owner/repo or GitLab / Gitea URL"),
                    );

                    let enter_pressed =
                        text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    let load_btn = ui.add(
                        egui::Button::new(
                            RichText::new("⚡ Load Repo")
                                .strong()
                                .color(theme.text_on_accent),
                        )
                        .fill(theme.accent)
                        .min_size(Vec2::new(100.0, 28.0)),
                    );

                    if load_btn.clicked() || enter_pressed {
                        let token = if self.token_input.trim().is_empty() {
                            None
                        } else {
                            Some(self.token_input.trim().to_string())
                        };
                        self.is_busy = true;
                        self.busy_message = "Fetching repository contents...".to_string();
                        let _ = self.cmd_tx.send(GuiCommand::LoadRepo {
                            url: self.repo_url_input.trim().to_string(),
                            token,
                        });
                    }

                    if ui
                        .button(RichText::new("📋 Paste").color(theme.text_secondary))
                        .clicked()
                    {
                        if let Some(text) = ui.ctx().input(|i| {
                            i.events.iter().find_map(|e| {
                                if let egui::Event::Paste(s) = e {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            })
                        }) {
                            self.repo_url_input = text;
                        }
                    }
                });

                ui.add_space(4.0);

                // Quick Picks row
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Quick Picks:")
                            .size(11.0)
                            .color(theme.text_muted),
                    );
                    let quick_picks = [
                        ("ratatui/ratatui", "https://github.com/ratatui/ratatui"),
                        ("tokio-rs/tokio", "https://github.com/tokio-rs/tokio"),
                        ("rust-lang/rust", "https://github.com/rust-lang/rust"),
                        ("astral-sh/uv", "https://github.com/astral-sh/uv"),
                        ("emilk/egui", "https://github.com/emilk/egui"),
                    ];

                    for (name, url) in quick_picks {
                        if ui
                            .button(RichText::new(name).size(11.0).color(theme.accent))
                            .clicked()
                        {
                            self.repo_url_input = url.to_string();
                            let token = if self.token_input.trim().is_empty() {
                                None
                            } else {
                                Some(self.token_input.trim().to_string())
                            };
                            self.is_busy = true;
                            self.busy_message = format!("Loading {}...", name);
                            let _ = self.cmd_tx.send(GuiCommand::LoadRepo {
                                url: url.to_string(),
                                token,
                            });
                        }
                    }
                });
            });
    }

    // ========================================================================
    // Left Sidebar: Download Matrix & Repository Meta
    // ========================================================================

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        egui::Frame::new()
            .fill(theme.bg_panel)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.heading(
                    RichText::new("⬇ Download Options")
                        .size(15.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add_space(8.0);

                // Destination Folder Card
                ui.label(
                    RichText::new("Destination Directory:")
                        .size(12.0)
                        .strong()
                        .color(theme.text_secondary),
                );

                egui::Frame::new()
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(self.dest_path.to_string_lossy())
                                .size(11.0)
                                .monospace()
                                .color(theme.text_primary),
                        );
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("📂 Choose Folder...").color(theme.text_primary))
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_directory(&self.dest_path)
                            .pick_folder()
                        {
                            self.dest_path = folder;
                        }
                    }

                    if ui
                        .button(RichText::new("🔍 Open Folder").color(theme.accent))
                        .clicked()
                    {
                        open_path_in_file_manager(&self.dest_path);
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // Concurrency Slider
                ui.label(
                    RichText::new(format!("Parallel Streams: {}", self.concurrency))
                        .size(12.0)
                        .strong()
                        .color(theme.text_secondary),
                );
                ui.add(egui::Slider::new(&mut self.concurrency, 1..=16).show_value(false));

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // Selection Stats & Actions
                let total_items = self.items.len();
                let selected_count = self.selected_paths.len();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Selected Items:")
                            .size(12.0)
                            .strong()
                            .color(theme.text_secondary),
                    );
                    ui.label(
                        RichText::new(format!("{}/{}", selected_count, total_items))
                            .size(12.0)
                            .strong()
                            .color(theme.accent),
                    );
                });

                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("Select All").color(theme.text_primary))
                        .clicked()
                    {
                        for item in &self.items {
                            self.selected_paths.insert(item.path.clone());
                        }
                    }
                    if ui
                        .button(RichText::new("Clear").color(theme.text_muted))
                        .clicked()
                    {
                        self.selected_paths.clear();
                    }
                    if ui
                        .button(RichText::new("Files Only").color(theme.text_secondary))
                        .clicked()
                    {
                        self.selected_paths.clear();
                        for item in &self.items {
                            if item.is_file() {
                                self.selected_paths.insert(item.path.clone());
                            }
                        }
                    }
                });

                ui.add_space(14.0);

                // Prominent Main Download Button
                let can_download = selected_count > 0 && !self.is_busy;
                let btn_text = if selected_count == 0 {
                    "Select items to download".to_string()
                } else {
                    format!("⬇ Download Selected ({})", selected_count)
                };

                let download_btn = ui.add_enabled(
                    can_download,
                    egui::Button::new(RichText::new(btn_text).size(13.0).strong().color(
                        if can_download {
                            theme.text_on_accent
                        } else {
                            theme.text_muted
                        },
                    ))
                    .fill(if can_download {
                        theme.success
                    } else {
                        theme.bg_card
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if can_download {
                            theme.border_focus
                        } else {
                            theme.border_subtle
                        },
                    ))
                    .min_size(Vec2::new(ui.available_width(), 38.0)),
                );

                if download_btn.clicked() {
                    let selected_items: Vec<RepoItem> = self
                        .items
                        .iter()
                        .filter(|item| self.selected_paths.contains(&item.path))
                        .cloned()
                        .collect();

                    let token = if self.token_input.trim().is_empty() {
                        None
                    } else {
                        Some(self.token_input.trim().to_string())
                    };

                    self.is_busy = true;
                    self.busy_message = format!("Downloading {} items...", selected_items.len());
                    let _ = self.cmd_tx.send(GuiCommand::DownloadBatch {
                        items: selected_items,
                        dest: self.dest_path.clone(),
                        token,
                        jobs: self.concurrency,
                    });
                }

                // Progress Bar Display
                if let Some((completed, total, msg)) = &self.download_progress {
                    ui.add_space(12.0);
                    let progress = if *total > 0 {
                        (*completed as f32 / *total as f32).min(1.0)
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                    ui.label(RichText::new(msg).size(11.0).color(theme.warning));
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                // Repository Metadata Card
                if let Some(gh_url) = &self.current_gh_url {
                    egui::Frame::new()
                        .fill(theme.bg_card)
                        .stroke(Stroke::new(1.0, theme.border_subtle))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("📦 Repository Info")
                                    .size(12.0)
                                    .strong()
                                    .color(theme.accent),
                            );
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Owner:").size(11.0).color(theme.text_muted),
                                );
                                ui.label(
                                    RichText::new(&gh_url.owner)
                                        .size(11.0)
                                        .strong()
                                        .color(theme.text_primary),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Repo:").size(11.0).color(theme.text_muted));
                                ui.label(
                                    RichText::new(&gh_url.repo)
                                        .size(11.0)
                                        .strong()
                                        .color(theme.text_primary),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Branch:").size(11.0).color(theme.text_muted),
                                );
                                ui.label(
                                    RichText::new(&gh_url.branch)
                                        .size(11.0)
                                        .monospace()
                                        .color(theme.text_secondary),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Host:").size(11.0).color(theme.text_muted));
                                ui.label(
                                    RichText::new(gh_url.platform.host())
                                        .size(11.0)
                                        .color(theme.text_secondary),
                                );
                            });

                            ui.add_space(6.0);
                            let web_url = format!(
                                "https://{}/{}/{}",
                                gh_url.platform.host(),
                                gh_url.owner,
                                gh_url.repo
                            );
                            if ui
                                .button(
                                    RichText::new("🌐 View on GitHub")
                                        .size(11.0)
                                        .color(theme.accent),
                                )
                                .clicked()
                            {
                                open_url_in_browser(&web_url);
                            }
                        });
                }
            });
    }

    // ========================================================================
    // File Browser Tab
    // ========================================================================

    fn render_browser_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left pane: File list (55% width)
            let left_width = ui.available_width() * 0.55;
            ui.allocate_ui(Vec2::new(left_width, ui.available_height()), |ui| {
                self.render_file_list(ui);
            });

            ui.separator();

            // Right pane: File Preview
            ui.allocate_ui(
                Vec2::new(ui.available_width(), ui.available_height()),
                |ui| {
                    self.render_preview_panel(ui);
                },
            );
        });
    }

    fn render_file_list(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        // Breadcrumb Navigation Bar
        ui.horizontal(|ui| {
            let has_parent = !self.current_path.is_empty();
            if ui
                .add_enabled(has_parent, egui::Button::new("⬆ Up"))
                .clicked()
            {
                let parent_path = if let Some(idx) = self.current_path.rfind('/') {
                    self.current_path[..idx].to_string()
                } else {
                    String::new()
                };
                self.is_busy = true;
                self.busy_message = if parent_path.is_empty() {
                    "Navigating to root...".to_string()
                } else {
                    format!("Navigating to /{}...", parent_path)
                };
                let _ = self
                    .cmd_tx
                    .send(GuiCommand::NavigateTo { path: parent_path });
            }

            if ui.button("🏠 root").clicked() {
                self.is_busy = true;
                self.busy_message = "Navigating to root...".to_string();
                let _ = self.cmd_tx.send(GuiCommand::NavigateTo {
                    path: String::new(),
                });
            }

            let mut accum_path = String::new();
            for (idx, seg) in self.path_breadcrumbs.clone().iter().enumerate() {
                ui.label(RichText::new("/").color(theme.text_muted));
                if idx > 0 {
                    accum_path.push('/');
                }
                accum_path.push_str(seg);
                let target_path = accum_path.clone();

                let is_last = idx == self.path_breadcrumbs.len() - 1;
                let text = RichText::new(seg).strong();
                let text = if is_last {
                    text.color(theme.accent)
                } else {
                    text.color(theme.text_primary)
                };

                if ui.button(text).clicked() {
                    self.is_busy = true;
                    self.busy_message = format!("Navigating to /{}...", target_path);
                    let _ = self
                        .cmd_tx
                        .send(GuiCommand::NavigateTo { path: target_path });
                }
            }
        });

        ui.add_space(4.0);

        // Filter search input
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔍").size(13.0));
            ui.add_sized(
                [ui.available_width() - 75.0, 24.0],
                egui::TextEdit::singleline(&mut self.filter_input)
                    .hint_text("Filter files in folder..."),
            );
            if !self.filter_input.is_empty() && ui.button("✕ Clear").clicked() {
                self.filter_input.clear();
            }
        });

        ui.add_space(4.0);
        ui.separator();

        // Empty repository state
        if self.items.is_empty() && !self.is_busy {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("📁").size(32.0));
                ui.label(
                    RichText::new("No files loaded.")
                        .size(14.0)
                        .strong()
                        .color(theme.text_secondary),
                );
                ui.label(
                    RichText::new("Enter a GitHub repository URL above and click 'Load Repo'.")
                        .size(12.0)
                        .color(theme.text_muted),
                );
            });
            return;
        }

        // File and Folder List Scroll Area
        egui::ScrollArea::vertical().show(ui, |ui| {
            let filter = self.filter_input.to_lowercase();
            let mut navigate_to: Option<String> = None;
            let mut preview_item: Option<RepoItem> = None;

            for item in &self.items {
                if !filter.is_empty() && !item.name.to_lowercase().contains(&filter) {
                    continue;
                }

                let is_selected_for_download = self.selected_paths.contains(&item.path);
                let is_active_preview = self
                    .selected_preview_item
                    .as_ref()
                    .map(|i| i.path == item.path)
                    .unwrap_or(false);

                // Row frame with active preview highlight
                let row_bg = if is_active_preview {
                    theme.row_selected
                } else {
                    Color32::TRANSPARENT
                };

                egui::Frame::new()
                    .fill(row_bg)
                    .inner_margin(Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Checkbox for batch download
                            let mut checked = is_selected_for_download;
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    self.selected_paths.insert(item.path.clone());
                                } else {
                                    self.selected_paths.remove(&item.path);
                                }
                            }

                            // File / Folder Icon
                            let icon = Self::get_file_icon(&item.name, item.is_dir(), item.is_lfs());
                            ui.label(RichText::new(icon).size(14.0));

                            // Item Name Button
                            let name_text = RichText::new(&item.name).size(13.0);
                            let name_text = if item.is_dir() {
                                name_text.strong().color(theme.folder)
                            } else if is_active_preview {
                                name_text.strong().color(theme.accent)
                            } else {
                                name_text.color(theme.text_primary)
                            };

                            let name_btn = ui.add(egui::Button::new(name_text).frame(false));
                            if name_btn.clicked() {
                                if item.is_dir() {
                                    navigate_to = Some(item.path.clone());
                                } else {
                                    preview_item = Some(item.clone());
                                }
                            }

                            // Right-aligned Size & Quick Download
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(RichText::new("⬇").size(12.0).color(theme.accent)).on_hover_text("Download this item").clicked() {
                                    let token = if self.token_input.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.token_input.trim().to_string())
                                    };
                                    self.is_busy = true;
                                    self.busy_message = format!("Downloading {}...", item.name);
                                    let _ = self.cmd_tx.send(GuiCommand::DownloadBatch {
                                        items: vec![item.clone()],
                                        dest: self.dest_path.clone(),
                                        token,
                                        jobs: 1,
                                    });
                                }

                                if let Some(size) = item.actual_size() {
                                    ui.label(
                                        RichText::new(Self::format_bytes(size))
                                            .size(11.0)
                                            .monospace()
                                            .color(theme.text_muted),
                                    );
                                } else if item.is_dir() {
                                    ui.label(RichText::new("folder").size(11.0).color(theme.text_muted));
                                } else if item.is_lfs() {
                                    ui.label(RichText::new("LFS").size(11.0).color(theme.warning));
                                }
                            });
                        });
                    });

                ui.separator();
            }

            if let Some(path) = navigate_to {
                self.is_busy = true;
                self.busy_message = format!("Opening /{}...", path);
                let _ = self.cmd_tx.send(GuiCommand::NavigateTo { path });
            }

            if let Some(item) = preview_item {
                self.selected_preview_item = Some(item.clone());
                let ext = Path::new(&item.name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if is_binary_extension(ext) {
                    self.preview_loading = false;
                    self.preview_content = Some(format!(
                        "[Binary File: {} ({})]\nBinary files cannot be previewed in text mode.\nClick 'Download File' above to download this file.",
                        item.name,
                        item.actual_size().map(Self::format_bytes).unwrap_or_else(|| "unknown size".into())
                    ));
                } else {
                    self.preview_loading = true;
                    self.preview_content = None;
                    let _ = self.cmd_tx.send(GuiCommand::FetchFilePreview { item });
                }
            }
        });
    }

    // ========================================================================
    // File Preview Panel
    // ========================================================================

    fn render_preview_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("Source Preview")
                    .size(14.0)
                    .strong()
                    .color(theme.text_primary),
            );

            let preview_item_opt = self.selected_preview_item.clone();
            let preview_content_opt = self.preview_content.clone();

            if let Some(ref item) = preview_item_opt {
                let mut trigger_download = false;
                let mut trigger_copy = false;

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let download_btn = ui.add(
                        egui::Button::new(
                            RichText::new("⬇ Download File")
                                .size(11.0)
                                .strong()
                                .color(theme.text_on_accent),
                        )
                        .fill(theme.success),
                    );

                    if download_btn.clicked() {
                        trigger_download = true;
                    }

                    if preview_content_opt.is_some()
                        && ui
                            .button(
                                RichText::new("📋 Copy")
                                    .size(11.0)
                                    .color(theme.text_primary),
                            )
                            .clicked()
                    {
                        trigger_copy = true;
                    }

                    if let Some(size) = item.actual_size() {
                        ui.label(
                            RichText::new(Self::format_bytes(size))
                                .size(11.0)
                                .monospace()
                                .color(theme.text_muted),
                        );
                    }
                });

                if trigger_download {
                    let token = if self.token_input.trim().is_empty() {
                        None
                    } else {
                        Some(self.token_input.trim().to_string())
                    };
                    self.is_busy = true;
                    self.busy_message = format!("Downloading {}...", item.name);
                    let _ = self.cmd_tx.send(GuiCommand::DownloadBatch {
                        items: vec![item.clone()],
                        dest: self.dest_path.clone(),
                        token,
                        jobs: 1,
                    });
                }

                if trigger_copy {
                    if let Some(content) = &preview_content_opt {
                        ui.ctx().copy_text(content.clone());
                        self.set_status("Copied code to clipboard!", false);
                    }
                }
            }
        });

        ui.separator();

        if let Some(item) = &self.selected_preview_item {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&item.path)
                        .size(11.0)
                        .monospace()
                        .color(theme.accent),
                );
            });

            if self.preview_loading {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading file preview...")
                            .size(12.0)
                            .color(theme.text_secondary),
                    );
                });
            } else if let Some(content) = &self.preview_content {
                egui::Frame::new()
                    .fill(theme.bg_code)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.vertical(|ui| {
                                for (idx, line) in content.lines().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{:>4} │", idx + 1))
                                                .size(11.0)
                                                .monospace()
                                                .color(theme.text_muted),
                                        );
                                        ui.label(
                                            RichText::new(line)
                                                .size(11.0)
                                                .monospace()
                                                .color(theme.text_primary),
                                        );
                                    });
                                }
                            });
                        });
                    });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new("📄").size(36.0));
                ui.label(
                    RichText::new("Select a file from the repository to view its contents.")
                        .size(13.0)
                        .color(theme.text_muted),
                );
            });
        }
    }

    // ========================================================================
    // Releases Tab
    // ========================================================================

    fn render_releases_tab(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("🚀 Releases & Binary Assets")
                    .size(16.0)
                    .strong()
                    .color(theme.text_primary),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("🔄 Refresh").color(theme.accent))
                    .clicked()
                {
                    self.is_busy = true;
                    self.busy_message = "Fetching repository releases...".to_string();
                    let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                }
            });
        });

        ui.separator();

        if self.releases.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new("📦").size(36.0));
                ui.label(
                    RichText::new("No releases cached. Load a repository first.")
                        .size(13.0)
                        .color(theme.text_muted),
                );
                if ui
                    .button(RichText::new("Fetch Releases Now").color(theme.accent))
                    .clicked()
                {
                    self.is_busy = true;
                    self.busy_message = "Fetching repository releases...".to_string();
                    let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                }
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut download_asset: Option<GitHubReleaseAsset> = None;

            for release in &self.releases {
                egui::Frame::new()
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&release.tag_name)
                                    .size(15.0)
                                    .strong()
                                    .color(theme.accent),
                            );
                            if release.prerelease {
                                ui.label(
                                    RichText::new("[Pre-release]")
                                        .size(11.0)
                                        .color(theme.warning),
                                );
                            }
                            if release.draft {
                                ui.label(
                                    RichText::new("[Draft]").size(11.0).color(theme.text_muted),
                                );
                            }
                        });

                        ui.add_space(6.0);

                        for asset in &release.assets {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📦").size(13.0));
                                ui.label(
                                    RichText::new(&asset.name)
                                        .size(13.0)
                                        .strong()
                                        .color(theme.text_primary),
                                );
                                ui.label(
                                    RichText::new(format!("({})", Self::format_bytes(asset.size)))
                                        .size(11.0)
                                        .monospace()
                                        .color(theme.text_muted),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .button(
                                                RichText::new("⬇ Download Asset")
                                                    .color(theme.success),
                                            )
                                            .clicked()
                                        {
                                            download_asset = Some(asset.clone());
                                        }
                                    },
                                );
                            });
                            ui.separator();
                        }
                    });
                ui.add_space(8.0);
            }

            if let Some(asset) = download_asset {
                let token = if self.token_input.trim().is_empty() {
                    None
                } else {
                    Some(self.token_input.trim().to_string())
                };
                self.is_busy = true;
                self.busy_message = format!("Downloading release asset {}...", asset.name);
                let _ = self.cmd_tx.send(GuiCommand::DownloadReleaseAsset {
                    asset,
                    dest: self.dest_path.clone(),
                    token,
                });
            }
        });
    }

    // ========================================================================
    // Repository Search Tab
    // ========================================================================

    fn render_search_tab(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        ui.heading(
            RichText::new("🔍 Search GitHub Repositories")
                .size(16.0)
                .strong()
                .color(theme.text_primary),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let search_edit = ui.add_sized(
                [ui.available_width() - 130.0, 28.0],
                egui::TextEdit::singleline(&mut self.search_query_input)
                    .hint_text("Search by keyword (e.g. ratatui, tokio, tui, rust-cli)..."),
            );

            let enter_pressed =
                search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let search_btn = ui.add(
                egui::Button::new(
                    RichText::new("🔍 Search")
                        .strong()
                        .color(theme.text_on_accent),
                )
                .fill(theme.accent)
                .min_size(Vec2::new(100.0, 28.0)),
            );

            if (search_btn.clicked() || enter_pressed) && !self.search_query_input.trim().is_empty()
            {
                self.is_busy = true;
                self.busy_message = format!("Searching for '{}'...", self.search_query_input);
                let _ = self.cmd_tx.send(GuiCommand::SearchRepositories {
                    query: self.search_query_input.trim().to_string(),
                });
            }
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut load_repo_url: Option<String> = None;

            for item in &self.search_results {
                egui::Frame::new()
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&item.full_name)
                                    .size(14.0)
                                    .strong()
                                    .color(theme.accent),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(
                                            RichText::new("⚡ Load Repo")
                                                .color(theme.text_on_accent),
                                        )
                                        .clicked()
                                    {
                                        load_repo_url = Some(item.html_url.clone());
                                    }
                                    if ui
                                        .button(
                                            RichText::new("🌐 View").color(theme.text_secondary),
                                        )
                                        .clicked()
                                    {
                                        open_url_in_browser(&item.html_url);
                                    }
                                    ui.label(
                                        RichText::new(format!("★ {}", item.stargazers_count))
                                            .size(12.0)
                                            .color(theme.warning),
                                    );
                                    if let Some(lang) = &item.language {
                                        ui.label(
                                            RichText::new(lang).size(11.0).color(theme.success),
                                        );
                                    }
                                },
                            );
                        });

                        if let Some(desc) = &item.description {
                            ui.add_space(4.0);
                            ui.label(RichText::new(desc).size(12.0).color(theme.text_secondary));
                        }
                    });
                ui.add_space(8.0);
            }

            if let Some(url) = load_repo_url {
                self.repo_url_input = url.clone();
                self.active_tab = AppTab::Browser;
                let token = if self.token_input.trim().is_empty() {
                    None
                } else {
                    Some(self.token_input.trim().to_string())
                };
                self.is_busy = true;
                self.busy_message = format!("Loading {}...", url);
                let _ = self.cmd_tx.send(GuiCommand::LoadRepo { url, token });
            }
        });
    }

    // ========================================================================
    // Settings & Configuration Tab
    // ========================================================================

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme_mode.get_theme();

        ui.heading(
            RichText::new("⚙️ Settings & Configuration")
                .size(16.0)
                .strong()
                .color(theme.text_primary),
        );
        ui.add_space(10.0);

        // Appearance / Theme Section
        egui::Frame::new()
            .fill(theme.bg_card)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Interface Appearance & Theme")
                        .size(13.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new(
                        "Choose an appearance theme designed for optimal contrast and readability.",
                    )
                    .size(11.0)
                    .color(theme.text_muted),
                );

                ui.add_space(8.0);

                let prev_theme = self.theme_mode;
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.theme_mode,
                        ThemeMode::GitHubDark,
                        "🌙 GitHub Dark (Default)",
                    );
                    ui.selectable_value(
                        &mut self.theme_mode,
                        ThemeMode::GitHubLight,
                        "☀️ GitHub Light",
                    );
                    ui.selectable_value(
                        &mut self.theme_mode,
                        ThemeMode::Vaporwave,
                        "🌴 Vaporwave (High Contrast)",
                    );
                });

                if self.theme_mode != prev_theme {
                    self.theme_mode.apply(ui.ctx());
                }
            });

        ui.add_space(12.0);

        // GitHub Authentication Card
        egui::Frame::new()
            .fill(theme.bg_card)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("GitHub Personal Access Token")
                        .size(13.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new("Providing a token increases your GitHub API rate limit from 60 to 5,000 requests/hr and enables private repository access.")
                        .size(11.0)
                        .color(theme.text_muted),
                );

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.add_sized(
                        [ui.available_width() - 260.0, 26.0],
                        egui::TextEdit::singleline(&mut self.token_input)
                            .password(!self.show_token)
                            .hint_text("ghp_... or gho_..."),
                    );

                    let show_btn_text = if self.show_token { "🙈 Hide" } else { "👁 Show" };
                    if ui.button(show_btn_text).clicked() {
                        self.show_token = !self.show_token;
                    }

                    if ui.button(RichText::new("🔑 Import from `gh`").color(theme.accent)).clicked() {
                        if let Ok(output) = Command::new("gh").args(["auth", "token"]).output() {
                            if output.status.success() {
                                let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                if !tok.is_empty() {
                                    self.token_input = tok;
                                    self.set_status("Token imported from GitHub CLI!", false);
                                }
                            } else {
                                self.set_status("Failed to read token from gh CLI. Run `gh auth login` first.", true);
                            }
                        }
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    RichText::new("Default Download Directory")
                        .size(13.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new(self.dest_path.to_string_lossy())
                        .size(11.0)
                        .monospace()
                        .color(theme.accent),
                );

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button(RichText::new("📂 Change Directory...").color(theme.text_primary)).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().set_directory(&self.dest_path).pick_folder() {
                            self.dest_path = folder;
                        }
                    }

                    if ui.button(RichText::new("🔍 Open in File Manager").color(theme.accent)).clicked() {
                        open_path_in_file_manager(&self.dest_path);
                    }
                });
            });

        ui.add_space(12.0);

        // Activity Log Console
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("Activity Console")
                    .size(13.0)
                    .strong()
                    .color(theme.text_primary),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(
                        RichText::new("Clear Log")
                            .size(11.0)
                            .color(theme.text_muted),
                    )
                    .clicked()
                {
                    self.download_logs.clear();
                }
            });
        });

        egui::Frame::new()
            .fill(theme.bg_code)
            .stroke(Stroke::new(1.0, theme.border_subtle))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        if self.download_logs.is_empty() {
                            ui.label(
                                RichText::new("// No network downloads logged yet //")
                                    .size(11.0)
                                    .color(theme.text_muted),
                            );
                        } else {
                            for log in self.download_logs.iter().rev() {
                                ui.label(
                                    RichText::new(format!("> {}", log))
                                        .size(11.0)
                                        .monospace()
                                        .color(theme.text_secondary),
                                );
                            }
                        }
                    });
            });
    }
}

impl eframe::App for GhGrabGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_events();

        let theme = self.theme_mode.get_theme();

        // Top Header
        egui::Panel::top("top_panel")
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_surface)
                    .stroke(Stroke::new(1.0, theme.border)),
            )
            .show(ui, |ui| {
                self.render_top_bar(ui);
            });

        // Left Sidebar
        egui::Panel::left("left_sidebar")
            .default_size(300.0)
            .min_size(260.0)
            .max_size(380.0)
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_panel)
                    .stroke(Stroke::new(1.0, theme.border)),
            )
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });

        // Central Panel
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_base)
                    .inner_margin(Margin::same(12)),
            )
            .show(ui, |ui| match self.active_tab {
                AppTab::Browser => self.render_browser_tab(ui),
                AppTab::Releases => self.render_releases_tab(ui),
                AppTab::Search => self.render_search_tab(ui),
                AppTab::Settings => self.render_settings_tab(ui),
            });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([850.0, 550.0])
            .with_title("GitHub Grab GUI"),
        ..Default::default()
    };

    eframe::run_native(
        "GitHub Grab GUI",
        options,
        Box::new(|cc| Ok(Box::new(GhGrabGuiApp::new(cc)))),
    )
}
