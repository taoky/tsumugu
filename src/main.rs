use std::{
    collections::HashSet,
    fs::File,
    io::Write,
    net::IpAddr,
    os::unix::fs::symlink,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use clap::{Parser, Subcommand};
use crossbeam_deque::{Injector, Worker};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use parser::ParserType;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;
use url::Url;

mod list;
mod parser;
use list::ListItem;

use crate::{
    compare::{should_download_by_head, should_download_by_list},
    parser::ListResult,
    utils::{again, again_async, get_async, head}, regex::ExclusionManager,
};

mod compare;
mod regex;
mod utils;

use crate::regex::ExpandedRegex;

#[derive(Debug, Clone)]
enum TaskType {
    Listing,
    Download(ListItem),
}

#[derive(Debug, Clone)]
struct Task {
    task: TaskType,
    relative: Vec<String>,
    url: Url,
}

#[derive(Parser, Debug)]
#[command(about, version)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sync files from upstream to local.
    Sync(SyncArgs),

    /// List files from upstream.
    List(ListArgs),
}

#[derive(Parser, Debug)]
struct SyncArgs {
    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// Do not download files and cleanup.
    #[clap(long)]
    dry_run: bool,

    /// Threads at work.
    #[clap(long, default_value_t = 2)]
    threads: usize,

    /// Do not clean up after sync.
    #[clap(long)]
    no_delete: bool,

    /// Set max delete count.
    #[clap(long, default_value_t = 100)]
    max_delete: usize,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream: Url,

    /// The local directory.
    #[clap(value_parser)]
    local: PathBuf,

    /// Default: auto. You can set a valid URL for guessing, or an invalid one for disabling.
    #[clap(long)]
    timezone_file: Option<String>,

    /// Retry count for each request.
    #[clap(long, default_value_t = 3)]
    retry: usize,

    /// Do an HEAD before actual GET. Add this if you are not sure if the results from parser is correct.
    #[clap(long)]
    head_before_get: bool,

    /// Choose a parser.
    #[clap(long, value_enum, default_value_t = ParserType::Nginx)]
    parser: ParserType,

    /// Excluded file regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included file regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,
}

#[derive(Parser, Debug)]
struct ListArgs {
    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream: Url,

    /// Choose a parser.
    #[clap(long, value_enum, default_value_t=ParserType::Nginx)]
    parser: ParserType,
}

macro_rules! build_client {
    ($client: ty, $args: expr, $parser: expr, $bind_address: expr) => {{
        let mut builder = <$client>::builder()
            .user_agent($args.user_agent.clone())
            .local_address($bind_address.map(|x| x.parse::<IpAddr>().unwrap()));
        if !$parser.is_auto_redirect() {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }
        builder.build().unwrap()
    }};
}

