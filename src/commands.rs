#![allow(dead_code)]

use std::path::Path;
use std::sync::mpsc::Sender;

use reqwest::Client;
use reqwest::{header::HeaderMap, header::HeaderValue, header::USER_AGENT};

use crate::funcs::{self, create_dl_dir, parse_artists};
use crate::type_defs::api_defs::Posts;

#[allow(clippy::too_many_arguments)]
pub async fn download_favourites(
    username: String,
    count: i32,
    random: bool,
    tags: String,
    lower_quality: bool,
    api_source: String,
    tx: Sender<u64>,
    ctx: egui::Context,
) -> Option<f64> {
    // println!("{}", args.random);
    println!(
        "Downloading {} Favorites of {} into the ./dl/ folder!\n",
        count, username
    );

    let client = Client::builder();
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("rust-powered-post-download/0.1"),
    );
    let client = client.default_headers(headers).build().unwrap();

    let random_check: &str = if random { "order:random" } else { "" };

    let tags: &str = if !tags.is_empty() { &tags } else { "" };

    let target: String = format!(
        "https://{}/posts.json?tags=fav:{} {} {}&limit={}",
        api_source, username, tags, random_check, count
    );

    let data: Posts = client
        .get(target)
        .send()
        .await
        .expect("Err")
        .json::<Posts>()
        .await
        .expect("Err");

    if data.posts.is_empty() {
        println!("No post found...");
        return None;
    }

    let created_dir = create_dl_dir().await;
    if created_dir {
        println!("Created a ./dl/ directory for all the downloaded files.\n")
    }

    let mut dl_size: f64 = 0.0;
    for post in data.posts {
        let artist_name = parse_artists(&post.tags);

        let path_string = format!("./dl/{}-{}.{}", artist_name, post.id, post.file.ext);
        let path = Path::new(&path_string);

        println!(
            "Starting download of {}-{}.{}",
            artist_name, post.id, post.file.ext
        );

        if !path.exists() {
            let file_size: f64;
            if lower_quality {
                file_size = funcs::lower_quality_dl(&post, &artist_name).await
            } else {
                match &post.file.url {
                    Some(url) => {
                        file_size =
                            funcs::download(url, &post.file.ext, post.id, &artist_name).await
                    }
                    None => {
                        println!(
                            "Cannot download post {}-{} due to it missing a file url",
                            artist_name, post.id
                        );
                        file_size = 0.0;
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

            let _ = tx.send(1);
            ctx.request_repaint();

            dl_size += file_size
        } else {
            println!(
                "File {}-{}.{} already Exists!\n",
                artist_name, post.id, post.file.ext
            )
        }
    }

    if dl_size > 0.0 {
        Some(dl_size)
    } else {
        None
    }
}
