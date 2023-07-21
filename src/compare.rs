use std::path::Path;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use tracing::{debug, warn};

use crate::{
    list::{FileType, ListItem, FileSize},
    utils,
};

pub fn compare_filetype(fstype: std::fs::FileType, tsumugu_type: &FileType) -> bool {
    match tsumugu_type {
        FileType::File => fstype.is_file(),
        FileType::Directory => fstype.is_dir(),
    }
}

pub fn should_download_by_list(
    path: &Path,
    remote: &ListItem,
    remote_timezone: Option<FixedOffset>,
) -> bool {
    let local_metadata = match path.metadata() {
        Ok(m) => m,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to get metadata of {:?}: {:?}", path, e);
            }
            return true;
        }
    };
    if !compare_filetype(local_metadata.file_type(), &remote.type_) {
        // TODO: delete old file which type is not correct
        warn!("Type mismatch: {:?} remote {:?}", path, remote.type_);
        return true;
    }
    let local_size = local_metadata.len();
    let is_size_match = match remote.size.unwrap_or(FileSize::Precise(0)) {
        FileSize::Precise(size) => {
            local_size == size
        }
        // A very rough size check is used here,
        // as it looks like size returned by server may not be very accurate
        FileSize::HumanizedBinary(size, unit) => {
            let base = 1024_f64.powf(unit.get_exp().into());
            let lsize = local_size as f64 / base as f64;
            (lsize - size).abs() < 2.0
        }
        FileSize::HumanizedDecimal(size, unit) => {
            let base = 1000_f64.powf(unit.get_exp().into());
            let lsize = local_size as f64 / base as f64;
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
    let local_mtime: DateTime<Utc> = match local_metadata.modified() {
        Ok(m) => m,
        Err(_) => {
            // Here we expect all fs to support mtime
            unreachable!()
        }
    }
    .into();
    match remote_timezone {
        None => {
            // treat remote as UTC
            let remote_mtime = DateTime::<Utc>::from_utc(remote.mtime, Utc);
            let offset = remote_mtime - local_mtime;
            // allow an offset to up to 24hrs
            debug!("DateTime offset: {:?} {:?}", path, offset);
            offset.num_hours().abs() > 24
        }
        Some(timezone) => {
            let remote_mtime = timezone.from_local_datetime(&remote.mtime).unwrap();
            let remote_mtime: DateTime<Utc> = remote_mtime.into();
            let offset = remote_mtime - local_mtime;
            // allow an offset up to 1min
            debug!("DateTime offset: {:?} {:?}", path, offset);
            offset.num_minutes().abs() > 1
        }
    }
}

pub fn should_download_by_head(path: &Path, resp: &reqwest::blocking::Response) -> bool {
    // Construct a valid "ListItem" and pass to should_download_by_list
    debug!("Checking {:?} by HEAD: {:?}", path, resp);
    let item = ListItem {
        url: resp.url().clone(),
        name: path.file_name().unwrap().to_str().unwrap().to_string(),
        type_: if resp.url().as_str().ends_with('/') {
            FileType::Directory
        } else {
            FileType::File
        },
        size: Some(FileSize::Precise(resp.content_length().unwrap())),
        mtime: utils::get_blocking_response_mtime(resp)
            .unwrap()
            .naive_utc(),
    };
    should_download_by_list(path, &item, FixedOffset::east_opt(0))
}
