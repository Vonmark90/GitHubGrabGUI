use eframe::egui::{self, Color32, Margin, RichText, Stroke, Vec2};
use ghgrab::config::Config;
use ghgrab::download::Downloader;
use ghgrab::github::{GitHubClient, GitHubRelease, GitHubReleaseAsset, GitHubUrl, RepoItem, SearchItem};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

/// Signature Vaporwave Color Palette
#[allow(dead_code)]
struct VpColors;

#[allow(dead_code)]
impl VpColors {
    // Deep nocturnal obsidian-purple dark shades (NO white anywhere)
    pub const BG_BASE: Color32 = Color32::from_rgb(14, 8, 25);         // #0e0819 Deep nocturnal purple
    pub const BG_TOP: Color32 = Color32::from_rgb(20, 11, 36);         // #140b24
    pub const BG_SIDEBAR: Color32 = Color32::from_rgb(23, 12, 42);     // #170c2a
    pub const BG_CARD: Color32 = Color32::from_rgb(33, 17, 60);        // #21113c
    pub const BG_CARD_ALT: Color32 = Color32::from_rgb(40, 21, 72);    // #281548
    pub const BG_INPUT: Color32 = Color32::from_rgb(26, 13, 47);       // #1a0d2f
    pub const BG_PREVIEW: Color32 = Color32::from_rgb(9, 4, 17);       // #090411 Pitch dark synth console
    pub const BORDER: Color32 = Color32::from_rgb(66, 34, 118);        // #422276
    pub const BORDER_BRIGHT: Color32 = Color32::from_rgb(125, 62, 220); // #7d3edc

    // Signature Neon Vaporwave Accents
    pub const PINK: Color32 = Color32::from_rgb(255, 113, 206);       // #ff71ce (Neon Hot Pink)
    pub const CYAN: Color32 = Color32::from_rgb(1, 205, 254);         // #01cdfe (Laser Teal / Cyan)
    pub const MINT: Color32 = Color32::from_rgb(5, 255, 161);         // #05ffa1 (Turquoise Mint)
    pub const YELLOW: Color32 = Color32::from_rgb(255, 251, 150);     // #fffb96 (Pastel Sunset Gold)
    pub const PURPLE: Color32 = Color32::from_rgb(185, 103, 255);     // #b967ff (Electric Lavender)

    // Text hierarchy
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(251, 245, 255);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(215, 195, 245);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(155, 130, 190);
    pub const TEXT_DARK: Color32 = Color32::from_rgb(20, 10, 36);
}

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

struct GhGrabGuiApp {
    // Inputs & Options
    repo_url_input: String,
    token_input: String,
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
        // Configure global Vaporwave Visuals in egui
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = VpColors::BG_BASE;
        visuals.window_fill = VpColors::BG_BASE;
        visuals.extreme_bg_color = VpColors::BG_INPUT;
        visuals.faint_bg_color = VpColors::BG_CARD;
        visuals.code_bg_color = VpColors::BG_PREVIEW;
        visuals.hyperlink_color = VpColors::CYAN;
        visuals.warn_fg_color = VpColors::YELLOW;
        visuals.error_fg_color = VpColors::PINK;

        visuals.selection.bg_fill = Color32::from_rgb(140, 50, 160);
        visuals.selection.stroke = Stroke::new(1.0, VpColors::CYAN);

