use anyhow::{anyhow, Result};
use chrono::FixedOffset;
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use tracing::warn;

pub fn parse_last_modified(last_modified: &str) -> Result<DateTime<Utc>> {
    let last_modified = match DateTime::parse_from_rfc2822(last_modified) {
        Ok(res) => res,
        Err(_) => {
            // Maybe header is in format like "Monday, 02-Sep-2024 09:11:16 GMT"
            // Parse this type of datetime

            // Chrono does not support %Z, but according to rfc1945, it shall always GMT.
            let last_modified = last_modified.trim_end_matches(" GMT");
            let naive = NaiveDateTime::parse_from_str(last_modified, "%A, %d-%b-%Y %H:%M:%S")?;
            naive
                .and_local_timezone(FixedOffset::east_opt(0).unwrap())
                .single()
                .unwrap()
        }
    };
    Ok(last_modified.with_timezone(&Utc))
}

pub fn last_modified_from_header(headers: &http::HeaderMap) -> Result<DateTime<Utc>> {
    let last_modified = headers
        .get(http::header::LAST_MODIFIED)
        .ok_or(anyhow!("No Last-Modified header found in response"))?;
    let last_modified = last_modified.to_str()?;
    parse_last_modified(last_modified)
}

pub fn again<T, E: std::fmt::Debug, F: FnMut() -> Result<T, E>>(
    mut f: F,
    retries: usize,
) -> Result<T, E> {
    for attempt in 0..=retries {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt == retries => return Err(e),
            Err(e) => {
                warn!("Error: {:?}. retry {}/{}", e, attempt + 1, retries);
            }
        }
    }
    unreachable!()
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

    #[test]
    fn test_parse_last_modified() {
        assert_eq!(
            parse_last_modified("Monday, 02-Sep-2024 09:11:16 GMT").unwrap(),
            DateTime::parse_from_str("2024/09/02 09:11:16 +0000", "%Y/%m/%d %H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            parse_last_modified("Wed, 14 Aug 2024 07:02:10 GMT").unwrap(),
            DateTime::parse_from_str("2024/08/14 07:02:10 +0000", "%Y/%m/%d %H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }
}
