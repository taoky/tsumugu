use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};

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

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

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
