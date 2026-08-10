#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use backend::{DownloadSettings, JobKind, Progress, ZipEvent};
use e_cli::cli::ArchiveFormat;
use e_cli::config as econfig;
use e_cli::update;
use eframe::egui;
use egui::{Align2, Color32, RichText};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};

const DL_DIR: &str = "./dl";
const KEY_FILE: &str = "./key";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "E-CLI GUI",
        options,
        Box::new(|cc| {
            egui_extras_setup(&cc.egui_ctx);
            let mut app = App::default();
            app.load_settings();
            app.spawn_version_check(cc.egui_ctx.clone());
            Ok(Box::new(app))
        }),
    )
}

fn egui_extras_setup(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Favourites,
    Tags,
    Pool,
    Utilities,
    Config,
}

struct ActiveJob {
    label: &'static str,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Progress>,
    completed: u64,
    total: Option<u64>,
}

struct ActiveZip {
    rx: Receiver<ZipEvent>,
}

struct App {
    tab: Tab,

    nsfw: bool,
    username: String,
    api_key: String,
    fav_tags: String,
    fav_count: u32,
    fav_random: bool,
    search_tags: String,
    search_count: u32,
    search_random: bool,
    pages: i64,
    threads: usize,
    lower_quality: bool,
    open_folder_after: bool,
    track_file: String,
    dl_dir: String,

    pool_id: String,
    zip_name: String,
    zip_format: ArchiveFormat,

    config: econfig::Config,

    job: Option<ActiveJob>,
    zip_job: Option<ActiveZip>,

    version_check_rx: Option<Receiver<Result<Option<String>, String>>>,

    pending_toasts: Vec<(String, ToastKind)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tab: Tab::Favourites,
            nsfw: false,
            username: String::new(),
            api_key: String::new(),
            fav_tags: String::new(),
            fav_count: 75,
            fav_random: false,
            search_tags: String::new(),
            search_count: 75,
            search_random: false,
            pages: -1,
            threads: 5,
            lower_quality: false,
            open_folder_after: false,
            track_file: String::new(),
            dl_dir: DL_DIR.to_string(),
            pool_id: String::new(),
            zip_name: String::new(),
            zip_format: ArchiveFormat::Cbz,
            config: econfig::Config::default(),
            job: None,
            zip_job: None,
            version_check_rx: None,
            pending_toasts: Vec::new(),
        }
    }
}

