use std::{
    collections::HashSet,
    fs::File,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use clap::Parser;
use crossbeam_deque::{Injector, Worker};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use url::Url;

mod list;
mod parser;
use list::ListItem;

use crate::{compare::should_download, parser::Parser as HTMLParser};

mod compare;
mod utils;

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
#[clap(about, version)]
struct Args {
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    #[clap(long)]
    dry_run: bool,
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

    let args = Args::parse();

    let client = reqwest::blocking::Client::builder()
        .user_agent(args.user_agent.clone())
        .build()
        .unwrap();
    let parser = parser::nginx::NginxListingParser::default();

    // async support
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_client = reqwest::Client::builder()
        .user_agent(args.user_agent)
        .build()
        .unwrap();

    let timezone = list::guess_remote_timezone(
        &parser,
        &client,
        Url::parse("https://mirrors.ustc.edu.cn/monitoring-plugins/timestamp").unwrap(),
    );
    let timezone = match timezone {
        Ok(tz) => Some(tz),
        Err(e) => {
            warn!("Failed to guess timezone: {:?}", e);
            None
        }
    };
    info!("Guessed timezone: {:?}", timezone);

    let download_dir = Path::new("/tmp/tsumugu/");
    std::fs::create_dir_all(download_dir).unwrap();

    let remote_list = Arc::new(Mutex::new(HashSet::new()));

    const THREADS_NUM: usize = 2;
    let workers: Vec<_> = (0..THREADS_NUM)
        .map(|_| Worker::<Task>::new_fifo())
        .collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let global = Injector::<Task>::new();

    global.push(Task {
        task: TaskType::Listing,
        relative: vec![],
        url: Url::parse("https://mirrors.ustc.edu.cn/monitoring-plugins/").unwrap(),
    });

    let active_cnt = AtomicUsize::new(0);
    let wake = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for worker in workers.into_iter() {
            let stealers = &stealers;
            let parser = parser.clone();
            let client = client.clone();
            let global = &global;

            let active_cnt = &active_cnt;
            let wake = &wake;

            let remote_list = remote_list.clone();

            let async_client = async_client.clone();
            let runtime = &runtime;
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
                        let cwd = download_dir.join(task.relative.join("/"));
                        debug!("CWD: {:?}", cwd);
                        match task.task {
                            TaskType::Listing => {
                                info!("Listing {}", task.url);
                                {
                                    remote_list.lock().unwrap().insert(cwd.clone());
                                }
                                let items = parser.get_list(&client, &task.url).unwrap();
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
                                        worker.push(Task {
                                            task: TaskType::Download(item.clone()),
                                            relative: task.relative.clone(),
                                            url: item.url,
                                        });
                                        wake.fetch_add(1, Ordering::SeqCst);
                                    }
                                }
                            }
                            TaskType::Download(item) => {
                                // create path in case for first sync
                                std::fs::create_dir_all(&cwd).unwrap();
                                let expected_path = cwd.join(&item.name);
                                {
                                    remote_list.lock().unwrap().insert(expected_path.clone());
                                }
                                if !should_download(&expected_path, &item, timezone) {
                                    info!("Skipping {}", task.url);
                                    continue;
                                }

                                if !args.dry_run {
                                    let future = async {
                                        // Here we use async to allow streaming and progress bar
                                        // Ref: https://gist.github.com/giuliano-oliveira/4d11d6b3bb003dba3a1b53f43d81b30d
                                        let resp = async_client
                                            .get(item.url.clone())
                                            .send()
                                            .await
                                            .unwrap()
                                            .error_for_status()
                                            .unwrap();
                                        let total_size = resp.content_length().unwrap();
                                        let pb = ProgressBar::new(total_size);
                                        pb.set_style(ProgressStyle::default_bar()
                                            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
                                            .progress_chars("#>-"));
                                        pb.set_message(format!("Downloading {}", item.url));

                                        let mtime = utils::get_async_response_mtime(&resp).unwrap();
                                        let mut dest_file = File::create(&expected_path).unwrap();
                                        let mut stream = resp.bytes_stream();
                                        // let mut downloaded_bytes = 0;

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

    // Removing files that are not in remote list
    for entry in walkdir::WalkDir::new(download_dir).contents_first(true) {
        let entry = entry.unwrap();
        let path = entry.path();
        if !remote_list.lock().unwrap().contains(&path.to_path_buf()) {
            info!("{:?} not in remote", path);
        }
    }
}
