use anyhow::anyhow;
use anyhow::Result;
use chrono::FixedOffset;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use futures_util::Future;
use tracing::debug;
use tracing::warn;
use url::Url;

use crate::SharedArgs;

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
            warn!("Unknown protcol in proxy env, this might be silently ignored by reqwest.");
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

pub fn build_client(
    args: impl SharedArgs,
    redirect: bool,
    bind_address: Option<&String>,
    auto_compress: bool,
) -> reqwest::Client {
    proxy_precheck();
    let minute = std::time::Duration::new(60, 0);
    let mut builder = reqwest::Client::builder()
        .user_agent(args.user_agent())
        .local_address(bind_address.map(|x| x.parse::<std::net::IpAddr>().unwrap()))
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

pub fn get_response_mtime(resp: &reqwest::Response) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc2822(
        resp.headers()
            .get("Last-Modified")
            .ok_or(anyhow!("Last-Modified header not found"))?
            .to_str()?,
    )?
    .with_timezone(&Utc))
}

pub fn again<T>(closure: impl Fn() -> Result<T>, retry: usize) -> Result<T> {
    let mut count = 0;
    loop {
        match closure() {
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

pub async fn again_async<T, Fut, F: Fn() -> Fut>(f: F, retry: usize) -> Result<T>
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

pub async fn get_async(
    client: &reqwest::Client,
    url: Url,
) -> Result<reqwest::Response, reqwest::Error> {
    client.get(url).send().await?.error_for_status()
}

pub async fn head_async(
    client: &reqwest::Client,
    url: Url,
) -> Result<reqwest::Response, reqwest::Error> {
    client.head(url).send().await?.error_for_status()
}

pub fn get(
    runtime: &tokio::runtime::Runtime,
    client: &reqwest::Client,
    url: Url,
) -> Result<reqwest::Response, reqwest::Error> {
    let future = async { get_async(client, url).await };
    runtime.block_on(future)
}

pub fn get_text(
    runtime: &tokio::runtime::Runtime,
    response: reqwest::Response,
) -> Result<String, reqwest::Error> {
    let future = async { response.text().await };
    runtime.block_on(future)
}

pub fn head(
    runtime: &tokio::runtime::Runtime,
    client: &reqwest::Client,
    url: Url,
) -> Result<reqwest::Response, reqwest::Error> {
    let future = async { head_async(client, url).await };
    runtime.block_on(future)
}

pub fn is_symlink(path: &std::path::Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn naive_to_utc(naive: &chrono::NaiveDateTime, timezone: Option<FixedOffset>) -> DateTime<Utc> {
    match timezone {
        None => DateTime::<Utc>::from_naive_utc_and_offset(*naive, Utc),
        Some(timezone) => timezone.from_local_datetime(naive).unwrap().into(),
    }
}

pub fn relative_str_process(relative: &str) -> String {
    let mut r = relative.to_string();
    if r.starts_with('/') {
        warn!("unexpected / at the beginning of relative ({r})");
    } else {
        r.insert(0, '/');
    }
    if r.len() != 1 {
        if r.ends_with('/') {
            warn!("unexpected / at the end of relative ({r})")
        } else {
            r.push('/')
        }
    }
    r
}

pub fn relative_to_str(relative: &[String], filename: Option<&str>) -> String {
    let r = relative.join("/");
    let r = relative_str_process(&r);

    // here r already has / at the end
    match filename {
        None => r,
        Some(filename) => {
            assert!(!filename.starts_with('/') && !filename.ends_with('/'));
            format!("{}{}", r, filename)
        }
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
    fn test_relative() {
        let mut relative: Vec<String> = vec![];
        assert_eq!(relative_to_str(&relative, None), "/");
        relative.push("debian".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/");
        relative.push("dists".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/dists/");
        relative.push("bookworm".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/dists/bookworm/");
        assert_eq!(
            relative_to_str(&relative, Some("Release")),
            "/debian/dists/bookworm/Release"
        );
    }
}
