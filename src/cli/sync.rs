use std::{
    collections::HashSet,
    fs::File,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use anyhow::Result;
use chrono::{FixedOffset, NaiveDateTime};
use crossbeam_deque::{Injector, Worker};
use futures_util::StreamExt;
use reqwest::StatusCode;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::{
    bar::set_progress_bar,
    compare::{should_download_by_header, should_download_by_list},
    extensions::{extension_handler, ExtensionPackage},
    listing::{self, ListItem},
    parser::{ListResult, ParserMux},
    regex_process::{self, ExclusionManager},
    timezone::determinate_timezone,
    utils::{
        self, again, again_async, build_client, get_async, head, is_symlink, naive_to_utc,
        relative_to_str,
    },
    AsyncContext, SyncArgs,
};

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

fn worker_add_task(worker: &Worker<Task>, wake: &AtomicUsize, task: Task) {
    worker.push(task);
    wake.fetch_add(1, Ordering::SeqCst);
}

fn extension_push_task(worker: &Worker<Task>, wake: &AtomicUsize, package: &ExtensionPackage) {
    worker_add_task(
        worker,
        wake,
        Task {
            task: TaskType::Download(ListItem {
                url: package.url.clone(),
                name: package.filename.clone(),
                type_: listing::FileType::File,
                // size and mtime would be ignored as skip_check is set
                size: None,
                mtime: NaiveDateTime::default(),
                timezone: None,
                skip_check: true,
            }),
            relative: package.relative.clone(),
            url: package.url.clone(),
        },
    );
}

fn download_file(
    task_context: &TaskContext,
    item: &ListItem,
    path: &Path,
    args: &SyncArgs,
    bar: &kyuri::Bar,
    cwd: &Path,
    check_header: bool,
    compare_size_only: bool,
) -> Result<()> {
    let async_context = task_context.async_context;
    let timezone = task_context.timezone;
    let client = &async_context.download_client;
    let runtime = &async_context.runtime;
    // Here we use async to allow streaming and progress bar
    // Ref: https://gist.github.com/giuliano-oliveira/4d11d6b3bb003dba3a1b53f43d81b30d
    let future = again_async(
        || async {
            let url = item.url.clone();
            let resp = match get_async(client, url.clone()).await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Failed to GET {}: {:?}", url, e);
                    return Err(e.into());
                }
            };
            if check_header && !should_download_by_header(path, &resp, compare_size_only) {
                warn!("Skipping {} (GET header matches local file)", url);
                return Ok(());
            }
            let total_size = match resp.content_length() {
                Some(s) => s,
                None => {
                    warn!("URL {} does not give a content length", url);
                    // This value would be used only for showing progress bar.
                    0
                }
            };
            set_progress_bar(bar, total_size, &url);

            let mtime = match utils::get_response_mtime(&resp) {
                Ok(mtime) => mtime,
                Err(e) => {
                    if args.allow_mtime_from_parser {
                        naive_to_utc(&item.mtime, timezone)
                    } else {
                        error!("Failed to get mtime of {}: {:?}", url, e);
                        return Err(e);
                    }
                }
            };
            debug!("Get mtime to set for {:?}: {}", path, mtime);

            let tmp_path = cwd.join(format!(".tmp.{}", item.name));
            {
                let mut dest_file = File::create(&tmp_path).unwrap();
                let mut stream = resp.bytes_stream();

                while let Some(item) = stream.next().await {
                    let chunk = match item {
                        Ok(i) => i,
                        Err(e) => {
                            error!("Failed when downloading {}: {:?}", url, e);
                            return Err(e.into());
                        }
                    };
                    dest_file.write_all(&chunk).unwrap();
                    let new = std::cmp::min(bar.get_pos() + (chunk.len() as u64), total_size);
                    bar.set_pos(new);
                }
                filetime::set_file_handle_times(
                    &dest_file,
                    None,
                    Some(filetime::FileTime::from_system_time(mtime.into())),
                )
                .unwrap();
            }
            // move tmp file to expected path
            std::fs::rename(&tmp_path, path).unwrap();
            bar.finish();
            bar.set_visible(false);
            Ok(())
        },
        args.retry,
    );
    runtime.block_on(future)
}

