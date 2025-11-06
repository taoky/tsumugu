use anyhow::Result;
use chrono::FixedOffset;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use futures_util::Future;
use tracing::debug;
use tracing::warn;
use tsumugu_parser::regex_manager::{get_exclusion_manager_v1, get_exclusion_manager_v2};
use url::Url;

use crate::CommonArgs;

// A simple diagnose of (frustrating) proxy settings
fn proxy_precheck() {
    // follows the order of get_from_environment() in reqwest src/proxy.rs
    let mut http_proxy = std::env::var("HTTP_PROXY");
    if http_proxy.is_err() {
        http_proxy = std::env::var("http_proxy");
    }
    let mut https_proxy = std::env::var("HTTPS_PROXY");
    if https_proxy.is_err() {
        https_proxy = std::env::var("https_proxy");
    }
    let mut all_proxy = std::env::var("ALL_PROXY");
    if all_proxy.is_err() {
        all_proxy = std::env::var("all_proxy");
    }
    if http_proxy.is_err() && https_proxy.is_err() && all_proxy.is_err() {
        debug!("No proxy environment is given.");
        return;
    }
    fn check_format(s: &str) {
        if s.starts_with("socks://") {
            warn!("Typo in proxy env detected: use socks5:// or socks5h:// protocol please.");
            return;
        }
        if !s.starts_with("http://")
            && !s.starts_with("https://")
            && !s.starts_with("socks5://")
            && !s.starts_with("socks5h://")
        {
            warn!("Unknown protocol in proxy env, this might be silently ignored by reqwest.");
            return;
        }
        let url = match Url::parse(s) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to parse proxy URL {}: {}", s, e);
                return;
            }
        };
        if url.scheme() == "socks5" || url.scheme() == "socks5h" {
            // extra check for hostname resolve ability
            if let Err(e) = url.socket_addrs(|| Some(1080)) {
                warn!("Failed to get socket addr from {}: {}", url, e);
                warn!("This might be silently ignored later.");
                return;
            }
        }
        debug!("Seems OK with this proxy URL: {}", url);
    }
    if let Ok(s) = http_proxy {
        check_format(&s);
    }
    if let Ok(s) = https_proxy {
        check_format(&s);
    }
    if let Ok(s) = all_proxy {
        check_format(&s);
    }
}

// Helper structs for custom header support
#[derive(Debug, Clone)]
pub struct Header {
    pub name: reqwest::header::HeaderName,
    pub value: reqwest::header::HeaderValue,
}

pub fn headers_to_headermap(value: &[Header]) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
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

        let header_name =
            reqwest::header::HeaderName::from_str(name).map_err(|_| HeaderParseError)?;
        let header_value =
            reqwest::header::HeaderValue::from_str(value).map_err(|_| HeaderParseError)?;

        Ok(Header {
            name: header_name,
            value: header_value,
        })
    }
}

pub(crate) fn get_exclusion_manager(args: &CommonArgs) -> Box<dyn tsumugu_parser::regex_manager::ExclusionManagerTrait> {
    let exclusion_manager = if args.exclusion_v2 {
        let args = std::env::args().collect::<Vec<_>>();
        get_exclusion_manager_v2(&args)
    } else {
        get_exclusion_manager_v1(&args.exclude, &args.include)
    };
    exclusion_manager
}

pub(crate) fn build_client(
    args: &CommonArgs,
    redirect: bool,
    bind_address: Option<&String>,
    auto_compress: bool,
) -> reqwest::Client {
    proxy_precheck();
    let minute = std::time::Duration::new(60, 0);
    let mut builder = reqwest::Client::builder()
        .user_agent(args.user_agent.clone())
        .local_address(bind_address.map(|x| x.parse::<std::net::IpAddr>().unwrap()))
        .default_headers(args.headers())
        // hard code 1min connect/read timeout currently
        .connect_timeout(minute)
        .read_timeout(minute)
        .gzip(auto_compress)
        .brotli(auto_compress)
        .deflate(auto_compress);
    if !redirect {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    builder.build().unwrap()
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

pub(crate) fn is_symlink(path: &std::path::Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub(crate) fn naive_to_utc(naive: &chrono::NaiveDateTime, timezone: Option<FixedOffset>) -> DateTime<Utc> {
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
}
