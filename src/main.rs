#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(rustdoc::missing_crate_level_docs)]

use e_handler::EHandler;
use eframe::egui;
use egui::{Align2, Color32};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use std::{path::Path, sync::mpsc::Sender};
use tokio::task::AbortHandle;
use util_lib::open_dl_dir;

mod channel_handler;
mod e_handler;
mod type_defs;
mod util_lib;

#[tokio::main]
async fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_resizable(false)
            .with_inner_size([300.0, 550.0])
            .with_maximize_button(false),
        ..Default::default()
    };

    let _ = eframe::run_native("E-CLI GUI", options, Box::new(|_| Box::<App>::default()));
}

struct App {
    data: e_handler::EHandler,
    channels: channel_handler::GuiChannels,
    dl_count: u64,
    open_folder: bool,
    downloading_status: bool,
    task_abort_handle: Option<AbortHandle>,
}

impl Default for App {
    fn default() -> Self {
        let channels = channel_handler::GuiChannels::default();

        let mut data = e_handler::EHandler::default();
        data.define_senders(channels.dl_count_channel.0.clone());

        Self {
            data,
            channels,
            dl_count: 0,
            open_folder: false,
            downloading_status: false,
            task_abort_handle: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(dl_count) = self.channels.dl_count_channel.1.try_recv() {
            if dl_count < 1 {
                self.dl_count = dl_count;
            }
            self.dl_count += dl_count;
        }

        if let Ok(value) = self.channels.dl_status_channel.1.try_recv() {
            self.downloading_status = value
        }

        self.data.define_gui(ctx.clone());

        let mut toasts = Toasts::new()
            .anchor(Align2::CENTER_BOTTOM, (0.0, -8.0))
            .direction(egui::Direction::BottomUp);

        if let Ok(finished) = self.channels.finished_status_channel.1.try_recv() {
            if finished {
                toasts.add(Toast {
                    kind: ToastKind::Info,
                    text: "Finished Downloading!".into(),
                    options: ToastOptions::default()
                        .duration_in_seconds(1.5)
                        .show_progress(true),
                });
                let _ = self.channels.finished_status_channel.0.send(false);
                self.task_abort_handle = None
            }
        }

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
            ui.add(
                egui::Slider::new(&mut self.data.count, 1..=250)
                    .prefix("Download: ")
                    .text("Posts."),
            );
            ui.add(
                egui::Slider::new(&mut self.data.pages, -1..=75)
                    .prefix("Bulk Get: ")
                    .text("Pages."),
            );

            ui.add_space(15.0);
            ui.vertical_centered(|ui| {
                let open_dl_btn = ui.button("Open the ./dl Folder with Explorer");
                if open_dl_btn.clicked() && Path::new("./dl").exists() {
                    open_dl_dir()
                } else if open_dl_btn.clicked() {
                    toasts.add(Toast {
                        text: "No Folder found!".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default()
                            .duration_in_seconds(1.5)
                            .show_progress(true),
                    });
                }
                let clear_dirs_btn_style =
                    egui::Button::new("Cleanup (Trash data/dl folder if exists)")
                        .fill(Color32::from_rgb(125, 0, 0));
                let clear_dirs_btn = ui.add(clear_dirs_btn_style);

                if clear_dirs_btn.clicked()
                    && (Path::new("./dl").exists() || Path::new("./data").exists())
                {
                    if Path::new("./dl").exists() {
                        let _ = trash::delete("./dl");
                    }
                    if Path::new("./data").exists() {
                        let _ = trash::delete("./data");
                    }

                    toasts.add(Toast {
                        text: "Cleaned Up!".into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(1.5)
                            .show_progress(true),
                    });
                } else if clear_dirs_btn.clicked() {
                    toasts.add(Toast {
                        text: "No Folder/s found!".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default()
                            .duration_in_seconds(1.5)
                            .show_progress(true),
                    });
                };
                ui.add_space(20.0);
                ui.label("Main Functions:");
                if ui.button("Download Favourites").clicked() {
                    if self.task_abort_handle.is_none() {
                        toasts.add(Toast {
                            kind: ToastKind::Info,
                            text: "Starting Download...".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });

                        self.task_abort_handle = Some(dl_favs_btn(
                            self.data.clone(),
                            self.open_folder,
                            self.channels.dl_count_channel.0.clone(),
                            self.channels.dl_status_channel.0.clone(),
                            self.channels.finished_status_channel.0.clone(),
                        ));
                    } else {
                        toasts.add(Toast {
                            kind: ToastKind::Warning,
                            text: "Cannot start a new download!".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });
                    }
                }
                ui.add_space(5.0);
                if ui.button("Download Posts with Tags").clicked() {
                    if self.task_abort_handle.is_none() {
                        toasts.add(Toast {
                            kind: ToastKind::Info,
                            text: "Starting Download...".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });

                        self.task_abort_handle = Some(dl_tags_btn(
                            self.data.clone(),
                            self.open_folder,
                            self.channels.dl_count_channel.0.clone(),
                            self.channels.dl_status_channel.0.clone(),
                            self.channels.finished_status_channel.0.clone(),
                        ));
                    } else {
                        toasts.add(Toast {
                            kind: ToastKind::Warning,
                            text: "Cannot start a new download!".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });
                    }
                }
                ui.add_space(5.0);
                if ui.button("Download Bulk").clicked() {
                    if self.data.pages == 0 {
                        toasts.add(Toast {
                            kind: ToastKind::Info,
                            text: "You can't get 0 pages.".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });
                    } else if self.task_abort_handle.is_none() {
                        toasts.add(Toast {
                            kind: ToastKind::Info,
                            text: "Starting Download...".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });

                        self.task_abort_handle = Some(dl_bulk_btn(
                            self.data.clone(),
                            self.open_folder,
                            self.channels.dl_count_channel.0.clone(),
                            self.channels.dl_status_channel.0.clone(),
                            self.channels.finished_status_channel.0.clone(),
                        ));
                    } else {
                        toasts.add(Toast {
                            kind: ToastKind::Warning,
                            text: "Cannot start a new download!".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });
                    }
                }

                ui.add_space(10.0);
                if self.downloading_status {
                    ui.spinner();

                    ui.label(format!("Downloading... ({})", self.dl_count));

                    ui.add_space(10.0);
                }

                if let Some(handle) = &self.task_abort_handle {
                    if ui.button("Stop Download").clicked() {
                        toasts.add(Toast {
                            kind: ToastKind::Warning,
                            text: "Aborted Download!".into(),
                            options: ToastOptions::default()
                                .duration_in_seconds(1.5)
                                .show_progress(true),
                        });
                        handle.abort();
                        let _ = self.channels.dl_count_channel.0.send(0);
                        let _ = self.channels.dl_status_channel.0.send(false);
                        self.task_abort_handle = None
                    }
                }
            });
            toasts.show(ctx);
        });
    }
}