struct ThreadsContext<'a> {
    bind_address: Option<String>,
    download_dir: &'a Path,
    remote_list: &'a Arc<Mutex<HashSet<PathBuf>>>,
    stat_objects: &'a AtomicUsize,
    stat_size: &'a AtomicU64,
    failure_listing: &'a AtomicBool,
    failure_downloading: &'a AtomicBool,
}

struct TaskContext<'a> {
    task: &'a Task,
    cwd: &'a Path,
    /// current dir, or dir of current file
    relative: &'a Vec<String>,
    worker: &'a Worker<Task>,
    wake: &'a AtomicUsize,
    exclusion_result: regex_process::Comparison,
    exclusion_manager: &'a ExclusionManager,
    timezone: Option<FixedOffset>,
    async_context: &'a AsyncContext,
}

fn list_handler(
    args: &SyncArgs,
    parser: &ParserMux,
    thr_context: &ThreadsContext,
    task_context: &TaskContext,
) {
    let task = task_context.task;
    let cwd = task_context.cwd;
    info!("Listing {}", task.url);
    {
        thr_context
            .remote_list
            .lock()
            .unwrap()
            .insert(cwd.to_path_buf());
    }

    if is_symlink(cwd) && !task_context.relative.is_empty() {
        info!("{:?} is a symlink, ignored", cwd);
        return;
    }

    let relative = &relative_to_str(task_context.relative, None);
    let items = match again(
        || Ok(parser.get_list_with_filter(task_context.async_context, &task.url, relative)?),
        args.retry,
    ) {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to list {}: {:?}", task.url, e);
            thr_context.failure_listing.store(true, Ordering::SeqCst);
            return;
        }
    };
    match items {
        ListResult::List(items) => {
            for item in items {
                if item.type_ == listing::FileType::Directory {
                    let mut relative = task.relative.clone();
                    relative.push(item.name);
                    worker_add_task(
                        task_context.worker,
                        task_context.wake,
                        Task {
                            task: TaskType::Listing,
                            relative,
                            url: item.url,
                        },
                    );
                } else {
                    if task_context.exclusion_result == regex_process::Comparison::ListOnly {
                        // Even though the dir is ListOnly, it could be possible that the file itself under dir is "included".
                        // So we need to check again...
                        let relative_filepath =
                            relative_to_str(task_context.relative, Some(&item.name));
                        if !(task_context.exclusion_manager.match_str(&relative_filepath)
                            == regex_process::Comparison::Ok)
                        {
                            info!("Skipping (by list only) {}", item.url);
                            continue;
                        }
                    }
                    worker_add_task(
                        task_context.worker,
                        task_context.wake,
                        Task {
                            task: TaskType::Download(item.clone()),
                            relative: task.relative.clone(),
                            url: item.url,
                        },
                    );
                    thr_context.stat_size.fetch_add(
                        match item.size {
                            Some(size) => size.get_estimated(),
                            None => 0,
                        },
                        Ordering::SeqCst,
                    );
                }
                thr_context.stat_objects.fetch_add(1, Ordering::SeqCst);
            }
        }
        ListResult::Redirect(target_url) => {
            // This "Redirect" only supports creating symlink of current directory
            info!(
                "Redirected {} -> {}. Try to create a symlink",
                task.url, target_url
            );
            if cwd.exists() {
                warn!("Skipping symlink creation because {:?} already exists, but it is not a symlink", cwd);
                return;
            }
            // get last segment of target_url
            let target_name = match target_url.split('/').nth_back(1) {
                Some(name) => name,
                None => {
                    error!("Failed to get last segment of target_url: {}", target_url);
                    return;
                }
            };
            info!("Try symlink {:?} -> {}", cwd, target_name);
            if let Err(e) = symlink(target_name, cwd) {
                error!(
                    "Failed to create symlink {:?} -> {}: {:?}",
                    cwd, target_name, e
                );
            }
        }
    }
}

