//! Drives e-cli's blocking commands on background OS threads and reports
//! progress back to the egui update loop over `std::sync::mpsc` channels.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use e_cli::cli::ArchiveFormat;
use e_cli::commands::get_client;
use e_cli::funcs::{self, get_pages, get_pool, get_post_data};
use e_cli::type_defs::api_defs::Post;
use e_cli::{CliContext, DownloadStatistics, Login, Tracker};

#[derive(Clone)]
pub struct DownloadSettings {
    pub nsfw: bool,
    pub username: String,
    pub api_key: String,
    pub tags: String,
    pub count: u32,
    pub pages: i64,
    pub threads: usize,
    pub random: bool,
    pub lower_quality: bool,
    pub track_file: String,
    pub retries: u32,
    pub duplicate_index: String,
    pub dry_run: bool,
    pub manifest_path: String,
    pub failure_manifest: String,
}

pub enum JobKind {
    Favourites,
    Tags,
    Pool(u64),
    RetryFailed,
}

pub enum Progress {
    /// Total post count is now known; UI can switch from indeterminate to a counter.
    Total(u64),
    /// One more post has finished (successfully or not).
    Tick,
    Finished(DownloadStatistics),
    Cancelled,
    Error(String),
}

pub enum ZipEvent {
    Finished(bool),
}

/// Spawns a download job on its own thread. Progress/completion is reported via `tx`.
/// `cancel` is checked between pages/posts so "Stop" can take effect promptly without
/// aborting a file download mid-write.
pub fn spawn_download(
    kind: JobKind,
    settings: DownloadSettings,
    output_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<Progress>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if !settings.dry_run {
            funcs::ensure_dl_dir(&output_dir);
        }

        let context = CliContext {
            verbose: false,
            nsfw: settings.nsfw,
            lower_quality: settings.lower_quality,
            pages: settings.pages,
            num_threads: settings.threads.clamp(1, 10),
            retries: settings.retries,
            duplicate_index: if settings.dry_run {
                None
            } else {
                load_duplicate_index(&settings.duplicate_index, &output_dir, &tx)
            },
            cancel: Some(cancel.clone()),
            progress: None,
        };
        let login = Login {
            username: settings.username.clone(),
            api_key: settings.api_key.clone(),
        };
        let tracker = if settings.dry_run || settings.track_file.trim().is_empty() {
            None
        } else {
            match Tracker::load(std::path::Path::new(&settings.track_file)) {
                Ok(t) => Some(t),
                Err(e) => {
                    let _ = tx.send(Progress::Error(format!(
                        "Failed to open tracking file {}: {e}",
                        settings.track_file
                    )));
                    return;
                }
            }
        };
        if matches!(kind, JobKind::RetryFailed) {
            run_retry_failed(&settings, &output_dir, &cancel, &tx, tracker.as_ref());
            return;
        }
        let client = get_client();
        let random_check = if settings.random { "order:random" } else { "" };

        let pages: Vec<Vec<Post>> = match &kind {
            JobKind::Favourites => {
                let fav = format!("fav:{}", settings.username);
                get_pages(
                    &context,
                    &login,
                    &client,
                    &fav,
                    &settings.tags,
                    random_check,
                    &settings.count,
                )
            }
            JobKind::Tags => get_pages(
                &context,
                &login,
                &client,
                "",
                &settings.tags,
                random_check,
                &settings.count,
            ),
            JobKind::Pool(pool_id) => {
                let Some(pool) = get_pool(&context, &client, &login, pool_id) else {
                    let _ = tx.send(Progress::Error("Pool not found.".into()));
                    return;
                };
                let posts = get_post_data(&context, &client, &login, &pool.post_ids);
                if posts.is_empty() {
                    let _ = tx.send(Progress::Error("Pool has no downloadable posts.".into()));
                    return;
                }
                run_indexed_download(
                    posts,
                    &settings,
                    &client,
                    &login,
                    &context,
                    &output_dir,
                    &cancel,
                    &tx,
                    tracker.as_ref(),
                );
                return;
            }
            JobKind::RetryFailed => unreachable!(),
        };

        if pages.is_empty() {
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(Progress::Cancelled);
                return;
            }
            let _ = tx.send(Progress::Error("No posts found.".into()));
            return;
        }

        let total: usize = pages.iter().map(|p| p.len()).sum();
        let _ = tx.send(Progress::Total(total as u64));

        if settings.dry_run {
            let estimated = pages
                .iter()
                .flatten()
                .filter_map(|post| post.file.size)
                .sum::<u64>() as f64;
            finish_download(
                &settings,
                &output_dir,
                &context,
                DownloadStatistics {
                    completed: 0,
                    failed: 0,
                    skipped: 0,
                    total,
                    downloaded_amount: estimated,
                    records: Vec::new(),
                },
                &tx,
            );
            return;
        }

        let mut completed: i64 = 0;
        let mut failed: i64 = 0;
        let mut skipped: i64 = 0;
        let mut downloaded_amount = 0.0;
        let mut records = Vec::new();

        let pool = rayon_pool(context.num_threads);

        for posts in pages {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let (page_finished, page_failed, page_skipped, page_bytes, page_records) =
                download_posts_parallel(
                    &pool,
                    &client,
                    &login,
                    &context,
                    &output_dir,
                    posts,
                    &cancel,
                    &tx,
                    tracker.as_ref(),
                );

            completed += page_finished;
            failed += page_failed;
            skipped += page_skipped;
            downloaded_amount += page_bytes;
            records.extend(page_records);
        }

        finish_download(
            &settings,
            &output_dir,
            &context,
            DownloadStatistics {
                completed,
                failed,
                skipped,
                total,
                downloaded_amount,
                records,
            },
            &tx,
        );
    })
}

