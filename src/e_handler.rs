#![allow(dead_code)]

use std::{cmp::Ordering, fs, path::Path, sync::mpsc::Sender};

use reqwest::{
    header::{HeaderMap, HeaderValue, USER_AGENT},
    Client,
};
use tokio::fs as tokio_fs;

use crate::{
    type_defs::api_defs::{Posts, Tags},
    util_lib::{self, create_data_dir, create_dl_dir},
};

#[derive(Clone)]
pub struct EHandler {
    pub username: String,
    api_key: String,
    pub count: i32,
    pub pages: i32,
    pub random: bool,
    pub tags: String,
    pub lower_quality: bool,
    pub api_source: String,
    pub dl_count_tx: Option<Sender<u64>>,
    pub post_count_tx: Option<Sender<u64>>,
    ctx: Option<egui::Context>,
    client: Client,
    old_post_count: u64,
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
            api_key: String::new(),
            count: 5,
            pages: 5,
            random: false,
            tags: String::new(),
            lower_quality: false,
            api_source: "e926.net".to_string(),
            dl_count_tx: None,
            post_count_tx: None,
            ctx: None,
            client: new_client,
            old_post_count: 0,
        }
    }
}

impl EHandler {
    pub fn define_gui(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }

    pub fn check_api_key(&mut self) -> bool {
        if Path::new("./key").exists() {
            let key = fs::read_to_string("./key").expect("Err");
            if !key.is_empty() {
                self.api_key = key;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn clear_api_key(&mut self) {
        if !self.api_key.is_empty() {
            self.api_key = String::new();
        }
    }

    pub fn define_senders(&mut self, dl_count_tx: Sender<u64>, post_count_tx: Sender<u64>) {
        self.dl_count_tx = Some(dl_count_tx);
        self.post_count_tx = Some(post_count_tx);
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

    async fn handle_request(&self, target: String) -> Posts {
        if !self.api_key.is_empty() {
            self.client
                .get(target)
                .basic_auth(self.username.clone(), Some(self.api_key.clone()))
                .send()
                .await
                .expect("Err")
                .json::<Posts>()
                .await
                .expect("Err")
        } else {
            self.client
                .get(target)
                .send()
                .await
                .expect("Err")
                .json::<Posts>()
                .await
                .expect("Err")
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

                let _ = self.dl_count_tx.as_ref().unwrap().send(1);
                self.ctx.as_ref().unwrap().request_repaint();
            } else {
                println!(
                    "File {}-{}.{} already Exists!\n",
                    artist_name, post.id, post.file.ext
                );

                let _ = self.dl_count_tx.as_ref().unwrap().send(1);
                self.ctx.as_ref().unwrap().request_repaint();
            }
        }
        println!("Download Complete!");
    }

    pub async fn download_favourites(&self) {
        let _ = self.post_count_tx.as_ref().unwrap().send(0);

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

            let data: Posts = self.handle_request(target).await;

            if data.posts.is_empty() {
                println!("No post found...");
            }

            let posts_amount = u64::try_from(data.posts.len()).unwrap();
            let _ = self.post_count_tx.as_ref().unwrap().send(posts_amount);

            let created_dir = create_dl_dir().await;
            if created_dir {
                println!("Created a ./dl/ directory for all the downloaded files.\n")
            }

            self.handle_download(data).await;
        }
    }

    pub async fn download_with_tags(&self) {
        let _ = self.post_count_tx.as_ref().unwrap().send(0);

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

        let data: Posts = self.handle_request(target).await;

        if data.posts.is_empty() {
            println!("No post found...");
        }

        let posts_amount = u64::try_from(data.posts.len()).unwrap();
        let _ = self.post_count_tx.as_ref().unwrap().send(posts_amount);

        let created_dir = create_dl_dir().await;
        if created_dir {
            println!("Created a ./dl/ directory for all the downloaded files.\n")
        }

        self.handle_download(data).await;
    }

    pub async fn get_bulk_data(&self) {
        let _ = self.post_count_tx.as_ref().unwrap().send(0);

        println!("Downloading posts, into the ./dl/ folder!\n");

        let random_check: &str = if self.random { "order:random" } else { "" };

        let tags: &str = if !self.tags.is_empty() {
            &self.tags
        } else {
            ""
        };

        let fav = if !self.username.is_empty() {
            format!("fav:{:?}", self.username)
        } else {
            "".to_owned()
        };

        if Path::new("./data/").exists() {
            let _ = trash::delete("./data/");
        }
        create_data_dir().await;

        let mut page = 0;
        #[allow(unused_assignments)]
        let mut num_file = 0;
        let mut posts_amount: u64 = 0;

        loop {
            if self.pages != -1 && page == self.pages {
                num_file = page;
                break;
            }

            let target: String = format!(
                "https://{}/posts.json?tags={} {} {}&limit={}&page={}",
                self.api_source,
                fav,
                tags,
                random_check,
                self.count,
                page + 1
            );

            let data: Posts = self.handle_request(target).await;

            //println!("\n\n\n\n{:?}", data);
            if data.posts.is_empty() {
                num_file = page;
                break;
            }

            posts_amount += u64::try_from(data.posts.len()).unwrap();

            let _ = tokio_fs::write(
                format!("./data/post_page_{}.json", page + 1),
                serde_json::to_string(&data).unwrap(),
            )
            .await;

            page += 1
        }

        println!("Finished getting data! Num. of data files: {}", num_file);

        page = 0;

        let created_dir = create_dl_dir().await;
        if created_dir {
            println!("Created a ./dl/ directory for all the downloaded files.\n")
        }

        let _ = self.post_count_tx.as_ref().unwrap().send(posts_amount);

        loop {
            if page == num_file {
                break;
            }

            let contents = tokio_fs::read_to_string(format!("./data/post_page_{}.json", page + 1))
                .await
                .expect("Err");
            let data: Posts = serde_json::from_str(&contents).expect("Err");
            //println!("{:?}", data);

            self.handle_download(data).await;

            page += 1
        }
    }
}
