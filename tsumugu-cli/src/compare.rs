use std::path::Path;

use chrono::{DateTime, FixedOffset, Utc};
use tracing::{debug, warn};

use tsumugu_parser::listing::{FileSize, FileType, ListItem, SizeUnit};

use crate::utils::naive_to_utc;

pub(crate) fn compare_filetype(fstype: std::fs::FileType, tsumugu_type: FileType) -> bool {
    match tsumugu_type {
        FileType::File => fstype.is_file(),
        FileType::Directory => fstype.is_dir(),
    }
}

pub(crate) fn should_download_by_list(
    path: &Path,
    remote: &ListItem,
    remote_timezone: Option<FixedOffset>,
    skip_if_exists: bool,
    size_only: bool,
) -> bool {
    let local_metadata = match path.metadata() {
        Ok(m) => {
            if skip_if_exists || remote.skip_check {
                debug!("Skipping {:?} because it exists", path);
                return false;
            }
            m
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to get metadata of {:?}: {:?}", path, e);
            }
            return true;
        }
    };
    if !compare_filetype(local_metadata.file_type(), remote.type_) {
        // TODO: delete old file which type is not correct
        warn!("Type mismatch: {:?} remote {:?}", path, remote.type_);
        return true;
    }
    let local_size = local_metadata.len();
    let is_size_match = match remote.size.unwrap_or(FileSize::Precise(0)) {
        FileSize::Precise(size) => local_size == size,
        FileSize::HumanizedBinary(size, SizeUnit::B) => {
            // SizeUnit::B is a special case, it means the size is in bytes
            local_size == size as u64
        }
        // A very rough size check is used here,
        // as it looks like size returned by server may not be very accurate
        FileSize::HumanizedBinary(size, unit) => {
            let base = 1024_f64.powf(unit.get_exp().into());
            let lsize = local_size as f64 / base;
            (lsize - size).abs() < 2.0
        }
        FileSize::HumanizedDecimal(size, unit) => {
            let base = 1000_f64.powf(unit.get_exp().into());
            let lsize = local_size as f64 / base;
            (lsize - size).abs() < 2.0
        }
    };
    if !is_size_match {
        debug!(
            "Size mismatch: {:?} local {:?} remote {:?}",
            path, local_size, remote.size
        );
        return true;
    }
    if size_only {
        return false;
    }
    let local_mtime: DateTime<Utc> = match local_metadata.modified() {
        Ok(m) => m,
        Err(_) => {
            // Here we expect all fs to support mtime
            unreachable!()
        }
    }
    .into();
    // Use remote timezone or not?
    let timezone = match remote.timezone {
        None => remote_timezone,
        Some(tz) => Some(tz),
    };
    let remote_mtime = naive_to_utc(&remote.mtime, timezone);
    let offset = remote_mtime - local_mtime;
    debug!("DateTime offset: {:?} {:?}", path, offset);
    match timezone {
        None => {
            // allow an offset to up to 24hrs
            offset.num_hours().abs() > 24
        }
        Some(_) => {
            // allow an offset up to 1min
            offset.num_minutes().abs() > 1
        }
    }
}

pub(crate) fn should_download_by_header(
    path: &Path,
    resp: &reqwest::Response,
    size_only: bool,
) -> bool {
    // Construct a valid "ListItem" and pass to should_download_by_list
    debug!("Checking {:?} by header: {:?}", path, resp);
    let item = ListItem {
        url: resp.url().clone(),
        name: path.file_name().unwrap().to_str().unwrap().to_string(),
        type_: if resp.url().as_str().ends_with('/') {
            FileType::Directory
        } else {
            FileType::File
        },
        size: Some(FileSize::Precise(match resp.content_length() {
            Some(l) => l,
            None => {
                warn!(
                    "No content-length from upstream ({}), go downloading anyway",
                    resp.url()
                );
                return true;
            }
        })),
        mtime: match tsumugu_parser::utils::get_response_mtime(resp) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "Cannot get mtime from {} ({}), go downloading anyway",
                    resp.url(),
                    e
                );
                return true;
            }
        }
        .naive_utc(),
        timezone: None,
        skip_check: false,
    };
    should_download_by_list(path, &item, FixedOffset::east_opt(0), false, size_only)
}
