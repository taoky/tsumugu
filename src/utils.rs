use anyhow::anyhow;
use anyhow::Result;
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
