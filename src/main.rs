#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(rustdoc::missing_crate_level_docs)]

use commands::download_favourites;
use eframe::egui;
use tokio::runtime::Runtime;

mod funcs;
mod type_defs;
mod commands;

#[tokio::main]
async fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([300.0, 300.0]),
        ..Default::default()
    };

    let _ = eframe::run_native("E-CLI GUI", options, Box::new(|_| {
        Box::<App>::default()
    }));
}

struct App {
    tags: String,
    username: String,
    post_amount: i32,
    random: bool,
    lower_quality: bool,
    api_source: String
}

impl Default for App {
    fn default() -> Self {
        Self { 
            tags: String::new(), 
            username: String::new(), 
            post_amount: 5, 
            random: false, 
            lower_quality: false, 
            api_source: "e926.net".to_string()
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let api_source_label = ui.label("Api Source");
                ui.text_edit_singleline(&mut self.api_source).labelled_by(api_source_label.id);

                let username_label = ui.label("Username");
                ui.text_edit_singleline(&mut self.username).labelled_by(username_label.id);
                
                let tags_label = ui.label("Tags");
                ui.text_edit_multiline(&mut self.tags).labelled_by(tags_label.id);

                ui.checkbox(&mut self.random, "Get Random Posts?");
                ui.checkbox(&mut self.lower_quality, "Get lower quality of posts?");
            });
            ui.add_space(5.0);
            let slider = ui.add(egui::Slider::new(&mut self.post_amount, 1..=250).prefix("Download: ").text("Posts."));
            slider.on_hover_text("Amount of posts to download.");

            ui.add_space(15.0);
            ui.vertical_centered(|ui| {
                if ui.button("Download Favourites").clicked() {
                    println!("Something...");
                }
                ui.add_space(5.0);
                ui.add_enabled(false, egui::Button::new("Download Posts with Tags"));
            })
        });
    }
}