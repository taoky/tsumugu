use anyhow::anyhow;
use anyhow::Result;
use chrono::FixedOffset;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use futures_util::Future;
use tracing::warn;
use url::Url;

macro_rules! get_resp_mtime {
    ($resp: expr) => {
        Ok(DateTime::parse_from_rfc2822(
            $resp
                .headers()
                .get("Last-Modified")
                .ok_or(anyhow!("Last-Modified header not found"))?
                .to_str()?,
        )?
        .with_timezone(&Utc))
    };
}

#[macro_export]
macro_rules! build_client {
    ($client: ty, $args: expr, $parser: expr, $bind_address: expr) => {{
        let mut builder = <$client>::builder()
            .user_agent($args.user_agent.clone())
            .local_address($bind_address.map(|x| x.parse::<std::net::IpAddr>().unwrap()));
        if !$parser.is_auto_redirect() {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }
        builder.build().unwrap()
    }};
}

pub fn get_async_response_mtime(resp: &reqwest::Response) -> Result<DateTime<Utc>> {
    get_resp_mtime!(resp)
}

pub fn get_blocking_response_mtime(resp: &reqwest::blocking::Response) -> Result<DateTime<Utc>> {
    get_resp_mtime!(resp)
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

pub async fn get_async(client: &reqwest::Client, url: Url) -> Result<reqwest::Response> {
    Ok(client.get(url).send().await?.error_for_status()?)
}

#[allow(dead_code)]
pub async fn head_async(client: &reqwest::Client, url: Url) -> Result<reqwest::Response> {
    Ok(client.head(url).send().await?.error_for_status()?)
}

pub fn get(client: &reqwest::blocking::Client, url: Url) -> Result<reqwest::blocking::Response> {
    Ok(client.get(url).send()?.error_for_status()?)
}

pub fn head(client: &reqwest::blocking::Client, url: Url) -> Result<reqwest::blocking::Response> {
    Ok(client.head(url).send()?.error_for_status()?)
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

#[cfg(test)]
mod tests {
    use super::*;

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
