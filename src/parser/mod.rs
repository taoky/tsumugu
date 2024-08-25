use anyhow::Result;
use clap::ValueEnum;
use tracing::warn;
use url::Url;

use crate::utils::{get, get_text};

use crate::{listing::ListItem, AsyncContext};

pub mod apache_f2;
pub mod caddy;
pub mod directory_lister;
pub mod docker;
pub mod fancyindex;
pub mod gradle;
pub mod lighttpd;
pub mod nginx;

#[derive(Debug)]
pub enum ListResult {
    List(Vec<ListItem>),
    Redirect(String),
}

pub trait Parser: Sync {
    fn get_list(&self, async_context: &AsyncContext, url: &Url) -> Result<ListResult>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ParserType {
    Nginx,
    ApacheF2,
    Docker,
    DirectoryLister,
    Lighttpd,
    Caddy,
    FancyIndex,
    Gradle,
}

impl ParserType {
    pub fn build(&self) -> Box<dyn Parser> {
        match self {
            Self::Nginx => Box::<nginx::NginxListingParser>::default(),
            Self::ApacheF2 => Box::<apache_f2::ApacheF2ListingParser>::default(),
            Self::Docker => Box::<docker::DockerListingParser>::default(),
            Self::DirectoryLister => {
                warn!("html5ever parser does not support foster parenting. The result may be incorrect.");
                Box::<directory_lister::DirectoryListerListingParser>::default()
            }
            Self::Lighttpd => Box::<lighttpd::LighttpdListingParser>::default(),
            Self::Caddy => Box::<caddy::CaddyListingParser>::default(),
            Self::FancyIndex => Box::<fancyindex::FancyIndexListingParser>::default(),
            Self::Gradle => Box::<gradle::GradleListingParser>::default(),
        }
    }
}

fn assert_if_url_has_no_trailing_slash(url: &Url) {
    assert!(
        url.path().ends_with('/'),
        "URL for listing should have a trailing slash"
    );
}

fn get_real_name_from_href(href: &str) -> String {
    // Remove trailing slashes for correct name extraction.
    let trimmed = href.trim_end_matches('/');

    // Find the position of the last '/' and take the substring after it.
    let last_slash_pos = trimmed.rfind('/').map(|pos| pos + 1).unwrap_or(0);
    let after_last_slash = &trimmed[last_slash_pos..];

    // Find the position of the first '?' and take the substring before it.
    let query_pos = after_last_slash.find('?').unwrap_or(after_last_slash.len());
    let before_query = &after_last_slash[..query_pos];

    let name: String = url::form_urlencoded::parse(before_query.as_bytes())
        .map(|(k, v)| [k, v].concat())
        .collect();
    name
}

fn contains_abbreviated_month(s: &str) -> bool {
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    months.iter().any(|&month| s.contains(month))
}

fn contains_two_colons(s: &str) -> bool {
    s.matches(':').count() >= 2
}

fn has_four_numeric_prefix(s: &str) -> bool {
    s.chars().take(4).all(|c| c.is_ascii_digit()) && s.len() >= 4
}

fn has_timezone_suffix(s: &str) -> bool {
    if s.len() < 5 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    let c1 = chars[chars.len() - 4..].iter().all(|c| c.is_ascii_digit());
    let c2 = chars[chars.len() - 5] == '+' || chars[chars.len() - 5] == '-';

    c1 && c2
}

// Returns format and regex string
fn guess_date_fmt(date: &str) -> (String, String) {
    let two_colons = contains_two_colons(date);
    let abbr_month = contains_abbreviated_month(date);
    let year_first = has_four_numeric_prefix(date);
    let has_timezone = has_timezone_suffix(date);
    let (dfmt, dfmt_regex) = match (abbr_month, year_first) {
        (true, true) => ("%Y-%b-%d", r"\d{4}-\w{3}-\d{2}"),
        (true, false) => ("%d-%b-%Y", r"\d{2}-\w{3}-\d{4}"),
        (false, true) => ("%Y-%m-%d", r"\d{4}-\d{2}-\d{2}"),
        (false, false) => ("%d-%m-%Y", r"\d{2}-\d{2}-\d{4}"),
    };
    let (tfmt, tfmt_regex) = if two_colons {
        ("%H:%M:%S", r"\d{2}:\d{2}:\d{2}")
    } else {
        ("%H:%M", r"\d{2}:\d{2}")
    };
    let (zfmt, zfmt_regex) = if has_timezone {
        (" %z", r" [+-]\d{4}")
    } else {
        ("", "")
    };
    (
        format!("{} {}{}", dfmt, tfmt, zfmt),
        format!("{} {}{}", dfmt_regex, tfmt_regex, zfmt_regex),
    )
}

fn date_fmt_has_timezone(datefmt: &str) -> bool {
    datefmt.contains("%z")
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn init_async_context() -> AsyncContext {
        let client = reqwest::Client::new();
        AsyncContext {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            listing_client: client.clone(),
            download_client: client,
        }
    }

    #[test]
    fn test_guess_date_fmt() {
        assert_eq!(
            guess_date_fmt("2024-Jul-15 09:46"),
            (
                "%Y-%b-%d %H:%M".to_owned(),
                r"\d{4}-\w{3}-\d{2} \d{2}:\d{2}".to_owned()
            )
        );
        assert_eq!(
            guess_date_fmt("2023-11-27 14:22:08 +0000"),
            (
                "%Y-%m-%d %H:%M:%S %z".to_owned(),
                r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [+-]\d{4}".to_owned()
            )
        );
    }

    #[test]
    fn test_get_real_name_from_href() {
        assert_eq!(get_real_name_from_href("test/"), "test");
        assert_eq!(
            get_real_name_from_href("ceph-base_17.2.6-pve1%2B3.changelog"),
            "ceph-base_17.2.6-pve1+3.changelog"
        );
        assert_eq!(get_real_name_from_href("test?sort=name&order=asc"), "test");
        assert_eq!(get_real_name_from_href("/aaa/bbb"), "bbb");
    }
}
