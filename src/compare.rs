use std::path::Path;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use tracing::{debug, warn};

use crate::list::{FileType, ListItem};

pub fn compare_filetype(fstype: std::fs::FileType, tsumugu_type: &FileType) -> bool {
    match tsumugu_type {
        FileType::File => fstype.is_file(),
        FileType::Directory => fstype.is_dir(),
    }
}

pub fn should_download(
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
    if local_size != remote.size.unwrap_or(0) {
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
