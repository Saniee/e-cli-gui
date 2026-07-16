#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use backend::{DownloadSettings, JobKind, Progress, ZipEvent};
use e_cli::cli::ArchiveFormat;
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
            Ok(Box::<App>::default())
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

    api_source: String,
    username: String,
    api_key: String,
    tags: String,
    count: u32,
    pages: i64,
    threads: usize,
    random: bool,
    lower_quality: bool,
    open_folder_after: bool,

    pool_id: String,
    zip_name: String,
    zip_format: ArchiveFormat,

    job: Option<ActiveJob>,
    zip_job: Option<ActiveZip>,

    pending_toasts: Vec<(String, ToastKind)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tab: Tab::Favourites,
            api_source: "e926.net".to_string(),
            username: String::new(),
            api_key: String::new(),
            tags: String::new(),
            count: 75,
            pages: -1,
            threads: 5,
            random: false,
            lower_quality: false,
            open_folder_after: false,
            pool_id: String::new(),
            zip_name: String::new(),
            zip_format: ArchiveFormat::Cbz,
            job: None,
            zip_job: None,
            pending_toasts: Vec::new(),
        }
    }
}

impl App {
    fn download_settings(&self) -> DownloadSettings {
        DownloadSettings {
            api_source: self.api_source.clone(),
            username: self.username.clone(),
            api_key: self.api_key.clone(),
            tags: self.tags.clone(),
            count: self.count,
            pages: self.pages,
            threads: self.threads,
            random: self.random,
            lower_quality: self.lower_quality,
        }
    }

    fn start_job(&mut self, kind: JobKind, label: &'static str) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        backend::spawn_download(
            kind,
            self.download_settings(),
            PathBuf::from(DL_DIR),
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
                            "Finished! {} downloaded, {} failed (of {}).",
                            stats.completed, stats.failed, stats.total
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
                open_dl_dir();
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
        });

        toasts.show(ctx);
    }
}

impl App {
    fn shared_settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Connection settings")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("API source");
                    ui.text_edit_singleline(&mut self.api_source);
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
                ui.add(egui::Slider::new(&mut self.threads, 1..=10).text("Threads"));
                ui.checkbox(&mut self.random, "Random order");
                ui.checkbox(&mut self.lower_quality, "Prefer lower quality");
                ui.checkbox(&mut self.open_folder_after, "Open ./dl when finished");
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
        tags_edit(ui, &mut self.tags);
        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut self.count, 1..=250).text("Posts per page"));
        ui.add(egui::Slider::new(&mut self.pages, -1..=75).text("Pages (-1 = all)"));
        ui.add_space(10.0);
        self.shared_settings_ui(ui);

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
        tags_edit(ui, &mut self.tags);
        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut self.count, 1..=250).text("Posts per page"));
        ui.add(egui::Slider::new(&mut self.pages, -1..=75).text("Pages (-1 = all)"));
        ui.add_space(10.0);
        self.shared_settings_ui(ui);

        let busy = self.job.is_some();
        ui.add_enabled_ui(!busy && !self.tags.trim().is_empty(), |ui| {
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
        self.shared_settings_ui(ui);

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
                backend::spawn_zip(PathBuf::from(DL_DIR), self.zip_name.clone(), self.zip_format, tx);
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

        if ui.button("Open ./dl folder").clicked() {
            if Path::new(DL_DIR).exists() {
                open_dl_dir();
            } else {
                self.toast("No ./dl folder found.", ToastKind::Error);
            }
        }

        ui.add_space(6.0);
        let cleanup_style = egui::Button::new("Cleanup (trash ./dl)").fill(Color32::from_rgb(125, 0, 0));
        if ui.add(cleanup_style).clicked() {
            if Path::new(DL_DIR).exists() {
                let _ = trash::delete(DL_DIR);
                self.toast("Cleaned up!", ToastKind::Info);
            } else {
                self.toast("No ./dl folder found.", ToastKind::Error);
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

fn open_dl_dir() {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(DL_DIR.replace('/', "\\")).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(DL_DIR).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(DL_DIR).spawn();
}