#[allow(clippy::too_many_arguments)]
fn run_indexed_download(
    posts: Vec<Post>,
    settings: &DownloadSettings,
    client: &reqwest::blocking::Client,
    login: &Login,
    context: &CliContext,
    output_dir: &std::path::Path,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<Progress>,
    tracker: Option<&Tracker>,
) {
    funcs::ensure_dl_dir(output_dir);

    let total = posts.len();
    let _ = tx.send(Progress::Total(total as u64));

    let pool = rayon_pool(context.num_threads);
    let indexed: Vec<(u64, Post)> = posts
        .into_iter()
        .enumerate()
        .map(|(i, p)| ((i + 1) as u64, p))
        .collect();

    let mut completed: i64 = 0;
    let mut failed: i64 = 0;
    let mut skipped: i64 = 0;
    let mut downloaded_amount = 0.0;
    let mut records = Vec::new();

    if !cancel.load(Ordering::Relaxed) {
        let (tx_chunk, rx_chunk) = std::sync::mpsc::channel();
        let lower_quality = context.lower_quality;
        pool.install(|| {
            use rayon::prelude::*;
            indexed.into_par_iter().for_each_with(
                (tx_chunk, tx.clone()),
                |(chunk_tx, progress_tx), (index, post)| {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let result = funcs::download_with_options(
                        client,
                        login,
                        vec![post],
                        Some(&index),
                        &lower_quality,
                        output_dir,
                        tracker,
                        funcs::DownloadOptions {
                            retries: context.retries,
                            duplicate_index: context.duplicate_index.as_deref(),
                            cancel: Some(cancel.clone()),
                        },
                    );
                    let _ = progress_tx.send(Progress::Tick);
                    let _ = chunk_tx.send(result);
                },
            );
        });
        for result in rx_chunk {
            completed += result.amount_finished;
            failed += result.amount_failed;
            skipped += result.amount_skipped;
            downloaded_amount += result.amount;
            records.extend(result.records);
        }
    }

    finish_download(
        settings,
        output_dir,
        context,
        DownloadStatistics {
            completed,
            failed,
            skipped,
            total,
            downloaded_amount,
            records,
        },
        tx,
    );
}

fn rayon_pool(num_threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads.max(1))
        .build()
        .expect("Error building thread pool")
}

#[allow(clippy::too_many_arguments)]
fn download_posts_parallel(
    pool: &rayon::ThreadPool,
    client: &reqwest::blocking::Client,
    login: &Login,
    context: &CliContext,
    output_dir: &std::path::Path,
    posts: Vec<Post>,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<Progress>,
    tracker: Option<&Tracker>,
) -> (i64, i64, i64, f64, Vec<e_cli::DownloadRecord>) {
    use rayon::prelude::*;

    let (chunk_tx, chunk_rx) = std::sync::mpsc::channel();
    let lower_quality = context.lower_quality;

    pool.install(|| {
        posts.into_par_iter().for_each_with(
            (chunk_tx, tx.clone()),
            |(result_tx, progress_tx), post| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let result = funcs::download_with_options(
                    client,
                    login,
                    vec![post],
                    None,
                    &lower_quality,
                    output_dir,
                    tracker,
                    funcs::DownloadOptions {
                        retries: context.retries,
                        duplicate_index: context.duplicate_index.as_deref(),
                        cancel: Some(cancel.clone()),
                    },
                );
                let _ = progress_tx.send(Progress::Tick);
                let _ = result_tx.send(result);
            },
        );
    });

    let mut finished = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut amount = 0.0;
    let mut records = Vec::new();
    for result in chunk_rx {
        finished += result.amount_finished;
        failed += result.amount_failed;
        skipped += result.amount_skipped;
        amount += result.amount;
        records.extend(result.records);
    }
    (finished, failed, skipped, amount, records)
}

