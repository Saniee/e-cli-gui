#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(rustdoc::missing_crate_level_docs)]

use std::{
    fs,
    path::Path,
    sync::mpsc::{Receiver, Sender},
};

use commands::download_favourites;
use eframe::egui;
use egui::Align2;
use egui_toast::{Toast, ToastOptions, Toasts};
use funcs::open_dl_dir;
use tokio::runtime::Runtime;

mod commands;
mod funcs;
mod type_defs;

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
    // Sender/Reciver for async stuff.
    tx: Sender<u64>,
    rx: Receiver<u64>,

    // Other vars
    tags: String,
    username: String,
    post_amount: i32,
    random: bool,
    lower_quality: bool,
    api_source: String,
    dl_count: u64,
    open_folder: bool,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        Self {
            tx,
            rx,
            tags: String::new(),
            username: String::new(),
            post_amount: 5,
            random: false,
            lower_quality: false,
            api_source: "e926.net".to_string(),
            dl_count: 0,
            open_folder: true,
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

        let mut toasts = Toasts::new()
            .anchor(Align2::CENTER_BOTTOM, (0.0, -8.0))
            .direction(egui::Direction::BottomUp);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let api_source_label = ui.label("Api Source");
                ui.text_edit_singleline(&mut self.api_source)
                    .labelled_by(api_source_label.id);

                let username_label = ui.label("Username");
                ui.text_edit_singleline(&mut self.username)
                    .labelled_by(username_label.id);

                let tags_label = ui.label("Tags");
                ui.text_edit_multiline(&mut self.tags)
                    .labelled_by(tags_label.id);

                ui.checkbox(&mut self.random, "Get Random Posts?");
                ui.checkbox(&mut self.lower_quality, "Get lower quality of posts?");
                ui.checkbox(
                    &mut self.open_folder,
                    "Open /dl/ folder at download finish?",
                )
            });
            ui.add_space(5.0);
            let slider = ui.add(
                egui::Slider::new(&mut self.post_amount, 1..=250)
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
                        kind: egui_toast::ToastKind::Error,
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
                        kind: egui_toast::ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                } else if clear_dl_btn.clicked() {
                    toasts.add(Toast {
                        text: "No Folder found!".into(),
                        kind: egui_toast::ToastKind::Error,
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                };
                ui.add_space(5.0);
                if ui.button("Download Favourites").clicked() {
                    toasts.add(Toast {
                        kind: egui_toast::ToastKind::Info,
                        text: "Starting Download...".into(),
                        options: ToastOptions::default()
                            .duration_in_seconds(5.0)
                            .show_progress(true),
                    });
                    dl_favs(
                        self.username.clone(),
                        self.post_amount,
                        self.random,
                        self.tags.clone(),
                        self.lower_quality,
                        self.api_source.clone(),
                        self.open_folder,
                        self.tx.clone(),
                        ctx.clone(),
                    )
                }
                ui.add_space(5.0);
                ui.add_enabled(false, egui::Button::new("Download Posts with Tags"));

                ui.add_space(10.0);
                if self.dl_count > 0 {
                    ui.spinner();

                    ui.label(format!(
                        "Downloading... ({}/{})",
                        self.dl_count, self.post_amount
                    ));

                    ui.add_space(10.0);
                }
            });
            toasts.show(ctx);
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn dl_favs(
    username: String,
    count: i32,
    random: bool,
    tags: String,
    lower_quality: bool,
    api_source: String,
    open_dl_folder: bool,
    tx: Sender<u64>,
    ctx: egui::Context,
) {
    tokio::spawn(async move {
        download_favourites(
            username,
            count,
            random,
            tags,
            lower_quality,
            api_source,
            tx.clone(),
            ctx.clone(),
        )
        .await;

        let _ = tx.send(0);

        if open_dl_folder {
            open_dl_dir()
        }
    });
}