fn main() {
    // https://github.com/tokio-rs/tracing/issues/735#issuecomment-957884930
    std::env::set_var(
        "RUST_LOG",
        format!("info,{}", std::env::var("RUST_LOG").unwrap_or_default()),
    );
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bind_address = std::env::var("BIND_ADDRESS").ok();

    let args = Cli::parse();
    let args = match args.command {
        Commands::Sync(args) => args,
        Commands::List(args) => {
            let parser = args.parser.build();
            let client = build_client!(reqwest::blocking::Client, args, parser, bind_address);
            let list = parser.get_list(&client, &args.upstream).unwrap();
            match list {
                ListResult::Redirect(url) => {
                    println!("Redirect to {}", url);
                }
                ListResult::List(list) => {
                    for item in list {
                        println!("{}", item);
                    }
                }
            }
            return;
        }
    };
    debug!("{:?}", args);

    let exclusion_manager = ExclusionManager::new(args.exclude, args.include);

    let parser = args.parser.build();
    let client = build_client!(
        reqwest::blocking::Client,
        args,
        parser,
        bind_address.as_ref()
    );

    // async support
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_client = build_client!(reqwest::Client, args, parser, bind_address.as_ref());

    // terminate whole process when a thread panics
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(3);
    }));

    let mprogress = MultiProgress::new();

    // Check if to guess timezone
    let timezone_file = match args.timezone_file {
        Some(f) => match Url::parse(&f) {
            Ok(url) => Some(url),
            Err(_) => {
                warn!("Invalid timezone file URL, disabling timezone guessing");
                None
            }
        },
        None => {
            // eek, try getting first file in root index
            let list = parser.get_list(&client, &args.upstream).unwrap();
            match list {
                ListResult::List(list) => {
                    match list.iter().find(|x| x.type_ == list::FileType::File) {
                        None => {
                            warn!("No files in root index, disabling timezone guessing");
                            None
                        }
                        Some(x) => Some(x.url.clone()),
                    }
                }
                ListResult::Redirect(_) => {
                    warn!("Root index is a redirect, disabling timezone guessing");
                    None
                }
            }
        }
    };
    let timezone = match timezone_file {
        Some(timezone_url) => {
            let timezone = list::guess_remote_timezone(&*parser, &client, timezone_url);
            let timezone = match timezone {
                Ok(tz) => Some(tz),
                Err(e) => {
                    warn!("Failed to guess timezone: {:?}", e);
                    None
                }
            };
            info!("Guessed timezone: {:?}", timezone);
            timezone
        }
        None => None,
    };

    let download_dir = args.local.as_path();
    if !args.dry_run {
        std::fs::create_dir_all(download_dir).unwrap();
    }

    let remote_list = Arc::new(Mutex::new(HashSet::new()));

    let workers: Vec<_> = (0..args.threads)
        .map(|_| Worker::<Task>::new_fifo())
        .collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let global = Injector::<Task>::new();

    global.push(Task {
        task: TaskType::Listing,
        relative: vec![],
        url: args.upstream,
    });

    let active_cnt = AtomicUsize::new(0);
    let wake = AtomicUsize::new(0);

    let stat_objects = AtomicUsize::new(0);
    let stat_size = AtomicU64::new(0);

    let failure_listing = AtomicBool::new(false);
    let failure_downloading = AtomicBool::new(false);

    std::thread::scope(|scope| {
        for worker in workers.into_iter() {
            let stealers = &stealers;
            let parser = &parser;
            let client = client.clone();
            let global = &global;

            let active_cnt = &active_cnt;
            let wake = &wake;

            let remote_list = remote_list.clone();

            let async_client = async_client.clone();
            let runtime = &runtime;

            let mprogress = mprogress.clone();
            let exclusion_manager = exclusion_manager.clone();

            let stat_objects = &stat_objects;
            let stat_size = &stat_size;

            let failure_listing = &failure_listing;
            let failure_downloading = &failure_downloading;
            scope.spawn(move || {
                loop {
                    active_cnt.fetch_add(1, Ordering::SeqCst);
                    while let Some(task) = worker.pop().or_else(|| {
                        std::iter::repeat_with(|| {
                            global
                                .steal_batch_and_pop(&worker)
                                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
                        })
                        .find(|s| !s.is_retry())
                        .and_then(|s| s.success())
                    }) {
                        let relative = task.relative.join("/");
                        let cwd = download_dir.join(&relative);
                        debug!("cwd: {:?}, relative: {:?}", cwd, relative);
                        // exclude this?
                        let exclusion_result = exclusion_manager.match_str(&relative);
                        if exclusion_result == regex::Comparison::Stop {
                            info!("Skipping excluded {:?}", &relative);
                            continue;
                        } else if exclusion_result == regex::Comparison::ListOnly {
                            info!("List only in {:?}", &relative);
                        }
                        match task.task {
                            TaskType::Listing => {
                                info!("Listing {}", task.url);
                                {
                                    remote_list.lock().unwrap().insert(cwd.clone());
                                }

                                let items = match again(|| parser.get_list(&client, &task.url), args.retry) {
                                    Ok(items) => items,
                                    Err(e) => {
                                        error!("Failed to list {}: {:?}", task.url, e);
                                        failure_listing.store(true, Ordering::SeqCst);
                                        continue;
                                    }
                                };
                                match items {
                                    ListResult::List(items) => {
                                        for item in items {
                                            if item.type_ == list::FileType::Directory {
                                                let mut relative = task.relative.clone();
                                                relative.push(item.name);
                                                worker.push(Task {
                                                    task: TaskType::Listing,
                                                    relative,
                                                    url: item.url,
                                                });
                                                wake.fetch_add(1, Ordering::SeqCst);
                                            } else {
                                                if exclusion_result == regex::Comparison::ListOnly {
                                                    info!("Skipping (by list only) {}", task.url);
                                                    continue;
                                                }
                                                worker.push(Task {
                                                    task: TaskType::Download(item.clone()),
                                                    relative: task.relative.clone(),
                                                    url: item.url,
                                                });
                                                wake.fetch_add(1, Ordering::SeqCst);
                                                stat_size.fetch_add(match item.size {
                                                    Some(size) => size.get_estimated(),
                                                    None => 0,
                                                }, Ordering::SeqCst);
                                            }
                                            stat_objects.fetch_add(1, Ordering::SeqCst);
                                        }
                                    }
                                    ListResult::Redirect(target_url) => {
                                        // This "Redirect" only supports creating symlink of current directory
                                        info!("Redirected {} -> {}. Try to create a symlink", task.url, target_url);
                                        if cwd.symlink_metadata().is_ok() {
                                            info!("Skipping symlink creation because symlink {:?} already exists", cwd);
                                            continue;
                                        } else if cwd.exists() {
                                            warn!("Skipping symlink creation because {:?} already exists, but it is not a symlink", cwd);
                                            continue;
                                        }
                                        // get last segment of target_url
                                        let target_name = match target_url.split('/').nth_back(1) {
                                            Some(name) => name,
                                            None => {
                                                error!("Failed to get last segment of target_url: {}", target_url);
                                                continue;
                                            }
                                        };
                                        info!("Try symlink {:?} -> {}", cwd, target_name);
                                        if let Err(e) = symlink(target_name, cwd.clone()) {
                                            error!("Failed to create symlink {:?} -> {}: {:?}", cwd, target_name, e);
                                        }
                                    }
                                }
                            }
                            TaskType::Download(item) => {
                                // create path in case for first sync
                                if !args.dry_run {
                                    std::fs::create_dir_all(&cwd).unwrap();
                                }
                                let expected_path = cwd.join(&item.name);
                                {
                                    remote_list.lock().unwrap().insert(expected_path.clone());
                                }
                                if !should_download_by_list(&expected_path, &item, timezone) {
                                    info!("Skipping {}", task.url);
                                    continue;
                                }

                                if args.head_before_get {
                                    let resp = match again(|| head(&client, item.url.clone()), args.retry) {
                                        Ok(resp) => resp,
                                        Err(e) => {
                                            error!("Failed to HEAD {}: {:?}", task.url, e);
                                            failure_downloading.store(true, Ordering::SeqCst);
                                            continue;
                                        }
                                    };
                                    if !should_download_by_head(&expected_path, &resp) {
                                        info!("Skipping (by HEAD) {}", task.url);
                                        continue;
                                    }
                                }

                                if !args.dry_run {
                                    let future = async {
                                        // Here we use async to allow streaming and progress bar
                                        // Ref: https://gist.github.com/giuliano-oliveira/4d11d6b3bb003dba3a1b53f43d81b30d
                                        let resp = match again_async(|| get_async(&async_client, item.url.clone()), args.retry).await {
                                            Ok(resp) => resp,
                                            Err(e) => {
                                                error!("Failed to GET {}: {:?}", task.url, e);
                                                failure_downloading.store(true, Ordering::SeqCst);
                                                return;
                                            }
                                        };
                                        let total_size = resp.content_length().unwrap();
                                        let pb = mprogress.add(ProgressBar::new(total_size));
                                        pb.set_style(ProgressStyle::default_bar()
                                            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
                                            .progress_chars("#>-"));
                                        pb.set_message(format!("Downloading {}", item.url));

                                        let mtime = utils::get_async_response_mtime(&resp).unwrap();

                                        let tmp_path = cwd.join(format!(".tmp.{}", item.name));
                                        {
                                            let mut dest_file = File::create(&tmp_path).unwrap();
                                            let mut stream = resp.bytes_stream();

                                            while let Some(item) = stream.next().await {
                                                let chunk = item.unwrap();
                                                dest_file.write_all(&chunk).unwrap();
                                                let new = std::cmp::min(pb.position() + (chunk.len() as u64), total_size);
                                                pb.set_position(new);
                                            }
                                            filetime::set_file_handle_times(
                                                &dest_file,
                                                None,
                                                Some(filetime::FileTime::from_system_time(mtime.into())),
                                            )
                                            .unwrap();
                                        }
                                        // move tmp file to expected path
                                        std::fs::rename(&tmp_path, &expected_path).unwrap();
                                    };
                                    runtime.block_on(future);
                                }
                            }
                        }
                    }
                    let active = active_cnt.fetch_sub(1, Ordering::SeqCst);
                    if active == 1 {
                        // only self is active before this
                        break;
                    } else {
                        // sleep and wait for waking up
                        debug!("Sleep and wait for waking up");
                        loop {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            let old_wake = wake.load(Ordering::SeqCst);
                            if old_wake > 0 {
                                let new_wake = old_wake - 1;
                                if wake
                                    .compare_exchange(
                                        old_wake,
                                        new_wake,
                                        Ordering::SeqCst,
                                        Ordering::SeqCst,
                                    )
                                    .is_ok()
                                {
                                    break;
                                }
                            }
                        }
                    }
                }
                info!("This thread finished");
            });
        }
    });

    let mut exit_code = 0;

    // Removing files that are not in remote list
    let mut del_cnt = 0;
    let remote_list = remote_list.lock().unwrap();
    if failure_listing.load(Ordering::SeqCst) {
        error!("Failed to list remote, not to delete anything");
        exit_code = 1;
    } else {
        for entry in walkdir::WalkDir::new(download_dir).contents_first(true) {
            let entry = entry.unwrap();
            let path = entry.path();
            if !remote_list.contains(&path.to_path_buf()) {
                if !args.dry_run && !args.no_delete {
                    // always make sure that we are deleting the right thing
                    if del_cnt >= args.max_delete {
                        info!("Exceeding max delete count, aborting");
                        // exit with 25 to indicate that the deletion has been aborted
                        // this is the same as rsync
                        exit_code = 25;
                        break;
                    }
                    del_cnt += 1;
                    assert!(path.starts_with(download_dir));
                    info!("Deleting {:?}", path);
                    if entry.file_type().is_dir() {
                        if let Err(e) = std::fs::remove_dir(path) {
                            error!("Failed to remove {:?}: {:?}", path, e);
                            exit_code = 4;
                        }
                    } else if let Err(e) = std::fs::remove_file(path) {
                        error!("Failed to remove {:?}: {:?}", path, e);
                        exit_code = 4;
                    }
                } else {
                    info!("{:?} not in remote", path);
                }
            }
        }
    }

    if failure_downloading.load(Ordering::SeqCst) {
        error!("Failed to download some files");
        exit_code = 2;
    }

    // Show stat
    info!(
        "(Estimated) Total objects: {}, total size: {}",
        stat_objects.load(Ordering::SeqCst),
        humansize::format_size(stat_size.load(Ordering::SeqCst), humansize::BINARY)
    );

    std::process::exit(exit_code);
}