fn download_handler(
    item: &ListItem,
    args: &SyncArgs,
    thr_context: &ThreadsContext,
    task_context: &TaskContext,
    bar: &kyuri::Bar,
) {
    let task = task_context.task;
    let cwd = task_context.cwd;
    // create path in case for first sync
    if !args.dry_run {
        std::fs::create_dir_all(cwd).unwrap();
    }
    // Absolute filesystem path of expected file
    let expected_path = cwd.join(&item.name);
    // Here relative filepath is only used to check exclusion
    let relative_filepath = relative_to_str(task_context.relative, Some(&item.name));
    debug!(
        "expected_path: {:?}, relative: {:?}",
        expected_path, relative_filepath
    );

    // We should put relative filepath into exclusion manager here
    if task_context.exclusion_manager.match_str(&relative_filepath)
        == regex_process::Comparison::Stop
    {
        // This should be run before inserting remote_list.
        // Otherwise newly excluded files will not be deleted later.
        info!("Skipping excluded {:?}", &relative_filepath);
        return;
    }

    {
        if !thr_context
            .remote_list
            .lock()
            .unwrap()
            .insert(expected_path.clone())
        {
            // It is possible that multiple tasks might download the same file
            // (generated by apt/yum parser, etc.)
            // skip when we find that some threads has already downloaded it
            info!("Skipping already handled {:?}", &expected_path);
            return;
        }
    }

    let mut should_download = true;
    let mut skip_if_exists = false;
    for i in &args.skip_if_exists {
        if i.is_match(&relative_filepath) {
            skip_if_exists = true;
            break;
        }
    }

    // Following code requires real filesystem path (expected_path) to work
    if !should_download_by_list(
        &expected_path,
        item,
        task_context.timezone,
        skip_if_exists,
        false,
    ) {
        info!("Skipping {}", task.url);
        should_download = false;
    }

    let mut compare_size_only = false;
    for i in &args.compare_size_only {
        if i.is_match(&relative_filepath) {
            compare_size_only = true;
            break;
        }
    }

    if should_download && args.head_before_get {
        match again(
            || {
                Ok(head(
                    &task_context.async_context.runtime,
                    &task_context.async_context.download_client,
                    item.url.clone(),
                )?)
            },
            args.retry,
        ) {
            Ok(resp) => {
                if !should_download_by_header(&expected_path, &resp, compare_size_only) {
                    info!("Skipping (by HEAD) {}", task.url);
                    should_download = false;
                }
            }
            Err(e) => {
                error!("Failed to HEAD {}: {:?}", task.url, e);
                thr_context
                    .failure_downloading
                    .store(true, Ordering::SeqCst);
                should_download = false;
            }
        };
    }

    if should_download && !args.dry_run {
        if let Err(e) = download_file(
            task_context,
            item,
            &expected_path,
            args,
            bar,
            cwd,
            // If no sending HEAD before GET, and don't take mtime from parser, check header here
            !args.head_before_get && !args.allow_mtime_from_parser,
            // compare_size_only to give to should_download_by_header() inside (when check_header is true)
            compare_size_only,
        ) {
            let mut set_error = true;
            if args.ignore_nonexist {
                if let Some(reqwest_err) = e.downcast_ref::<reqwest::Error>() {
                    if reqwest_err.status() == Some(StatusCode::NOT_FOUND) {
                        set_error = false;
                    }
                }
            }
            if set_error {
                thr_context
                    .failure_downloading
                    .store(true, Ordering::SeqCst);
            }
        }
    } else if should_download {
        info!("Dry run, not downloading {}", task.url);
    }

    extension_handler(args, &expected_path, &task.relative, &item.url, |package| {
        extension_push_task(task_context.worker, task_context.wake, package);
    });
}

