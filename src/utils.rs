use anyhow::anyhow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Response;

pub fn get_response_mtime(resp: &Response) -> Result<DateTime<Utc>> {
    let mtime = resp
        .headers()
        .get("Last-Modified")
        .ok_or(anyhow!("Last-Modified header not found"))?;
    Ok(DateTime::parse_from_rfc2822(mtime.to_str()?)?.with_timezone(&Utc))
}
