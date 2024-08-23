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
    let name: String = url::form_urlencoded::parse(href.as_bytes())
        .map(|(k, v)| [k, v].concat())
        .collect();
    name.trim_end_matches('/').to_string()
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

fn has_numeric_prefix(s: &str) -> bool {
    s.chars().take(4).all(|c| c.is_ascii_digit()) && s.len() >= 4
}

// Returns format and regex string
fn guess_date_fmt(date: &str) -> (String, String) {
    let two_colons = contains_two_colons(date);
    let abbr_month = contains_abbreviated_month(date);
    let year_first = has_numeric_prefix(date);
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
    (
        format!("{} {}", dfmt, tfmt),
        format!("{} {}", dfmt_regex, tfmt_regex),
    )
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
    }
}
