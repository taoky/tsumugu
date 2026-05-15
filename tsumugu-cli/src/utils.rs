use std::os::unix::ffi::OsStrExt;

use anyhow::Result;
use chrono::FixedOffset;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use futures_util::Future;
use http::{HeaderMap, HeaderName, HeaderValue};
use tracing::debug;
use tracing::warn;
use tsumugu_parser::regex_manager::{get_exclusion_manager_v1, get_exclusion_manager_v2};

use crate::CommonArgs;

// Helper structs for custom header support
#[derive(Debug, Clone)]
pub struct Header {
    pub name: HeaderName,
    pub value: HeaderValue,
}

pub fn headers_to_headermap(value: &[Header]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for header in value.iter() {
        headers.insert(header.name.clone(), header.value.clone());
    }
    headers
}

#[derive(Debug)]
pub struct HeaderParseError;

impl std::fmt::Display for HeaderParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to parse header")
    }
}

impl std::error::Error for HeaderParseError {}

impl std::str::FromStr for Header {
    type Err = HeaderParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();

        let name = parts[0].trim();
        let value = parts[1].trim();

        if parts.len() != 2 {
            return Err(HeaderParseError);
        }

        let header_name = HeaderName::from_str(name).map_err(|_| HeaderParseError)?;
        let header_value = HeaderValue::from_str(value).map_err(|_| HeaderParseError)?;

        Ok(Header {
            name: header_name,
            value: header_value,
        })
    }
}

pub(crate) fn get_exclusion_manager(
    args: &CommonArgs,
) -> Box<dyn tsumugu_parser::regex_manager::ExclusionManagerTrait> {
    if args.exclusion_v2 {
        let args = std::env::args().collect::<Vec<_>>();
        get_exclusion_manager_v2(&args)
    } else {
        get_exclusion_manager_v1(&args.exclude, &args.include)
    }
}

pub(crate) async fn again_async<T, Fut, F: Fn() -> Fut>(f: F, retry: usize) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let mut count = 0;
    loop {
        match f().await {
            Ok(x) => return Ok(x),
            Err(e) => {
                warn!("Error: {:?}, retrying {}/{}", e, count, retry);
                count += 1;
                if count > retry {
                    return Err(e);
                }
            }
        }
    }
}

fn strip_path_trailing_slashes(path: &std::path::Path) -> std::borrow::Cow<'_, std::path::Path> {
    let bytes = path.as_os_str().as_bytes();
    let mut end = bytes.len();

    while end > 1 && bytes[end - 1] == b'/' {
        end -= 1;
    }

    if end == bytes.len() {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
            &bytes[..end],
        )))
    }
}

pub(crate) fn is_symlink(path: &std::path::Path) -> bool {
    // trailing slash of dir shall be removed, otherwise symlink_metadata still gets resolved result.
    let path = strip_path_trailing_slashes(path);

    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or_else(|e| {
            debug!("cannot get if {:?} is symlink or not: {}", path, e);
            false
        })
}

pub(crate) fn naive_to_utc(
    naive: &chrono::NaiveDateTime,
    timezone: Option<FixedOffset>,
) -> DateTime<Utc> {
    match timezone {
        None => DateTime::<Utc>::from_naive_utc_and_offset(*naive, Utc),
        Some(timezone) => timezone.from_local_datetime(naive).unwrap().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test]
    fn test_naive_to_utc() {
        let naive =
            chrono::NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap();
        let timezone = FixedOffset::east_opt(3600 * 8);
        let utc = naive_to_utc(&naive, timezone);
        assert_eq!(utc.to_string(), "2020-12-31 16:00:00 UTC");
        let utc = naive_to_utc(&naive, None);
        assert_eq!(utc.to_string(), "2021-01-01 00:00:00 UTC");
    }

    #[test]
    fn test_path_strip_trailing_slashes() {
        let path = std::path::Path::new("/");
        assert_eq!(
            strip_path_trailing_slashes(path)
                .as_os_str()
                .to_str()
                .unwrap(),
            "/"
        );
        let path = std::path::Path::new("/bin/");
        assert_eq!(
            strip_path_trailing_slashes(path)
                .as_os_str()
                .to_str()
                .unwrap(),
            "/bin"
        );
        let path = std::path::Path::new("/bin");
        assert_eq!(
            strip_path_trailing_slashes(path)
                .as_os_str()
                .to_str()
                .unwrap(),
            "/bin"
        );
        let path = std::path::Path::new("/bin////////");
        assert_eq!(
            strip_path_trailing_slashes(path)
                .as_os_str()
                .to_str()
                .unwrap(),
            "/bin"
        );
    }
}