impl App {
    fn start_job(&mut self, kind: JobKind, label: &'static str) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tags, count, random) = match &kind {
            JobKind::Favourites => (self.fav_tags.clone(), self.fav_count, self.fav_random),
            JobKind::Tags => (self.search_tags.clone(), self.search_count, self.search_random),
            JobKind::Pool(_) => (String::new(), 0, false),
        };
        let settings = DownloadSettings {
            nsfw: self.nsfw,
            username: self.username.clone(),
            api_key: self.api_key.clone(),
            tags,
            count,
            pages: self.pages,
            threads: self.threads,
            random,
            lower_quality: self.lower_quality,
            track_file: self.track_file.clone(),
        };
        backend::spawn_download(
            kind,
            settings,
            PathBuf::from(&self.dl_dir),
            cancel.clone(),
            tx,
        );
        self.job = Some(ActiveJob {
            label,
            cancel,
            rx,
            completed: 0,
            total: None,
        });
        self.toast(format!("Starting {label} download..."), ToastKind::Info);
    }

    fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.pending_toasts.push((text.into(), kind));
    }

    fn spawn_version_check(&mut self, ctx: egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result =
                update::check_update("Saniee/e-cli-gui", env!("CARGO_PKG_VERSION"));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.version_check_rx = Some(rx);
    }

    fn poll_version_check(&mut self) {
        let Some(rx) = self.version_check_rx.take() else { return };
        match rx.try_recv() {
            Ok(result) => match result {
                Ok(Some(latest)) => self.toast(
                    format!(
                        "New version v{latest} available! See \
                         https://github.com/Saniee/e-cli-gui/releases/tag/v{latest}",
                    ),
                    ToastKind::Info,
                ),
                Ok(None) => {}
                Err(e) => self.toast(
                    format!("Could not check for updates: {e}"),
                    ToastKind::Warning,
                ),
            },
            Err(_) => self.version_check_rx = Some(rx),
        }
    }

    fn load_settings(&mut self) {
        let cfg = match econfig::path().and_then(|p| econfig::load(&p)) {
            Ok(cfg) => cfg,
            Err(e) => {
                self.toast(format!("Could not load config: {e}"), ToastKind::Warning);
                return;
            }
        };
        self.apply_config(&cfg);
        self.config = cfg;
    }

    fn apply_config(&mut self, cfg: &econfig::Config) {
        if let Some(v) = cfg.global.nsfw {
            self.nsfw = v;
        }
        if let Some(v) = cfg.global.pages {
            self.pages = v;
        }
        if let Some(v) = cfg.global.num_threads {
            self.threads = v;
        }
        if let Some(v) = cfg.global.lower_quality {
            self.lower_quality = v;
        }
        if let Some(v) = cfg.global.dir.as_deref() {
            self.dl_dir = v.to_owned();
        }
        if let Some(v) = cfg.global.track_file.as_deref() {
            self.track_file = v.to_string_lossy().to_string();
        }
        if let Some(v) = cfg.d_favs.username.as_deref() {
            self.username = v.to_owned();
        }
        if let Some(v) = cfg.d_favs.tags.as_deref() {
            self.fav_tags = v.to_owned();
        }
        if let Some(v) = cfg.d_favs.count {
            self.fav_count = v;
        }
        if let Some(v) = cfg.d_favs.random {
            self.fav_random = v;
        }
        if let Some(v) = cfg.d_tags.tags.as_deref() {
            self.search_tags = v.to_owned();
        }
        if let Some(v) = cfg.d_tags.count {
            self.search_count = v;
        }
        if let Some(v) = cfg.d_tags.random {
            self.search_random = v;
        }
        if let Some(v) = cfg.d_pool.pool_id {
            self.pool_id = v.to_string();
        }
        if let Some(v) = cfg.zip.name.as_deref() {
            self.zip_name = v.to_owned();
        }
        if let Some(v) = cfg.zip.format.as_deref() {
            self.zip_format = match v {
                "zip" => ArchiveFormat::Zip,
                "7z" => ArchiveFormat::SevenZip,
                _ => ArchiveFormat::Cbz,
            };
        }
    }

    fn finish_save(&mut self, changed: bool) {
        if !changed {
            self.toast("No changes to save.", ToastKind::Info);
            return;
        }
        match econfig::save(&self.config) {
            Ok(()) => self.toast("Settings saved to config.toml.", ToastKind::Success),
            Err(e) => self.toast(format!("Could not save config: {e}"), ToastKind::Error),
        }
    }

    fn save_global(&mut self) {
        let cfg = &mut self.config;
        let mut changed = false;
        if Some(self.nsfw) != cfg.global.nsfw {
            cfg.global.nsfw = Some(self.nsfw);
            changed = true;
        }
        if Some(self.pages) != cfg.global.pages {
            cfg.global.pages = Some(self.pages);
            changed = true;
        }
        if Some(self.threads) != cfg.global.num_threads {
            cfg.global.num_threads = Some(self.threads);
            changed = true;
        }
        if Some(self.lower_quality) != cfg.global.lower_quality {
            cfg.global.lower_quality = Some(self.lower_quality);
            changed = true;
        }
        if Some(self.dl_dir.clone()) != cfg.global.dir {
            cfg.global.dir = Some(self.dl_dir.clone());
            changed = true;
        }
        let track = if self.track_file.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.track_file.trim()))
        };
        if track != cfg.global.track_file {
            cfg.global.track_file = track;
            changed = true;
        }
        self.finish_save(changed);
    }

    fn save_favourites(&mut self) {
        let cfg = &mut self.config;
        let mut changed = false;
        let username = if self.username.trim().is_empty() {
            None
        } else {
            Some(self.username.trim().to_owned())
        };
        if username != cfg.d_favs.username {
            cfg.d_favs.username = username;
            changed = true;
        }
        if Some(self.fav_tags.clone()) != cfg.d_favs.tags {
            cfg.d_favs.tags = Some(self.fav_tags.clone());
            changed = true;
        }
        if Some(self.fav_count) != cfg.d_favs.count {
            cfg.d_favs.count = Some(self.fav_count);
            changed = true;
        }
        if Some(self.fav_random) != cfg.d_favs.random {
            cfg.d_favs.random = Some(self.fav_random);
            changed = true;
        }
        self.finish_save(changed);
    }

    fn save_tags(&mut self) {
        let cfg = &mut self.config;
        let mut changed = false;
        if Some(self.search_tags.clone()) != cfg.d_tags.tags {
            cfg.d_tags.tags = Some(self.search_tags.clone());
            changed = true;
        }
        if Some(self.search_count) != cfg.d_tags.count {
            cfg.d_tags.count = Some(self.search_count);
            changed = true;
        }
        if Some(self.search_random) != cfg.d_tags.random {
            cfg.d_tags.random = Some(self.search_random);
            changed = true;
        }
        self.finish_save(changed);
    }

    fn save_pool(&mut self) {
        let cfg = &mut self.config;
        let mut changed = false;
        let pool_id = self.pool_id.trim().parse().ok();
        if pool_id != cfg.d_pool.pool_id {
            cfg.d_pool.pool_id = pool_id;
            changed = true;
        }
        let name = if self.zip_name.trim().is_empty() {
            None
        } else {
            Some(self.zip_name.trim().to_owned())
        };
        if name != cfg.zip.name {
            cfg.zip.name = name;
            changed = true;
        }
        let format = Some(match self.zip_format {
            ArchiveFormat::Zip => "zip".to_owned(),
            ArchiveFormat::SevenZip => "7z".to_owned(),
            ArchiveFormat::Cbz => "cbz".to_owned(),
        });
        if format != cfg.zip.format {
            cfg.zip.format = format;
            changed = true;
        }
        self.finish_save(changed);
    }

    fn open_config(&mut self) {
        if let Err(e) = econfig::open() {
            self.toast(format!("Could not open config: {e}"), ToastKind::Error);
        }
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.job else { return };
        let mut finished_msg: Option<(String, ToastKind)> = None;

        while let Ok(progress) = job.rx.try_recv() {
            match progress {
                Progress::Total(total) => job.total = Some(total),
                Progress::Tick => job.completed += 1,
                Progress::Finished(stats) => {
                    finished_msg = Some((
                        format!(
                            "Finished! {} downloaded, {} skipped, {} failed (of {}).",
                            stats.completed, stats.skipped, stats.failed, stats.total
                        ),
                        ToastKind::Success,
                    ));
                }
                Progress::Error(err) => {
                    finished_msg = Some((err, ToastKind::Error));
                }
            }
            ctx.request_repaint();
        }

        if let Some((text, kind)) = finished_msg {
            self.toast(text, kind);
            let open_folder = self.open_folder_after;
            self.job = None;
            if open_folder {
                open_dl_dir(&self.dl_dir);
            }
        }
    }

    fn poll_zip(&mut self) {
        let Some(zip_job) = &self.zip_job else { return };
        if let Ok(ZipEvent::Finished(ok)) = zip_job.rx.try_recv() {
            if ok {
                self.toast("Archive created!", ToastKind::Success);
            } else {
                self.toast("Failed to create archive. Is 7z on PATH?", ToastKind::Error);
            }
            self.zip_job = None;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_job(ctx);
        self.poll_zip();
        self.poll_version_check();

        let mut toasts = Toasts::new()
            .anchor(Align2::CENTER_BOTTOM, (0.0, -8.0))
            .direction(egui::Direction::BottomUp);

        for (text, kind) in self.pending_toasts.drain(..) {
            toasts.add(Toast {
                text: text.into(),
                kind,
                options: ToastOptions::default()
                    .duration_in_seconds(2.5)
                    .show_progress(true),
                ..Default::default()
            });
        }

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Favourites, "Favourites");
                ui.selectable_value(&mut self.tab, Tab::Tags, "Tags");
                ui.selectable_value(&mut self.tab, Tab::Pool, "Pool");
                ui.selectable_value(&mut self.tab, Tab::Utilities, "Utilities");
                ui.selectable_value(&mut self.tab, Tab::Config, "Config");
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("progress").show(ctx, |ui| {
            ui.add_space(6.0);
            self.progress_ui(ui);
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Favourites => self.favourites_ui(ui),
            Tab::Tags => self.tags_ui(ui),
            Tab::Pool => self.pool_ui(ui),
            Tab::Utilities => self.utilities_ui(ui),
            Tab::Config => self.config_ui(ui),
        });

        toasts.show(ctx);
    }
}

