use anyhow::anyhow;
use anyhow::Result;
use chrono::{DateTime, Utc};

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