fn dl_favs_btn(
    data: EHandler,
    open_dl_folder: bool,
    dl_count_tx: Sender<u64>,
    dl_status_tx: Sender<bool>,
    finished_status_tx: Sender<bool>,
) -> AbortHandle {
    let working_thread = tokio::spawn(async move {
        let _ = dl_status_tx.send(true);
        data.download_favourites().await;

        let _ = dl_count_tx.send(0);
        let _ = dl_status_tx.send(false);
        let _ = finished_status_tx.send(true);

        if open_dl_folder {
            open_dl_dir();
        }
    });

    working_thread.abort_handle()
}

fn dl_tags_btn(
    data: EHandler,
    open_dl_folder: bool,
    dl_count_tx: Sender<u64>,
    dl_status_tx: Sender<bool>,
    finished_status_tx: Sender<bool>,
) -> AbortHandle {
    let working_thread = tokio::spawn(async move {
        let _ = dl_status_tx.send(true);

        data.download_with_tags().await;

        let _ = dl_count_tx.send(0);
        let _ = dl_status_tx.send(false);
        let _ = finished_status_tx.send(true);

        if open_dl_folder {
            open_dl_dir();
        }
    });

    working_thread.abort_handle()
}

fn dl_bulk_btn(
    data: EHandler,
    open_dl_folder: bool,
    dl_count_tx: Sender<u64>,
    dl_status_tx: Sender<bool>,
    finished_status_tx: Sender<bool>,
) -> AbortHandle {
    let working_thread = tokio::spawn(async move {
        let _ = dl_status_tx.send(true);

        data.get_bulk_data().await;

        let _ = dl_count_tx.send(0);
        let _ = dl_status_tx.send(false);
        let _ = finished_status_tx.send(true);

        if open_dl_folder {
            open_dl_dir();
        }
    });

    working_thread.abort_handle()
}