fn sync_threads(args: &SyncArgs, parser: &ParserMux, thr_context: &ThreadsContext) {
    let exclusion_manager = ExclusionManager::new(&args.exclude, &args.include);

    // Handling listing
    let listing_client = build_client(
        args,
        parser.is_auto_redirect(),
        thr_context.bind_address.as_ref(),
        // some servers (such as download.zerotier.com) would give you gzipped list even if you don't ask for that,
        // so just enable auto compression when requesting listing
        true,
    );
    // Handling download
    let download_client = build_client(
        args,
        true,
        thr_context.bind_address.as_ref(),
        // auto compression is set to off here, as is known that some servers would wrongly report Content-Encoding for compressed files
        // like cloud.centos.org
        false,
    );
    // async runtime support
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let async_context = AsyncContext {
        download_client,
        listing_client,
        runtime,
    };

    let progressbar_manager = kyuri::Manager::new(std::time::Duration::from_secs(1));
    progressbar_manager.set_ticker(true);

    let timezone = determinate_timezone(args, parser, &async_context);

    if !args.dry_run {
        std::fs::create_dir_all(thr_context.download_dir).unwrap();
    }

    let workers: Vec<_> = (0..args.threads)
        .map(|_| Worker::<Task>::new_fifo())
        .collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let global = Injector::<Task>::new();

    global.push(Task {
        task: TaskType::Listing,
        relative: vec![],
        url: args.upstream.clone(),
    });

    let active_cnt = AtomicUsize::new(0);
    let wake = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for worker in workers {
            scope.spawn(|| {
                let bar = progressbar_manager.create_bar(0, "", "", false);
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
                        let relative = relative_to_str(&task.relative, None);
                        // When relative is used to join with cwd, the heading `/` shall be removed.
                        let cwd = thr_context.download_dir.join(&relative[1..]);
                        debug!("cwd: {:?}, relative: {:?}", cwd, relative);
                        // exclude this?
                        // note that it only checks the relative folder!
                        // Downloading files will still be checked again.
                        let exclusion_result = exclusion_manager.match_str(&relative);
                        if exclusion_result == regex_process::Comparison::Stop {
                            info!("Skipping excluded {:?}", &relative);
                            continue;
                        } else if exclusion_result == regex_process::Comparison::ListOnly {
                            info!("List only in {:?}", &relative);
                        }
                        let task_context = TaskContext {
                            task: &task,
                            cwd: &cwd,
                            relative: &task.relative,
                            worker: &worker,
                            wake: &wake,
                            exclusion_result,
                            exclusion_manager: &exclusion_manager,
                            timezone,
                            async_context: &async_context,
                        };
                        match &task.task {
                            TaskType::Listing => {
                                list_handler(args, parser, thr_context, &task_context);
                            }
                            TaskType::Download(item) => {
                                download_handler(item, args, thr_context, &task_context, &bar);
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
                            // If nobody is active and no wake signal is active, exit
                            let active = active_cnt.load(Ordering::SeqCst);
                            if old_wake == 0 && active == 0 {
                                info!("No longer wait, as nobody is alive and nothing to do");
                                break;
                            }
                        }
                    }
                }
                info!("This thread finished");
                // drop worker to let rustc know it moves inside the closure
                std::mem::drop(worker);
            });
        }
    });
}

pub fn sync(args: &SyncArgs, bind_address: Option<String>) -> ! {
    debug!("{:?}", args);
    let parser = ParserMux::new(
        args.parser.clone(),
        args.parser_match.clone(),
        args.auto_fallback,
    );

    let download_dir = args.local.as_path();

    let remote_list = Arc::new(Mutex::new(HashSet::new()));

    let stat_objects = AtomicUsize::new(0);
    let stat_size = AtomicU64::new(0);

    let failure_listing = AtomicBool::new(false);
    let failure_downloading = AtomicBool::new(false);

    sync_threads(
        args,
        &parser,
        &ThreadsContext {
            bind_address,
            download_dir,
            remote_list: &remote_list,
            stat_objects: &stat_objects,
            stat_size: &stat_size,
            failure_listing: &failure_listing,
            failure_downloading: &failure_downloading,
        },
    );

    let mut exit_code = 0;

    // Removing files that are not in remote list
    let mut del_cnt = 0;
    let remote_list = remote_list.lock().unwrap();
    if failure_listing.load(Ordering::SeqCst) {
        error!("Failed to list remote, not to delete anything");
        exit_code = 1;
    } else {
        // Don't even walkdir when dry_run, to prevent no dir error
        for entry in walkdir::WalkDir::new(download_dir).contents_first(true) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    error!("Failed to walkdir: {:?}", e);
                    if !args.dry_run {
                        exit_code = 1;
                    }
                    break;
                }
            };
            let path = entry.path();
            if !remote_list.contains(&path.to_path_buf()) {
                if args.no_delete {
                    info!("{:?} not in remote", path);
                } else {
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
                    if args.dry_run {
                        info!("Dry run, not deleting {:?}", path);
                        continue;
                    }

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
