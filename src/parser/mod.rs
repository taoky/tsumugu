use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, bail, Result};
use clap::ValueEnum;
use tracing::{info, warn};
use url::Url;

use crate::regex_process::ExpandedRegex;
use crate::utils::{get, get_text};

use crate::{listing::ListItem, AsyncContext};

pub mod apache_f2;
pub mod caddy;
pub mod directory_lister;
pub mod docker;
pub mod fallback;
pub mod fancyindex;
pub mod gradle;
pub mod lighttpd;
pub mod nginx;

#[derive(Debug)]
pub enum ListResult {
    List(Vec<ListItem>),
    Redirect(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ParserError {
    #[error("network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    ParseError(#[from] anyhow::Error),
}

macro_rules! impl_parser_error {
    ($($ty:ty),*) => {
        $(
            impl From<$ty> for ParserError {
                fn from(value: $ty) -> Self {
                    ParserError::ParseError(anyhow::Error::from(value))
                }
            }
        )*
    };
}

impl_parser_error!(
    url::ParseError,
    reqwest::header::ToStrError,
    chrono::ParseError,
    regex::Error
);

pub trait Parser: Sync {
    fn get_list(&self, async_context: &AsyncContext, url: &Url) -> Result<ListResult, ParserError>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str;

    /// Used for list command only
    fn get_path(&self, url: &Url) -> PathBuf {
        PathBuf::from(url.path())
    }
}

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum ParserType {
    Nginx,
    ApacheF2,
    Docker,
    DirectoryLister,
    Lighttpd,
    Caddy,
    FancyIndex,
    Gradle,

    Fallback,
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
            Self::Fallback => Box::<fallback::FallbackParser>::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParserTypeMatch {
    parser_type: ParserType,
    regex: ExpandedRegex,
}

impl FromStr for ParserTypeMatch {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // split by first :
        let (p, r) = match s.split_once(':') {
            Some((l, r)) => {
                let p = ParserType::from_str(l, true).map_err(|s| anyhow!(s))?;
                let r = ExpandedRegex::from_str(r)?;
                (p, r)
            }
            None => bail!("No ':' in given ParserTypeMatch"),
        };
        Ok(ParserTypeMatch {
            parser_type: p,
            regex: r,
        })
    }
}

// A "combined" parser from main parser + supplementary parsers
pub struct ParserMux {
    main: Box<dyn Parser>,
    supplementaries: Vec<(Box<dyn Parser>, ExpandedRegex)>,
    auto_fallback: bool,
}

impl ParserMux {
    pub fn new(
        main_parser: ParserType,
        supplementary_parsers: Vec<ParserTypeMatch>,
        auto_fallback: bool,
    ) -> Self {
        if main_parser == ParserType::Fallback {
            warn!("Please reconsider: fallback parser SHOULD NOT be used as main parser.");
        }
        let main = main_parser.build();
        let supplementaries = supplementary_parsers
            .into_iter()
            .map(|s| (s.parser_type.build(), s.regex))
            .collect();
        ParserMux {
            main,
            supplementaries,
            auto_fallback,
        }
    }

    pub fn get_list_with_filter(
        &self,
        async_context: &AsyncContext,
        url: &Url,
        relative: &str,
    ) -> Result<ListResult, ParserError> {
        fn get_list_inner(
            s: &ParserMux,
            async_context: &AsyncContext,
            url: &Url,
            relative: &str,
        ) -> Result<ListResult, ParserError> {
            for s in s.supplementaries.iter() {
                let regex = &s.1;
                if regex.is_match(relative) {
                    info!("URL {} Matches subparser {}", url, s.0.name());
                    return s.0.get_list(async_context, url);
                }
            }
            s.main.get_list(async_context, url)
        }

        let res = get_list_inner(self, async_context, url, relative);
        if !self.auto_fallback {
            return res;
        }
        let e = match res {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };
        let e = match e {
            ParserError::NetworkError(_) => return Err(e),
            ParserError::ParseError(e) => e,
        };
        // start autofallback logic
        warn!("Parse error with {url}: {e}, try fallback...");
        ParserType::Fallback.build().get_list(async_context, url)
    }

    pub fn is_auto_redirect(&self) -> bool {
        let main_redirect = self.main.is_auto_redirect();
        for s in self.supplementaries.iter() {
            if s.0.is_auto_redirect() != main_redirect {
                warn!("Supplementary parsers do not have same redirect settings as main parser. Ignored.")
            }
        }
        main_redirect
    }
}

fn assert_if_url_has_no_trailing_slash(url: &Url) {
    assert!(
        url.path().ends_with('/'),
        "URL for listing should have a trailing slash"
    );
}

fn get_last_part_from_href(href: &str) -> &str {
    // Remove trailing slashes for correct name extraction.
    let trimmed = href.trim_end_matches('/');

    // Find the position of the last '/' and take the substring after it.
    let last_slash_pos = trimmed.rfind('/').map(|pos| pos + 1).unwrap_or(0);
    let after_last_slash = &trimmed[last_slash_pos..];

    return after_last_slash;
}

fn get_real_name_from_href(href: &str) -> String {
    let after_last_slash = get_last_part_from_href(href);

    // TODO: this might have issues (inconsistent with other impls)

    // Find the position of the first '?' and take the substring before it.
    let query_pos = after_last_slash.find('?').unwrap_or(after_last_slash.len());
    let before_query = &after_last_slash[..query_pos];

    // Also do this for '#'
    let hash_pos = before_query.find('#').unwrap_or(before_query.len());
    let name = &before_query[..hash_pos];

    let name: String = url::form_urlencoded::parse(name.as_bytes())
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

        assert_eq!(get_real_name_from_href("somefile#performance"), "somefile");
    }
}