impl App {
    fn config_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Global settings");
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Connection settings")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.nsfw, "NSFW API");
                    ui.label(
                        RichText::new(if self.nsfw { "e621.net" } else { "e926.net" }).weak(),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button("Load API key").clicked() {
                        match std::fs::read_to_string(KEY_FILE) {
                            Ok(key) if !key.trim().is_empty() => {
                                self.api_key = key.trim().to_string();
                                self.toast("API key loaded!", ToastKind::Success);
                            }
                            _ => self.toast("No 'key' file found next to the exe.", ToastKind::Warning),
                        }
                    }
                    if ui.button("Clear key").clicked() {
                        self.api_key.clear();
                        self.toast("API key cleared.", ToastKind::Info);
                    }
                    ui.label(if self.api_key.is_empty() { "Not logged in" } else { "Key loaded" });
                });
                ui.horizontal(|ui| {
                    ui.label("Track file");
                    ui.text_edit_singleline(&mut self.track_file);
                    ui.label(if self.track_file.trim().is_empty() {
                        "off"
                    } else {
                        "on"
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Download dir");
                    ui.text_edit_singleline(&mut self.dl_dir);
                });
                ui.add(egui::Slider::new(&mut self.threads, 1..=10).text("Threads"));
                ui.add(egui::Slider::new(&mut self.pages, -1..=75).text("Pages (-1 = all)"));
                ui.checkbox(&mut self.lower_quality, "Prefer lower quality");
                ui.checkbox(
                    &mut self.open_folder_after,
                    format!("Open {} when finished", self.dl_dir),
                );
                ui.horizontal(|ui| {
                    if ui.button("Save settings").clicked() {
                        self.save_global();
                    }
                });
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Open config").clicked() {
                self.open_config();
            }
            ui.label(
                RichText::new(
                    econfig::path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "config location unavailable".to_owned()),
                )
                .weak(),
            );
        });
        ui.add_space(8.0);
    }

    fn favourites_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Download Favourites");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Username");
            ui.text_edit_singleline(&mut self.username);
        });
        ui.add_space(6.0);
        tags_edit(ui, &mut self.fav_tags);
        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut self.fav_count, 1..=250).text("Posts per page"));
        ui.checkbox(&mut self.fav_random, "Random order");
        ui.add_space(10.0);

        if ui.button("Save settings").clicked() {
            self.save_favourites();
        }

        let busy = self.job.is_some();
        ui.add_enabled_ui(!busy && !self.username.trim().is_empty(), |ui| {
            if ui.button("Download Favourites").clicked() {
                self.start_job(JobKind::Favourites, "Favourites");
            }
        });
        if busy {
            self.stop_button(ui);
        }
    }

    fn tags_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Download by Tags");
        ui.add_space(8.0);
        tags_edit(ui, &mut self.search_tags);
        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut self.search_count, 1..=250).text("Posts per page"));
        ui.checkbox(&mut self.search_random, "Random order");
        ui.add_space(10.0);

        if ui.button("Save settings").clicked() {
            self.save_tags();
        }

        let busy = self.job.is_some();
        ui.add_enabled_ui(!busy && !self.search_tags.trim().is_empty(), |ui| {
            if ui.button("Download Posts").clicked() {
                self.start_job(JobKind::Tags, "Tags");
            }
        });
        if busy {
            self.stop_button(ui);
        }
    }

    fn pool_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Download a Pool");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Pool ID");
            ui.text_edit_singleline(&mut self.pool_id);
        });
        ui.add_space(10.0);

        if ui.button("Save settings").clicked() {
            self.save_pool();
        }

        let pool_id: Option<u64> = self.pool_id.trim().parse().ok();
        let busy = self.job.is_some();
        ui.add_enabled_ui(!busy && pool_id.is_some(), |ui| {
            if ui.button("Download Pool").clicked() {
                if let Some(id) = pool_id {
                    self.start_job(JobKind::Pool(id), "Pool");
                }
            }
        });
        if busy {
            self.stop_button(ui);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.label("Package the downloaded pool into an archive (files are numbered so reading order is preserved):");
        ui.horizontal(|ui| {
            ui.label("Archive name");
            ui.text_edit_singleline(&mut self.zip_name);
        });
        ui.horizontal(|ui| {
            ui.label("Format");
            egui::ComboBox::from_id_salt("zip_format")
                .selected_text(archive_format_label(self.zip_format))
                .show_ui(ui, |ui| {
                    for fmt in [ArchiveFormat::Cbz, ArchiveFormat::Zip, ArchiveFormat::SevenZip] {
                        ui.selectable_value(&mut self.zip_format, fmt, archive_format_label(fmt));
                    }
                });
        });
        ui.add_enabled_ui(self.zip_job.is_none() && !self.zip_name.trim().is_empty(), |ui| {
            if ui.button("Package into archive").clicked() {
                let (tx, rx) = std::sync::mpsc::channel();
                backend::spawn_zip(PathBuf::from(&self.dl_dir), self.zip_name.clone(), self.zip_format, tx);
                self.zip_job = Some(ActiveZip { rx });
                self.toast("Packaging archive (requires 7z on PATH)...", ToastKind::Info);
            }
        });
        if self.zip_job.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Packaging...");
            });
        }
    }

    fn utilities_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Utilities");
        ui.add_space(10.0);

        if ui.button(format!("Open {} folder", self.dl_dir)).clicked() {
            if Path::new(&self.dl_dir).exists() {
                open_dl_dir(&self.dl_dir);
            } else {
                self.toast(format!("No {} folder found.", self.dl_dir), ToastKind::Error);
            }
        }

        ui.add_space(6.0);
        let cleanup_style = egui::Button::new(format!("Cleanup (trash {})", self.dl_dir))
            .fill(Color32::from_rgb(125, 0, 0));
        if ui.add(cleanup_style).clicked() {
            if Path::new(&self.dl_dir).exists() {
                let _ = trash::delete(&self.dl_dir);
                self.toast("Cleaned up!", ToastKind::Info);
            } else {
                self.toast(format!("No {} folder found.", self.dl_dir), ToastKind::Error);
            }
        }

        ui.add_space(16.0);
        ui.label(RichText::new("The API key is read from a plain-text file called 'key' next to the executable.").weak());
    }

    fn stop_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Stop").clicked() {
            if let Some(job) = &self.job {
                job.cancel.store(true, Ordering::Relaxed);
            }
            self.toast("Stopping...", ToastKind::Warning);
        }
    }

    fn progress_ui(&mut self, ui: &mut egui::Ui) {
        let Some(job) = &self.job else {
            ui.label(RichText::new("Idle").weak());
            return;
        };

        ui.horizontal(|ui| {
            ui.label(format!("Downloading {}...", job.label));
        });

        match job.total {
            Some(total) if total > 0 => {
                let fraction = job.completed as f32 / total as f32;
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(format!("{}/{}", job.completed, total))
                        .animate(true),
                );
            }
            _ => {
                ui.add(egui::ProgressBar::new(0.0).animate(true));
            }
        }
    }
}

fn tags_edit(ui: &mut egui::Ui, tags: &mut String) {
    ui.label("Tags");
    ui.add(egui::TextEdit::multiline(tags).desired_rows(2).char_limit(250));
}

fn archive_format_label(fmt: ArchiveFormat) -> &'static str {
    match fmt {
        ArchiveFormat::Zip => "zip",
        ArchiveFormat::SevenZip => "7z",
        ArchiveFormat::Cbz => "cbz",
    }
}

fn open_dl_dir(dir: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(dir.replace('/', "\\")).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
}