        visuals.widgets.noninteractive.bg_fill = VpColors::BG_CARD;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, VpColors::BORDER);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, VpColors::TEXT_SECONDARY);

        visuals.widgets.inactive.bg_fill = VpColors::BG_CARD;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, VpColors::BORDER);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, VpColors::TEXT_PRIMARY);

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 30, 102);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, VpColors::PINK);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);

        visuals.widgets.active.bg_fill = Color32::from_rgb(82, 42, 148);
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, VpColors::CYAN);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, VpColors::CYAN);

        cc.egui_ctx.set_visuals(visuals);

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
                        GuiCommand::LoadRepo { url, token } => {
                            match GitHubUrl::parse(&url) {
                                Ok(gh_url) => {
                                    match GitHubClient::new_for_url(token, &gh_url) {
                                        Ok(client) => {
                                            let api_url = gh_url.api_url();
                                            match client.fetch_contents(&api_url).await {
                                                Ok(mut items) => {
                                                    client.resolve_lfs_files(&mut items, &gh_url.owner, &gh_url.repo, &gh_url.branch).await;
                                                    current_client = Some(client);
                                                    current_url = Some(gh_url.clone());
                                                    let _ = event_tx.send(WorkerEvent::RepoLoaded {
                                                        gh_url,
                                                        items,
                                                    });
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
                                                "Failed to initialize client: {}",
                                                e
                                            )));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::Error(format!(
                                        "Invalid repository URL: {}",
                                        e
                                    )));
                                }
                            }
                        }

                        GuiCommand::NavigateTo { path } => {
                            if let (Some(client), Some(gh_url)) = (&current_client, &current_url) {
                                let api_url = gh_url.contents_api_url_for_path(&path);
                                match client.fetch_contents(&api_url).await {
                                    Ok(mut items) => {
                                        client.resolve_lfs_files(&mut items, &gh_url.owner, &gh_url.repo, &gh_url.branch).await;
                                        let _ = event_tx.send(WorkerEvent::FolderLoaded { path, items });
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
                                        let _ = event_tx.send(WorkerEvent::ReleasesLoaded(releases));
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
                                    let _ = event_tx.send(WorkerEvent::SearchResultsLoaded(results));
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::Error(format!(
                                        "Search failed: {}",
                                        e
                                    )));
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
                                            "Client creation error: {}",
                                            e
                                        )));
                                        continue;
                                    }
                                }
                            };

                            match Downloader::new(dest.clone(), client, jobs) {
                                Ok(downloader) => {
                                    let tx_clone = event_tx.clone();
                                    let completed_counter = Arc::new(AtomicUsize::new(0));
                                    let progress_cb = move |msg: String| {
                                        let completed = completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                                        let _ = tx_clone.send(WorkerEvent::DownloadProgress {
                                            message: msg,
                                            completed: completed.min(total),
                                            total,
                                        });
                                    };

                                    match downloader.download_items(&items, "", progress_cb).await {
                                        Ok(downloaded) => {
                                            let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                                dest,
                                                count: downloaded.len(),
                                                errors: vec![],
                                            });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(WorkerEvent::DownloadFinished {
                                                dest,
                                                count: 0,
                                                errors: vec![e.to_string()],
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(WorkerEvent::Error(format!(
                                        "Failed to initialize downloader: {}",
                                        e
                                    )));
                                }
                            }
                        }

                        GuiCommand::DownloadReleaseAsset {
                            asset,
                            dest,
                            token,
                        } => {
                            let client = if let Some(ref c) = current_client {
                                c.clone()
                            } else {
                                match GitHubClient::new(token) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let _ = event_tx.send(WorkerEvent::Error(format!(
                                            "Client error: {}",
                                            e
                                        )));
                                        continue;
                                    }
                                }
                            };

                            let _ = event_tx.send(WorkerEvent::DownloadProgress {
                                message: format!("Downloading {}...", asset.name),
                                completed: 0,
                                total: 1,
                            });

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
                                                errors: vec![format!("Failed to write file: {}", e)],
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
            repo_url_input: "https://github.com/ratatui/ratatui".to_string(),
            token_input: initial_token,
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

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                WorkerEvent::RepoLoaded { gh_url, items } => {
                    self.is_busy = false;
                    self.current_gh_url = Some(gh_url);
                    self.current_path = String::new();
                    self.path_breadcrumbs.clear();
                    self.items = items;
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
                        self.download_logs.push(format!(
                            "Completed: {} item(s) saved to {:?}",
                            count, dest
                        ));
                    } else {
                        let err_msg = errors.join(", ");
                        self.set_status(format!("Download completed with errors: {}", err_msg), true);
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
            format!("{:.0} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(VpColors::BG_TOP)
            .stroke(Stroke::new(1.0, VpColors::BORDER))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Title: Git Hub Grab GUI Vaporwave
                    ui.label(RichText::new("🌴").size(26.0));
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Git Hub Grab GUI")
                                    .size(20.0)
                                    .strong()
                                    .color(VpColors::PINK),
                            );
                            ui.label(
                                RichText::new("Vaporwave")
                                    .size(17.0)
                                    .strong()
                                    .color(VpColors::CYAN),
                            );
                        });
                        ui.label(
                            RichText::new("「 ＧＩＴ ＨＵＢ ＧＲＡＢ // ＶＡＰＯＲＷＡＶＥ 」")
                                .size(10.0)
                                .color(VpColors::PURPLE),
                        );
                    });

                    ui.add_space(16.0);

                    // Tab buttons styled with vaporwave neon colors
                    let tabs = [
                        (AppTab::Browser, "📁 REPO FILES"),
                        (AppTab::Releases, "🚀 RELEASE VAULT"),
                        (AppTab::Search, "🔍 REPO RADAR"),
                        (AppTab::Settings, "⚙️ CYBER CONFIG"),
                    ];

                    for (tab, label) in tabs {
                        let is_active = self.active_tab == tab;
                        let text = RichText::new(label).size(13.0);
                        let text = if is_active {
                            text.strong().color(VpColors::PINK)
                        } else {
                            text.color(VpColors::TEXT_MUTED)
                        };

                        if ui.selectable_label(is_active, text).clicked() {
                            self.active_tab = tab;
                            if tab == AppTab::Releases && self.releases.is_empty() && self.current_gh_url.is_some() {
                                self.is_busy = true;
                                self.busy_message = "Fetching releases...".to_string();
                                let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                            }
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.is_busy {
                            ui.spinner();
                            ui.label(RichText::new(&self.busy_message).size(12.0).color(VpColors::YELLOW));
                        } else if let Some((msg, is_err, time)) = &self.status_message {
                            if time.elapsed().as_secs() < 8 {
                                let color = if *is_err {
                                    VpColors::PINK
                                } else {
                                    VpColors::MINT
                                };
                                ui.label(RichText::new(msg).size(12.0).color(color));
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // URL Input Row
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Repo URL:").size(13.0).strong().color(VpColors::CYAN));

                    let text_edit = ui.add_sized(
                        [ui.available_width() - 260.0, 26.0],
                        egui::TextEdit::singleline(&mut self.repo_url_input)
                            .hint_text("https://github.com/owner/repo or GitLab / Gitea URL"),
                    );

                    let enter_pressed = text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    let load_btn = ui.add(
                        egui::Button::new(
                            RichText::new("⚡ LOAD REPO")
                                .strong()
                                .color(VpColors::TEXT_DARK),
                        )
                        .fill(VpColors::PINK),
                    );

                    if load_btn.clicked() || enter_pressed {
                        let token = if self.token_input.trim().is_empty() {
                            None
                        } else {
                            Some(self.token_input.trim().to_string())
                        };
                        self.is_busy = true;
                        self.busy_message = "Fetching repo contents...".to_string();
                        let _ = self.cmd_tx.send(GuiCommand::LoadRepo {
                            url: self.repo_url_input.trim().to_string(),
                            token,
                        });
                    }

                    if ui.button(RichText::new("📋 Paste").color(VpColors::CYAN)).clicked() {
                        if let Some(text) = ui.ctx().input(|i| i.events.iter().find_map(|e| {
                            if let egui::Event::Paste(s) = e {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })) {
                            self.repo_url_input = text;
                        }
                    }
                });

                // Quick Picks row in vaporwave neon pastels
                ui.horizontal(|ui| {
                    ui.label(RichText::new("QUICK PICKS:").size(11.0).color(VpColors::TEXT_MUTED));
                    let quick_picks = [
                        ("ratatui/ratatui", "https://github.com/ratatui/ratatui", VpColors::CYAN),
                        ("tokio-rs/tokio", "https://github.com/tokio-rs/tokio", VpColors::PINK),
                        ("rust-lang/rust", "https://github.com/rust-lang/rust", VpColors::MINT),
                        ("astral-sh/uv", "https://github.com/astral-sh/uv", VpColors::YELLOW),
                        ("emilk/egui", "https://github.com/emilk/egui", VpColors::PURPLE),
                    ];

                    for (name, url, color) in quick_picks {
                        if ui.link(RichText::new(name).size(11.0).color(color)).clicked() {
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

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(VpColors::BG_SIDEBAR)
            .stroke(Stroke::new(1.0, VpColors::BORDER))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.heading(RichText::new("❖ DOWNLOAD MATRIX").size(15.0).strong().color(VpColors::PINK));
                ui.add_space(8.0);

                // Destination Folder
                ui.label(RichText::new("Destination Folder:").size(12.0).strong().color(VpColors::TEXT_SECONDARY));
                ui.label(
                    RichText::new(self.dest_path.to_string_lossy())
                        .size(11.0)
                        .monospace()
                        .color(VpColors::CYAN),
                );

                ui.horizontal(|ui| {
                    if ui.button(RichText::new("📂 Choose Folder...").color(VpColors::PURPLE)).clicked() {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_directory(&self.dest_path)
                            .pick_folder()
                        {
                            self.dest_path = folder;
                        }
                    }

                    if ui.button(RichText::new("🔍 In Finder").color(VpColors::MINT)).clicked() {
                        let _ = std::fs::create_dir_all(&self.dest_path);
                        let _ = Command::new("open").arg(&self.dest_path).spawn();
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Concurrency Slider
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Parallel Streams:").color(VpColors::TEXT_SECONDARY));
                    ui.add(egui::Slider::new(&mut self.concurrency, 1..=16));
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Selection Stats
                let total_items = self.items.len();
                let selected_count = self.selected_paths.len();

                ui.label(
                    RichText::new(format!("SELECTED // {} / {}", selected_count, total_items))
                        .strong()
                        .color(VpColors::YELLOW),
                );

                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Select All").color(VpColors::CYAN)).clicked() {
                        for item in &self.items {
                            self.selected_paths.insert(item.path.clone());
                        }
                    }
                    if ui.button(RichText::new("Clear").color(VpColors::TEXT_MUTED)).clicked() {
                        self.selected_paths.clear();
                    }
                });

                ui.add_space(16.0);

                let can_download = selected_count > 0 && self.is_busy == false;
                let btn_text = if selected_count == 0 {
                    "▼ SELECT ITEMS TO DOWNLOAD".to_string()
                } else {
                    format!("▼ DOWNLOAD SELECTED ({})", selected_count)
                };

                let download_btn = ui.add_enabled(
                    can_download,
                    egui::Button::new(
                        RichText::new(btn_text)
                            .size(13.0)
                            .strong()
                            .color(if can_download { VpColors::TEXT_DARK } else { VpColors::TEXT_MUTED }),
                    )
                    .fill(if can_download {
                        VpColors::PINK
                    } else {
                        Color32::from_rgb(50, 26, 75)
                    })
                    .stroke(Stroke::new(1.0, if can_download { VpColors::CYAN } else { Color32::TRANSPARENT }))
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

                if let Some((completed, total, msg)) = &self.download_progress {
                    ui.add_space(12.0);
                    let progress = if *total > 0 {
                        *completed as f32 / *total as f32
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                    ui.label(RichText::new(msg).size(11.0).color(VpColors::YELLOW));
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // Repository Info card
                if let Some(gh_url) = &self.current_gh_url {
                    egui::Frame::new()
                        .fill(VpColors::BG_CARD)
                        .stroke(Stroke::new(1.0, VpColors::BORDER))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.heading(RichText::new("❖ REPO META").size(12.0).strong().color(VpColors::CYAN));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Owner:").size(11.0).color(VpColors::MINT));
                                ui.label(RichText::new(&gh_url.owner).size(11.0).color(VpColors::TEXT_PRIMARY));
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Repo:").size(11.0).color(VpColors::PINK));
                                ui.label(RichText::new(&gh_url.repo).size(11.0).color(VpColors::TEXT_PRIMARY));
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Branch:").size(11.0).color(VpColors::YELLOW));
                                ui.label(RichText::new(&gh_url.branch).size(11.0).color(VpColors::TEXT_PRIMARY));
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Host:").size(11.0).color(VpColors::PURPLE));
                                ui.label(RichText::new(gh_url.platform.host()).size(11.0).color(VpColors::TEXT_PRIMARY));
                            });
                        });
                }
            });
    }

    fn render_browser_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left pane: File list (55% width)
            let left_width = ui.available_width() * 0.55;
            ui.allocate_ui(Vec2::new(left_width, ui.available_height()), |ui| {
                self.render_file_list(ui);
            });

            ui.separator();

            // Right pane: File Preview
            ui.allocate_ui(Vec2::new(ui.available_width(), ui.available_height()), |ui| {
                self.render_preview_panel(ui);
            });
        });
    }

    fn render_file_list(&mut self, ui: &mut egui::Ui) {
        // Breadcrumb & Filter Bar
        ui.horizontal(|ui| {
            // Root button
            if ui.button(RichText::new("🏠 /").color(VpColors::PINK)).clicked() {
                self.is_busy = true;
                self.busy_message = "Navigating to root...".to_string();
                let _ = self.cmd_tx.send(GuiCommand::NavigateTo { path: String::new() });
            }

            // Path segments
            let mut accum_path = String::new();
            for (idx, seg) in self.path_breadcrumbs.clone().iter().enumerate() {
                if idx > 0 {
                    accum_path.push('/');
                }
                accum_path.push_str(seg);
                let target_path = accum_path.clone();

                if ui.button(RichText::new(format!("{}/", seg)).color(VpColors::CYAN)).clicked() {
                    self.is_busy = true;
                    self.busy_message = format!("Navigating to /{}...", target_path);
                    let _ = self.cmd_tx.send(GuiCommand::NavigateTo { path: target_path });
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_sized(
                    [160.0, 22.0],
                    egui::TextEdit::singleline(&mut self.filter_input)
                        .hint_text("🔍 Filter files..."),
                );
            });
        });

        ui.separator();

        // Items list
        egui::ScrollArea::vertical().show(ui, |ui| {
            let filter = self.filter_input.to_lowercase();
            let mut navigate_to: Option<String> = None;
            let mut preview_item: Option<RepoItem> = None;

            for item in &self.items {
                if !filter.is_empty() && !item.name.to_lowercase().contains(&filter) {
                    continue;
                }

                let is_selected = self.selected_paths.contains(&item.path);

                ui.horizontal(|ui| {
                    // Checkbox for batch download
                    let mut checked = is_selected;
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked {
                            self.selected_paths.insert(item.path.clone());
                        } else {
                            self.selected_paths.remove(&item.path);
                        }
                    }

                    // Icon & Name in vaporwave pastel neon
                    let (icon, color) = if item.is_dir() {
                        ("📁", VpColors::CYAN)
                    } else if item.is_lfs() {
                        ("📦", VpColors::YELLOW)
                    } else {
                        ("📄", VpColors::TEXT_PRIMARY)
                    };

                    ui.label(RichText::new(icon).size(14.0));

                    let name_btn = ui.link(RichText::new(&item.name).size(13.0).color(color));
                    if name_btn.clicked() {
                        if item.is_dir() {
                            navigate_to = Some(item.path.clone());
                        } else {
                            preview_item = Some(item.clone());
                        }
                    }

                    // Right side: Size badge and direct download button
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("⬇").size(12.0).color(VpColors::PINK)).clicked() {
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
                            ui.label(RichText::new(Self::format_bytes(size)).size(11.0).monospace().color(VpColors::MINT));
                        } else if item.is_dir() {
                            ui.label(RichText::new("folder").size(11.0).color(VpColors::TEXT_MUTED));
                        }
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
                self.preview_loading = true;
                let _ = self.cmd_tx.send(GuiCommand::FetchFilePreview { item });
            }
        });
    }

    fn render_preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("❖ SOURCE PREVIEW").size(14.0).strong().color(VpColors::CYAN));

            if let Some(item) = &self.selected_preview_item {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let download_btn = ui.add(
                        egui::Button::new(
                            RichText::new("▼ DOWNLOAD THIS FILE")
                                .size(11.0)
                                .strong()
                                .color(VpColors::TEXT_DARK),
                        )
                        .fill(VpColors::PINK),
                    );

                    if download_btn.clicked() {
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
                        ui.label(RichText::new(Self::format_bytes(size)).size(11.0).monospace().color(VpColors::MINT));
                    }
                });
            }
        });

        ui.separator();

        if let Some(item) = &self.selected_preview_item {
            ui.label(RichText::new(&item.path).size(12.0).monospace().color(VpColors::PINK));

            if self.preview_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Reading file...").color(VpColors::YELLOW));
                });
            } else if let Some(content) = &self.preview_content {
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Frame::new()
                        .fill(VpColors::BG_PREVIEW)
                        .stroke(Stroke::new(1.0, VpColors::BORDER))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut content.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .text_color(VpColors::TEXT_PRIMARY)
                                    .code_editor()
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new("⚡").size(36.0).color(VpColors::PINK));
                ui.label(RichText::new("SELECT A FILE TO INSPECT CODE").size(13.0).color(VpColors::TEXT_MUTED));
            });
        }
    }

    fn render_releases_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("❖ RELEASES & BINARY ASSETS").size(16.0).strong().color(VpColors::PINK));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("🔄 Refresh").color(VpColors::CYAN)).clicked() {
                    self.is_busy = true;
                    self.busy_message = "Fetching releases...".to_string();
                    let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                }
            });
        });

        ui.separator();

        if self.releases.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new("🚀").size(36.0).color(VpColors::CYAN));
                ui.label(RichText::new("No releases cached. Load a repository first.").color(VpColors::TEXT_MUTED));
                if ui.button(RichText::new("FETCH RELEASES NOW").color(VpColors::PINK)).clicked() {
                    self.is_busy = true;
                    self.busy_message = "Fetching releases...".to_string();
                    let _ = self.cmd_tx.send(GuiCommand::FetchReleases);
                }
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut download_asset: Option<GitHubReleaseAsset> = None;

            for release in &self.releases {
                egui::Frame::new()
                    .fill(VpColors::BG_CARD)
                    .stroke(Stroke::new(1.0, VpColors::BORDER))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&release.tag_name).size(15.0).strong().color(VpColors::CYAN));
                            if release.prerelease {
                                ui.label(RichText::new("[Pre-release]").size(11.0).color(VpColors::YELLOW));
                            }
                            if release.draft {
                                ui.label(RichText::new("[Draft]").size(11.0).color(VpColors::TEXT_MUTED));
                            }
                        });

                        ui.add_space(6.0);

                        for asset in &release.assets {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📦").size(13.0));
                                ui.label(RichText::new(&asset.name).size(13.0).strong().color(VpColors::TEXT_PRIMARY));
                                ui.label(RichText::new(format!("({})", Self::format_bytes(asset.size))).size(11.0).monospace().color(VpColors::MINT));

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new("▼ Grab Asset").color(VpColors::PINK)).clicked() {
                                        download_asset = Some(asset.clone());
                                    }
                                });
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

    fn render_search_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("❖ REPOSITORY DISCOVERY").size(16.0).strong().color(VpColors::CYAN));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let search_edit = ui.add_sized(
                [ui.available_width() - 130.0, 26.0],
                egui::TextEdit::singleline(&mut self.search_query_input)
                    .hint_text("Type keyword (e.g. ratatui, tokio, tui, rust-cli)..."),
            );

            let enter_pressed = search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let search_btn = ui.add(
                egui::Button::new(
                    RichText::new("🔍 SEARCH")
                        .strong()
                        .color(VpColors::TEXT_DARK),
                )
                .fill(VpColors::PINK),
            );

            if (search_btn.clicked() || enter_pressed) && !self.search_query_input.trim().is_empty() {
                self.is_busy = true;
                self.busy_message = format!("Searching '{}'...", self.search_query_input);
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
                    .fill(VpColors::BG_CARD)
                    .stroke(Stroke::new(1.0, VpColors::BORDER))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&item.full_name).size(14.0).strong().color(VpColors::CYAN));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(RichText::new("⚡ Open Repo").color(VpColors::PINK)).clicked() {
                                    load_repo_url = Some(item.html_url.clone());
                                }
                                ui.label(RichText::new(format!("★ {}", item.stargazers_count)).size(12.0).color(VpColors::YELLOW));
                                if let Some(lang) = &item.language {
                                    ui.label(RichText::new(lang).size(11.0).color(VpColors::MINT));
                                }
                            });
                        });

                        if let Some(desc) = &item.description {
                            ui.add_space(4.0);
                            ui.label(RichText::new(desc).size(12.0).color(VpColors::TEXT_SECONDARY));
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

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("❖ SETTINGS // AUTHENTICATION").size(16.0).strong().color(VpColors::PINK));
        ui.add_space(10.0);

        egui::Frame::new()
            .fill(VpColors::BG_CARD)
            .stroke(Stroke::new(1.0, VpColors::BORDER))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.label(RichText::new("GitHub Personal Access Token").size(13.0).strong().color(VpColors::CYAN));
                ui.label(RichText::new("Using a token increases rate limit from 60 to 5,000 requests/hr and enables private repository access.").size(11.0).color(VpColors::TEXT_MUTED));

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.add_sized(
                        [ui.available_width() - 170.0, 26.0],
                        egui::TextEdit::singleline(&mut self.token_input)
                            .password(true)
                            .hint_text("ghp_... or gho_..."),
                    );

                    if ui.button(RichText::new("🔑 Import from `gh`").color(VpColors::CYAN)).clicked() {
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

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                ui.label(RichText::new("Default Download Directory").size(13.0).strong().color(VpColors::CYAN));
                ui.label(RichText::new(self.dest_path.to_string_lossy()).size(11.0).monospace().color(VpColors::MINT));

                ui.add_space(6.0);

                if ui.button(RichText::new("📂 Change Directory...").color(VpColors::PURPLE)).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().set_directory(&self.dest_path).pick_folder() {
                        self.dest_path = folder;
                    }
                }
            });

        ui.add_space(16.0);

        // Download activity log styled like a retro synth terminal
        ui.heading(RichText::new("❖ ACTIVITY CONSOLE").size(13.0).strong().color(VpColors::CYAN));
        egui::Frame::new()
            .fill(VpColors::BG_PREVIEW)
            .stroke(Stroke::new(1.0, VpColors::BORDER))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    if self.download_logs.is_empty() {
                        ui.label(RichText::new("// no network downloads logged yet //").size(11.0).color(VpColors::TEXT_MUTED));
                    } else {
                        for log in self.download_logs.iter().rev() {
                            ui.label(RichText::new(format!("> {}", log)).size(11.0).monospace().color(VpColors::MINT));
                        }
                    }
                });
            });
    }
}

