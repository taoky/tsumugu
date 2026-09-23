use std::{
    collections::HashSet,
    fs::File,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use chrono::{FixedOffset, NaiveDateTime};
use crossbeam_deque::{Injector, Worker};
use tracing::{debug, error, info, warn};
use url::Url;

use tsumugu_parser::{
    extensions::{ExtensionPackage, extension_handler},
    listing::{self, ListItem},
    parser::{self, ListResult, ParserMux},
    regex_manager::{self, ExclusionManagerTrait},
    timezone::determinate_timezone,
    utils::{again, relative_to_str},
};

use tsumugu_net::client::{
    get_response_mtime,
    impls::{TokioHttpClient, build_client, error_status_code},
};

use crate::{
    SyncArgs,
    bar::set_progress_bar,
    compare::{should_download_by_header, should_download_by_list},
    utils::{again_async, get_exclusion_manager, is_symlink, naive_to_utc},
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

const KNOWN_METADATA_FILES: [&str; 11] = [
    // deb
    "Release",
    "Release.gpg",
    "InRelease",
    "Packages",
    "Packages.bz2",
    "Packages.gz",
    "Packages.xz",
    "Sources",
    "Sources.gz",
    "Sources.xz",
    // rpm
    "repomd.xml",
];

fn should_delay_update(args: &SyncArgs, item: &ListItem) -> bool {
    if args.delay_update {
        true
    } else if args.delay_update_metadata {
        KNOWN_METADATA_FILES.contains(&item.name.as_str())
    } else {
        false
    }
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
    do_rename: bool,
) -> Result<bool> {
    let http_client = task_context.client;
    let runtime = &http_client.runtime;
    let timezone = task_context.timezone;
    // Here we use async to allow streaming and progress bar
    // Ref: https://gist.github.com/giuliano-oliveira/4d11d6b3bb003dba3a1b53f43d81b30d
    let future = again_async(
        || async {
            let url = item.url.clone();
            let mut resp = match http_client.download(&url).await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Failed to GET {}: {:?}", url, e);
                    return Err(e);
                }
            };
            let http_resp = &resp.http_response;
            if check_header && !should_download_by_header(path, http_resp, compare_size_only) {
                warn!("Skipping {} (GET header matches local file)", url);
                return Ok(false);
            }
            let total_size = match http_resp.content_length {
                Some(s) => s,
                None => {
                    warn!("URL {} does not give a content length", url);
                    // This value would be used only for showing progress bar.
                    0
                }
            };
            set_progress_bar(bar, total_size, &url);

            let mtime = if args.trust_mtime_from_parser {
                naive_to_utc(&item.mtime, timezone)
            } else {
                match get_response_mtime(http_resp) {
                    Ok(mtime) => mtime,
                    Err(e) => {
                        let mtime = naive_to_utc(&item.mtime, timezone);
                        warn!(
                            "Failed to get mtime of {} from header, use parser mtime {} instead: {}",
                            url, mtime, e
                        );
                        mtime
                    }
                }
            };
            debug!("Get mtime to set for {:?}: {}", path, mtime);

            let tmp_path = cwd.join(format!(".tmp.{}", item.name));
            {
                let mut dest_file = File::create(&tmp_path).unwrap();

                while let Some(chunk) = resp.next_chunk().await? {
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
            if do_rename {
                std::fs::rename(&tmp_path, path).unwrap_or_else(|e| {
                    panic!(
                        "renaming from {:?} to {:?} shall never fail: {}",
                        tmp_path, path, e
                    )
                });
            }
            bar.finish();
            bar.set_visible(false);
            Ok(true)
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
    pb_manager: &'a kyuri::Manager,
    delayed_updates: &'a Arc<Mutex<Vec<PathBuf>>>,
}

impl ThreadsContext<'_> {
    fn mark_failure_listing(&self) {
        self.failure_listing.store(true, Ordering::SeqCst);
    }

    fn mark_failure_downloading(&self) {
        self.failure_downloading.store(true, Ordering::SeqCst);
    }
}

struct TaskContext<'a> {
    task: &'a Task,
    cwd: &'a Path,
    /// current dir, or dir of current file
    relative: &'a Vec<String>,
    worker: &'a Worker<Task>,
    wake: &'a AtomicUsize,
    exclusion_result: regex_manager::Comparison,
    exclusion_manager: &'a dyn ExclusionManagerTrait,
    timezone: Option<FixedOffset>,
    client: &'a TokioHttpClient,
}

fn should_set_error(args: &SyncArgs, e: &anyhow::Error) -> bool {
    if let Some(status) = error_status_code(e) {
        if args.ignore_nonexist && status == 404 {
            return false;
        }
        if args.ignore_forbidden && status == 403 {
            return false;
        }
        if args.ignore_status.contains(&status) {
            return false;
        }
    }
    true
}

// Check if the error should be counted as failure
fn parser_should_set_error(args: &SyncArgs, e: &parser::ParserError) -> bool {
    let e = match e {
        parser::ParserError::ParseError(_) => return true,
        parser::ParserError::NetworkError(parser::AnyNetworkError::Inner(e)) => e,
    };
    should_set_error(args, e)
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
        || parser.get_list_with_filter(task_context.client, &task.url, relative),
        args.retry,
    ) {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to list {}: {:?}", task.url, e);
            if parser_should_set_error(args, &e) {
                thr_context.mark_failure_listing();
            }
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
                    if task_context.exclusion_result == regex_manager::Comparison::ListOnly {
                        // Even though the dir is ListOnly, it could be possible that the file itself under dir is "included".
                        // So we need to check again...
                        let relative_filepath =
                            relative_to_str(task_context.relative, Some(&item.name));
                        if !(task_context.exclusion_manager.match_str(&relative_filepath)
                            == regex_manager::Comparison::Ok)
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
                warn!(
                    "Skipping symlink creation because {:?} already exists, but it is not a symlink",
                    cwd
                );
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
    if !args.dry_run
        && let Err(e) = std::fs::create_dir_all(cwd)
    {
        error!("Failed to create directory {:?}: {:?}", cwd, e);
        thr_context.mark_failure_downloading();
        return;
    }
    // Absolute filesystem path of expected file
    let expected_path = cwd.join(&item.name);
    // Here relative filepath is only used to check exclusion
    let relative_filepath = relative_to_str(task_context.relative, Some(&item.name));
    debug!(
        "expected_path: {:?}, relative: {:?}",
        expected_path, relative_filepath
    );
    if is_symlink(&expected_path) {
        info!("{:?} is a symlink, ignored", expected_path);
        return;
    }

    // We should put relative filepath into exclusion manager here
    if task_context.exclusion_manager.match_str(&relative_filepath)
        == regex_manager::Comparison::Stop
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
        match again(|| task_context.client.head_download(&task.url), args.retry) {
            Ok(resp) => {
                if !should_download_by_header(&expected_path, &resp, compare_size_only) {
                    info!("Skipping (by HEAD) {}", task.url);
                    should_download = false;
                }
            }
            Err(e) => {
                error!("Failed to HEAD {}: {:?}", task.url, e);
                thr_context.mark_failure_downloading();
                should_download = false;
            }
        };
    }

    if should_download && !args.dry_run {
        let should_delay = should_delay_update(args, item) && expected_path.exists();
        match download_file(
            task_context,
            item,
            &expected_path,
            args,
            bar,
            cwd,
            // If no sending HEAD before GET, and don't take mtime from parser, check header here
            !args.head_before_get && !args.trust_mtime_from_parser,
            // compare_size_only to give to should_download_by_header() inside (when check_header is true)
            compare_size_only,
            !should_delay,
        ) {
            Err(e) => {
                if should_set_error(args, &e) {
                    thr_context.mark_failure_downloading();
                }
            }
            Ok(true) if should_delay => {
                info!("Delaying update of {:?} to the end", expected_path);
                thr_context
                    .delayed_updates
                    .lock()
                    .unwrap()
                    .push(expected_path.clone());
            }
            Ok(_) => {}
        }
    } else if should_download {
        info!("Dry run, not downloading {}", task.url);
    }

    extension_handler(
        args.apt_packages,
        args.yum_packages,
        &expected_path,
        &task.relative,
        &item.url,
        |package| {
            extension_push_task(task_context.worker, task_context.wake, package);
        },
    );
}

fn sync_threads(
    args: &SyncArgs,
    parser: &ParserMux,
    thr_context: &ThreadsContext,
    exclusion_manager: &dyn ExclusionManagerTrait,
) {
    // Handling listing
    let listing_client = build_client(
        &args.user_agent,
        thr_context.bind_address.as_deref(),
        args.headers(),
        parser.is_auto_redirect(),
        // some servers (such as download.zerotier.com) would give you gzipped list even if you don't ask for that,
        // so just enable auto compression when requesting listing
        true,
    );
    // Handling download
    let download_client = build_client(
        &args.user_agent,
        thr_context.bind_address.as_deref(),
        args.headers(),
        true,
        // auto compression is set to off here, as is known that some servers would wrongly report Content-Encoding for compressed files
        // like cloud.centos.org
        false,
    );
    let client = TokioHttpClient::new(listing_client, download_client);

    let timezone = determinate_timezone(
        &args.upstream,
        args.timezone,
        args.timezone_file.as_deref(),
        args.retry,
        parser,
        exclusion_manager,
        &client,
    );

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
                let bar = thr_context.pb_manager.create_bar(0, "", "", false);
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
                        if exclusion_result == regex_manager::Comparison::Stop {
                            info!("Skipping excluded {:?}", &relative);
                            continue;
                        } else if exclusion_result == regex_manager::Comparison::ListOnly {
                            info!("List only in {:?}", &relative);
                        }
                        let task_context = TaskContext {
                            task: &task,
                            cwd: &cwd,
                            relative: &task.relative,
                            worker: &worker,
                            wake: &wake,
                            exclusion_result,
                            exclusion_manager,
                            timezone,
                            client: &client,
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

/// Check whether a local path (or any of its parent directories) is excluded,
/// so that cleanup shall keep it when `--exclude-no-delete` is set.
///
/// `is_dir` follows the same convention as syncing: directories get a relative
/// path with a trailing '/', while files do not.
fn is_delete_protected(
    exclusion_manager: &dyn ExclusionManagerTrait,
    download_dir: &Path,
    path: &Path,
    is_dir: bool,
) -> bool {
    let relative_components: Vec<String> = match path.strip_prefix(download_dir) {
        Ok(relative) => relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect(),
        Err(_) => {
            warn!("unexpected path {:?} outside of download dir", path);
            return false;
        }
    };
    if relative_components.is_empty() {
        // the download dir itself, never excluded here
        return false;
    }
    let (dir_components, filename) = if is_dir {
        (&relative_components[..], None)
    } else {
        (
            &relative_components[..relative_components.len() - 1],
            relative_components.last().map(|s| s.as_str()),
        )
    };
    if exclusion_manager.match_str(&relative_to_str(dir_components, filename))
        != regex_manager::Comparison::Ok
    {
        return true;
    }
    // An excluded directory protects its whole subtree from deletion (like rsync),
    // even when the exclusion regex does not match the children directly.
    for i in 1..relative_components.len() {
        if exclusion_manager.match_str(&relative_to_str(&relative_components[..i], None))
            != regex_manager::Comparison::Ok
        {
            return true;
        }
    }
    false
}

/// Remove local files and directories that are not in remote list.
/// Returns the exit code of this stage.
fn cleanup(
    args: &SyncArgs,
    download_dir: &Path,
    remote_list: &HashSet<PathBuf>,
    exclusion_manager: &dyn ExclusionManagerTrait,
) -> i32 {
    let mut exit_code = 0;
    let mut del_cnt = 0;
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
        if remote_list.contains(&path.to_path_buf()) {
            continue;
        }
        if args.exclude_no_delete
            && is_delete_protected(
                exclusion_manager,
                download_dir,
                path,
                entry.file_type().is_dir(),
            )
        {
            info!("{:?} not in remote, but excluded, keeping it", path);
            continue;
        }
        if args.no_delete {
            info!("{:?} not in remote", path);
            continue;
        }
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
    exit_code
}

pub(crate) fn sync(args: &SyncArgs, bind_address: Option<String>, pb_manager: kyuri::Manager) -> ! {
    debug!("{:?}", args);
    let parser = ParserMux::new(
        args.parser.clone(),
        args.parser_match.clone(),
        args.auto_fallback,
    );

    let download_dir = args.local.as_path();

    let exclusion_manager = get_exclusion_manager(args);

    let remote_list = Arc::new(Mutex::new(HashSet::new()));
    let delayed_updates = Arc::new(Mutex::new(Vec::new()));

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
            pb_manager: &pb_manager,
            delayed_updates: &delayed_updates,
        },
        &*exclusion_manager,
    );

    // Process delayed updates
    {
        let delayed_updates = delayed_updates.lock().unwrap();
        for path in delayed_updates.iter() {
            info!("Updating delayed file {:?}", path);
            let tmp_path = path.with_file_name(format!(
                ".tmp.{}",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::rename(&tmp_path, path).unwrap_or_else(|e| {
                panic!(
                    "renaming from {:?} to {:?} shall never fail: {}",
                    tmp_path, path, e
                )
            });
        }
    }

    // Removing files that are not in remote list
    let remote_list = remote_list.lock().unwrap();
    let mut exit_code = if failure_listing.load(Ordering::SeqCst) {
        error!("Failed to list remote, not to delete anything");
        1
    } else {
        cleanup(args, download_dir, &remote_list, &*exclusion_manager)
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use test_log::test;
    use tsumugu_parser::regex_manager::get_exclusion_manager_v2;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tsumugu-cleanup-test-{}-{}",
                std::process::id(),
                name
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Build a local tree and return the remote list containing only "current" files.
    ///
    /// The tree contains:
    /// - current/c.txt: in remote list, shall always be kept
    /// - frozen/a.txt, frozen/sub/b.txt, frozen/empty_sub/: excluded, not in remote list
    /// - skip.me: excluded by file pattern, not in remote list
    /// - stale/s.txt, stale_empty/, drop.me: not excluded, not in remote list
    fn build_tree(root: &Path) -> HashSet<PathBuf> {
        let file = |p: PathBuf| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"test").unwrap();
        };
        file(root.join("current/c.txt"));
        file(root.join("frozen/a.txt"));
        file(root.join("frozen/sub/b.txt"));
        std::fs::create_dir_all(root.join("frozen/empty_sub")).unwrap();
        file(root.join("skip.me"));
        file(root.join("stale/s.txt"));
        std::fs::create_dir_all(root.join("stale_empty")).unwrap();
        file(root.join("drop.me"));

        HashSet::from([
            root.to_path_buf(),
            root.join("current"),
            root.join("current/c.txt"),
        ])
    }

    fn test_args(root: &Path, excludes: &[&str], exclude_no_delete: bool) -> SyncArgs {
        let mut argv = vec!["tsumugu".to_string()];
        for exclude in excludes {
            argv.push("--exclude".to_string());
            argv.push(exclude.to_string());
        }
        if exclude_no_delete {
            argv.push("--exclude-no-delete".to_string());
        }
        argv.push("http://example.com/".to_string());
        argv.push(root.to_str().unwrap().to_string());
        SyncArgs::parse_from(argv)
    }

    fn assert_stale_removed(exit_code: i32, root: &Path) {
        assert_eq!(exit_code, 0);
        assert!(root.join("current/c.txt").exists());
        assert!(!root.join("stale").exists());
        assert!(!root.join("stale_empty").exists());
        assert!(!root.join("drop.me").exists());
    }

    fn assert_excluded_kept(root: &Path) {
        assert!(root.join("frozen/a.txt").exists());
        assert!(root.join("frozen/sub/b.txt").exists());
        assert!(root.join("frozen/empty_sub").exists());
        assert!(root.join("skip.me").exists());
    }

    #[test]
    fn test_cleanup_exclude_no_delete_v1() {
        let tmp = TempDir::new("v1-on");
        let remote_list = build_tree(&tmp.0);
        let args = test_args(&tmp.0, &["^/frozen/", r"/skip\.me$"], true);
        let manager = get_exclusion_manager(&args);
        let exit_code = cleanup(&args, &tmp.0, &remote_list, &*manager);
        assert_stale_removed(exit_code, &tmp.0);
        assert_excluded_kept(&tmp.0);
    }

    #[test]
    fn test_cleanup_default_still_deletes_excluded_v1() {
        let tmp = TempDir::new("v1-off");
        let remote_list = build_tree(&tmp.0);
        let args = test_args(&tmp.0, &["^/frozen/", r"/skip\.me$"], false);
        let manager = get_exclusion_manager(&args);
        let exit_code = cleanup(&args, &tmp.0, &remote_list, &*manager);
        assert_stale_removed(exit_code, &tmp.0);
        // without --exclude-no-delete, excluded paths are deleted as before
        assert!(!tmp.0.join("frozen").exists());
        assert!(!tmp.0.join("skip.me").exists());
    }

    #[test]
    fn test_cleanup_exclude_no_delete_v2() {
        let tmp = TempDir::new("v2-on");
        let remote_list = build_tree(&tmp.0);
        let args = test_args(&tmp.0, &["^/frozen/", r"/skip\.me$"], true);
        let argv = vec![
            "tsumugu".to_string(),
            "--exclude=^/frozen/".to_string(),
            r"--exclude=/skip\.me$".to_string(),
        ];
        let manager = get_exclusion_manager_v2(&argv);
        let exit_code = cleanup(&args, &tmp.0, &remote_list, &*manager);
        assert_stale_removed(exit_code, &tmp.0);
        assert_excluded_kept(&tmp.0);
    }

    #[test]
    fn test_cleanup_excluded_dir_protects_subtree() {
        let tmp = TempDir::new("anchored");
        let remote_list = build_tree(&tmp.0);
        // This regex matches the directory itself only, not its children.
        // The whole subtree shall still be kept, like rsync.
        let args = test_args(&tmp.0, &["^/frozen/$"], true);
        let manager = get_exclusion_manager(&args);
        let exit_code = cleanup(&args, &tmp.0, &remote_list, &*manager);
        assert_stale_removed(exit_code, &tmp.0);
        assert!(tmp.0.join("frozen/a.txt").exists());
        assert!(tmp.0.join("frozen/sub/b.txt").exists());
        assert!(tmp.0.join("frozen/empty_sub").exists());
        // skip.me is not excluded here
        assert!(!tmp.0.join("skip.me").exists());
    }
}
