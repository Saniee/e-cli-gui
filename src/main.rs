#![allow(rustdoc::missing_crate_level_docs)]

use e_handler::EHandler;
use eframe::egui;
use egui::Align2;
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use std::{
    fs,
    path::Path,
    sync::mpsc::{Receiver, Sender},
};
use tokio::runtime::Runtime;
use util_lib::open_dl_dir;

mod e_handler;
mod type_defs;
mod util_lib;

fn main() {
    let rt = Runtime::new().expect("Unable to create Runtime.");

    let _enter = rt.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_resizable(false)
            .with_inner_size([300.0, 500.0]),
        ..Default::default()
    };

    let _ = eframe::run_native("E-CLI GUI", options, Box::new(|_| Box::<App>::default()));
}

struct App {
    data: e_handler::EHandler,
    rx: Receiver<u64>,
    dl_count: u64,
    open_folder: bool,
    downloading_status: bool,
    dl_status_rx: Receiver<bool>,
    dl_status_tx: Sender<bool>,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (dl_status_tx, dl_status_rx) = std::sync::mpsc::channel();

        let mut data = e_handler::EHandler::default();
        data.define_sender(tx);

        Self {
            data,
            rx,
            dl_count: 0,
            open_folder: true,
            downloading_status: false,
            dl_status_rx,
            dl_status_tx,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(value) = self.rx.try_recv() {
            if value < 1 {
                self.dl_count = value;
            }
            self.dl_count += value;
        }

        if let Ok(value) = self.dl_status_rx.try_recv() {
            self.downloading_status = value
        }

        self.data.define_gui(ctx.clone());

        let mut toasts = Toasts::new()
            .anchor(Align2::CENTER_BOTTOM, (0.0, -8.0))
            .direction(egui::Direction::BottomUp);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let api_source_label = ui.label("Api Source");
                ui.text_edit_singleline(&mut self.data.api_source)
                    .labelled_by(api_source_label.id);

                let username_label = ui.label("Username");
                ui.text_edit_singleline(&mut self.data.username)
                    .labelled_by(username_label.id);

                let tags_label = ui.label("Tags");
                ui.text_edit_multiline(&mut self.data.tags)
                    .labelled_by(tags_label.id);

                ui.checkbox(&mut self.data.random, "Get Random Posts?");
                ui.checkbox(&mut self.data.lower_quality, "Get lower quality of posts?");
                ui.checkbox(
                    &mut self.open_folder,
                    "Open /dl/ folder at download finish?",
                )
            });
            ui.add_space(5.0);
            let slider = ui.add(
                egui::Slider::new(&mut self.data.count, 1..=250)
                    .prefix("Download: ")
                    .text("Posts."),
            );
            slider.on_hover_text("Amount of posts to download.");

            ui.add_space(15.0);
            ui.vertical_centered(|ui| {
                let open_dl_btn = ui.button("Open /dl/");
                if open_dl_btn.clicked() && Path::new("./dl").exists() {
                    open_dl_dir()
                } else if open_dl_btn.clicked() {
                    toasts.add(Toast {
                        text: "No Folder found!".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                }
                let clear_dl_btn = ui.button("Clear /dl/ folder");
                if clear_dl_btn.clicked() && Path::new("./dl").exists() {
                    fs::remove_dir_all("./dl").expect("Couldn't Remove directory.");
                    toasts.add(Toast {
                        text: "Cleared /dl/".into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                } else if clear_dl_btn.clicked() {
                    toasts.add(Toast {
                        text: "No Folder found!".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                };
                ui.add_space(5.0);
                if ui.button("Download Favourites").clicked() {
                    toasts.add(Toast {
                        kind: ToastKind::Info,
                        text: "Starting Download...".into(),
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                    dl_favs_btn(
                        self.data.clone(),
                        self.data.clone().tx.unwrap().clone(),
                        self.open_folder,
                        self.dl_status_tx.clone(),
                    )
                }
                ui.add_space(5.0);
                if ui.button("Download Posts with Tags").clicked() {
                    toasts.add(Toast {
                        kind: ToastKind::Info,
                        text: "Starting Download...".into(),
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                    dl_tags_btn(
                        self.data.clone(),
                        self.data.clone().tx.unwrap().clone(),
                        self.open_folder,
                        self.dl_status_tx.clone(),
                    )
                }

                ui.add_space(10.0);
                if self.downloading_status {
                    ui.spinner();

                    ui.label(format!(
                        "Downloading... ({}/{})",
                        self.dl_count, self.data.count
                    ));

                    ui.add_space(10.0);
                }
            });
            toasts.show(ctx);
        });
    }
}

fn dl_favs_btn(data: EHandler, tx: Sender<u64>, open_dl_folder: bool, dl_status_tx: Sender<bool>) {
    tokio::spawn(async move {
        let _ = dl_status_tx.send(true);
        data.download_favourites().await;

        let _ = tx.send(0);
        let _ = dl_status_tx.send(false);

        if open_dl_folder {
            open_dl_dir();
        }
    });
}

fn dl_tags_btn(data: EHandler, tx: Sender<u64>, open_dl_folder: bool, dl_status_tx: Sender<bool>) {
    tokio::spawn(async move {
        let _ = dl_status_tx.send(true);

        data.download_with_tags().await;

        let _ = tx.send(0);
        let _ = dl_status_tx.send(false);

        if open_dl_folder {
            open_dl_dir();
        }
    });
}