impl eframe::App for GhGrabGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_events();

        // Enforce dark vaporwave visuals on every frame so OS light mode can NEVER make it white!
        let mut visuals = egui::Visuals::dark();
        visuals.dark_mode = true;
        visuals.panel_fill = VpColors::BG_BASE;
        visuals.window_fill = VpColors::BG_BASE;
        visuals.extreme_bg_color = VpColors::BG_INPUT;
        visuals.faint_bg_color = VpColors::BG_CARD;
        visuals.code_bg_color = VpColors::BG_PREVIEW;
        visuals.hyperlink_color = VpColors::CYAN;
        visuals.warn_fg_color = VpColors::YELLOW;
        visuals.error_fg_color = VpColors::PINK;
        visuals.override_text_color = Some(VpColors::TEXT_PRIMARY);
        ui.ctx().set_visuals(visuals);

        // Top Header and Navigation Bar with explicit dark frame
        egui::Panel::top("top_panel")
            .frame(egui::Frame::new().fill(VpColors::BG_TOP).stroke(Stroke::new(1.0, VpColors::BORDER)))
            .show(ui, |ui| {
                self.render_top_bar(ui);
            });

        // Left sidebar for download config & actions with explicit dark frame
        egui::Panel::left("left_sidebar")
            .default_size(290.0)
            .min_size(250.0)
            .max_size(360.0)
            .frame(egui::Frame::new().fill(VpColors::BG_SIDEBAR).stroke(Stroke::new(1.0, VpColors::BORDER)))
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });

        // Main content area with explicit dark background frame
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(VpColors::BG_BASE).inner_margin(Margin::same(12)))
            .show(ui, |ui| {
                match self.active_tab {
                    AppTab::Browser => self.render_browser_tab(ui),
                    AppTab::Releases => self.render_releases_tab(ui),
                    AppTab::Search => self.render_search_tab(ui),
                    AppTab::Settings => self.render_settings_tab(ui),
                }
            });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([800.0, 550.0])
            .with_title("Git Hub Grab GUI Vaporwave"),
        ..Default::default()
    };

    eframe::run_native(
        "Git Hub Grab GUI Vaporwave",
        options,
        Box::new(|cc| Ok(Box::new(GhGrabGuiApp::new(cc)))),
    )
}