fn load_duplicate_index(
    configured: &str,
    output_dir: &std::path::Path,
    tx: &Sender<Progress>,
) -> Option<Arc<e_cli::duplicate::DuplicateIndex>> {
    let path = if configured.trim().is_empty() {
        output_dir.join(".e-cli-md5.json")
    } else {
        PathBuf::from(configured)
    };
    match e_cli::duplicate::DuplicateIndex::load(&path) {
        Ok(index) => Some(Arc::new(index)),
        Err(error) => {
            let _ = tx.send(Progress::Error(format!(
                "Failed to open duplicate index {}: {error}",
                path.display()
            )));
            None
        }
    }
}

fn finish_download(
    settings: &DownloadSettings,
    output_dir: &std::path::Path,
    context: &CliContext,
    statistics: DownloadStatistics,
    tx: &Sender<Progress>,
) {
    if !settings.dry_run {
        if !settings.manifest_path.trim().is_empty() {
            if let Err(error) = e_cli::manifest::write(
                std::path::Path::new(settings.manifest_path.trim()),
                &statistics,
            ) {
                let _ = tx.send(Progress::Error(error));
            }
        }
        let failure_path = if settings.failure_manifest.trim().is_empty() {
            output_dir.join(".e-cli-failed.json")
        } else {
            PathBuf::from(settings.failure_manifest.trim())
        };
        if let Some(manifest) = e_cli::failure_manifest::FailureManifest::from_statistics(
            context.api_source(),
            output_dir,
            context.lower_quality,
            context.retries,
            &statistics,
        ) {
            if let Err(error) = manifest.save(&failure_path) {
                let _ = tx.send(Progress::Error(error));
            }
        } else if statistics.failed == 0 {
            let _ = std::fs::remove_file(failure_path);
        }
    }
    let _ = tx.send(Progress::Finished(statistics));
}

fn run_retry_failed(
    settings: &DownloadSettings,
    output_dir: &std::path::Path,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<Progress>,
    tracker: Option<&Tracker>,
) {
    let failure_path = if settings.failure_manifest.trim().is_empty() {
        output_dir.join(".e-cli-failed.json")
    } else {
        PathBuf::from(settings.failure_manifest.trim())
    };
    let manifest = match e_cli::failure_manifest::FailureManifest::load(&failure_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = tx.send(Progress::Error(error));
            return;
        }
    };
    let retry_dir = manifest.destination.clone();
    if !settings.dry_run {
        funcs::ensure_dl_dir(&retry_dir);
    }
    let duplicate_path = if settings.duplicate_index.trim().is_empty() {
        retry_dir.join(".e-cli-md5.json")
    } else {
        PathBuf::from(settings.duplicate_index.trim())
    };
    let duplicate_index = e_cli::duplicate::DuplicateIndex::load(&duplicate_path)
        .ok()
        .map(Arc::new);
    let context = CliContext {
        verbose: false,
        nsfw: manifest.api_source == "e621.net",
        lower_quality: manifest.lower_quality,
        pages: -1,
        num_threads: settings.threads.clamp(1, 10),
        retries: settings.retries,
        duplicate_index,
        cancel: Some(cancel.clone()),
        progress: None,
    };
    let login = Login {
        username: settings.username.clone(),
        api_key: settings.api_key.clone(),
    };
    let client = get_client();
    let ids = manifest
        .records
        .iter()
        .map(|record| record.post_id)
        .collect::<Vec<_>>();
    let posts = get_post_data(&context, &client, &login, &ids);
    let _ = tx.send(Progress::Total(ids.len() as u64));
    let stats = funcs::download_with_options(
        &client,
        &login,
        posts,
        None,
        &manifest.lower_quality,
        &retry_dir,
        tracker,
        funcs::DownloadOptions {
            retries: settings.retries,
            duplicate_index: context.duplicate_index.as_deref(),
            cancel: Some(cancel.clone()),
        },
    )
    .into_statistics(ids.len());
    finish_download(settings, &retry_dir, &context, stats, tx);
}

/// Packages `dir` into an archive on its own thread (shells out to `7z`).
pub fn spawn_zip(
    dir: PathBuf,
    name: String,
    format: ArchiveFormat,
    tx: Sender<ZipEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let ok = e_cli::commands::zip_downloads(&dir, &name, format);
        let _ = tx.send(ZipEvent::Finished(ok));
    })
}
