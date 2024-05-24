#![allow(dead_code)]

use std::{cmp::Ordering, path::Path, sync::mpsc::Sender};

use reqwest::{
    header::{HeaderMap, HeaderValue, USER_AGENT},
    Client,
};

use crate::{
    type_defs::api_defs::{Posts, Tags},
    util_lib::{self, create_dl_dir},
};

#[derive(Clone)]
pub struct EHandler {
    pub username: String,
    pub count: i32,
    pub random: bool,
    pub tags: String,
    pub lower_quality: bool,
    pub api_source: String,
    pub tx: Option<Sender<u64>>,
    ctx: Option<egui::Context>,
    client: Client,
}

impl Default for EHandler {
    fn default() -> Self {
        let client = Client::builder();
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("rust-powered-post-download/0.1"),
        );
        let new_client = client.default_headers(headers).build().unwrap();

        Self {
            username: String::new(),
            count: 5,
            random: false,
            tags: String::new(),
            lower_quality: false,
            api_source: "e926.net".to_string(),
            tx: None,
            ctx: None,
            client: new_client,
        }
    }
}

impl EHandler {
    pub fn define_gui(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }

    pub fn define_sender(&mut self, tx: Sender<u64>) {
        self.tx = Some(tx)
    }

    fn parse_artists(&self, tags: &Tags) -> String {
        match tags.artist.len().cmp(&1) {
            Ordering::Greater => {
                let mut artists: String = String::new();
                for artist in tags.artist.iter() {
                    artists = artists + artist + ", "
                }
                artists[..artists.len() - 2].to_string()
            }
            Ordering::Equal => tags.artist[0].to_string(),
            Ordering::Less => "unknown-artist".to_string(),
        }
    }

    async fn handle_download(&self, data: Posts) {
        for post in data.posts {
            let artist_name = self.parse_artists(&post.tags);

            let path_string = format!("./dl/{}-{}.{}", artist_name, post.id, post.file.ext);
            let path = Path::new(&path_string);

            println!(
                "Starting download of {}-{}.{}",
                artist_name, post.id, post.file.ext
            );

            if !path.exists() {
                let file_size: f64;
                if self.lower_quality {
                    file_size = util_lib::lower_quality_dl(&post, &artist_name).await
                } else {
                    match &post.file.url {
                        Some(url) => {
                            file_size =
                                util_lib::download(url, &post.file.ext, post.id, &artist_name).await
                        }
                        None => {
                            println!(
                                "Cannot download post {}-{} due to it missing a file url",
                                artist_name, post.id
                            );
                            file_size = 0.0
                        }
                    }
                }

                println!(
                    "Downloaded {}-{}.{}! File size: {:.2} MB\n",
                    artist_name,
                    post.id,
                    post.file.ext,
                    file_size / 1024.0 / 1024.0
                );

                let _ = self.tx.as_ref().unwrap().send(1);
                self.ctx.as_ref().unwrap().request_repaint();
            } else {
                println!(
                    "File {}-{}.{} already Exists!\n",
                    artist_name, post.id, post.file.ext
                )
            }
        }
        println!("Download Complete!");
    }

    pub async fn download_favourites(&self) {
        println!(
            "Downloading {} Favorites of {} into the ./dl/ folder!\n",
            self.count, self.username
        );

        if !self.username.is_empty() {
            let random_check: &str = if self.random { "order:random" } else { "" };

            let tags: &str = if !self.tags.is_empty() {
                &self.tags
            } else {
                ""
            };

            let target: String = format!(
                "https://{}/posts.json?tags=fav:{:?} {} {}&limit={}",
                self.api_source, self.username, tags, random_check, self.count
            );

            let data: Posts = self
                .client
                .get(target)
                .send()
                .await
                .expect("Err")
                .json::<Posts>()
                .await
                .expect("Err");

            if data.posts.is_empty() {
                println!("No post found...");
            }

            let created_dir = create_dl_dir().await;
            if created_dir {
                println!("Created a ./dl/ directory for all the downloaded files.\n")
            }

            self.handle_download(data).await;
        }
    }

    pub async fn download_with_tags(&self) {
        println!("Downloading posts, into the ./dl/ folder!\n");

        let random_check: &str = if self.random { "order:random" } else { "" };

        let tags: &str = if !self.tags.is_empty() {
            &self.tags
        } else {
            ""
        };

        let target: String = format!(
            "https://{}/posts.json?tags={} {}&limit={}",
            self.api_source, tags, random_check, self.count
        );

        let data: Posts = self
            .client
            .get(target)
            .send()
            .await
            .expect("Err")
            .json::<Posts>()
            .await
            .expect("Err");

        if data.posts.is_empty() {
            println!("No post found...");
        }

        let created_dir = create_dl_dir().await;
        if created_dir {
            println!("Created a ./dl/ directory for all the downloaded files.\n")
        }

        self.handle_download(data).await;
    }
}
