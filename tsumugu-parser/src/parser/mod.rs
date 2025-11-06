use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, bail, Result};
use clap::ValueEnum;
use tracing::{info, warn};
use url::Url;

use crate::regex_manager::ExpandedRegex;

use crate::{client::HttpClient, listing::ListItem};

pub mod apache_f2;
pub mod caddy;
pub mod denoflare_r2;
pub mod directory_lister;
pub mod docker;
pub mod fallback;
pub mod fancyindex;
pub mod gradle;
pub mod lighttpd;
pub mod nginx;
pub mod s3indexbuilder;

#[derive(Debug)]
pub enum ListResult {
    List(Vec<ListItem>),
    Redirect(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ParserError {
    // TODO: now reqwest::Error is not returned by parsers because they do not use reqwest
    // #[error("network error: {0}")]
    // NetworkError(#[from] reqwest::Error),
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

impl_parser_error!(url::ParseError, chrono::ParseError, regex::Error);

pub trait Parser: Sync {
    fn get_list(&self, client: &dyn HttpClient, url: &Url) -> Result<ListResult, ParserError>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str;

    /// Some parsers (directiory lister) might have different URL path
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
    DenoflareR2,
    S3Indexbuilder,

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
            Self::DenoflareR2 => Box::<denoflare_r2::DenoFlareR2ListingParser>::default(),
            Self::S3Indexbuilder => Box::<s3indexbuilder::S3Indexbuilder>::default(),
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
        client: &dyn HttpClient,
        url: &Url,
        relative: &str,
    ) -> Result<ListResult, ParserError> {
        let res = {
            for s in self.supplementaries.iter() {
                let regex = &s.1;
                if regex.is_match(relative) {
                    info!("URL {} Matches subparser {}", url, s.0.name());
                    return s.0.get_list(client, url);
                }
            }
            self.main.get_list(client, url)
        };
        if !self.auto_fallback {
            return res;
        }
        let e = match res {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };
        // start autofallback logic
        warn!("Parse error with {url}: {e}, try fallback...");
        ParserType::Fallback.build().get_list(client, url)
    }

    pub fn get_path(&self, url: &Url) -> PathBuf {
        self.main.get_path(url)
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
        "URL for listing should have a trailing slash: {}",
        url.as_str()
    );
}

fn get_last_part_from_href(href: &str) -> &str {
    // Remove trailing slashes for correct name extraction.
    let trimmed = href.trim_end_matches('/');

    // Find the position of the last '/' and take the substring after it.
    let last_slash_pos = trimmed.rfind('/').map(|pos| pos + 1).unwrap_or(0);
    &trimmed[last_slash_pos..]
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

    let name: String = percent_encoding::percent_decode_str(name)
        .decode_utf8()
        .unwrap_or_else(|_| {
            warn!("Failed to decode percent-encoded string: {}", name);
            std::borrow::Cow::Borrowed(name)
        })
        .to_string();
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

fn is_space_before_timezone(s: &str) -> bool {
    if s.len() < 6 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    chars[chars.len() - 6] == ' '
}

fn has_colon_timezone_suffix(s: &str) -> bool {
    // such as +08:00
    if s.len() < 6 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    let c1 = chars[chars.len() - 3] == ':';
    let c2 = chars[chars.len() - 2..].iter().all(|c| c.is_ascii_digit());
    let c3 = chars[chars.len() - 5..chars.len() - 3]
        .iter()
        .all(|c| c.is_ascii_digit());
    let c4 = chars[chars.len() - 6] == '+' || chars[chars.len() - 6] == '-';
    c1 && c2 && c3 && c4
}

fn is_space_before_colon_timezone(s: &str) -> bool {
    // such as +08:00
    if s.len() < 7 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    chars[chars.len() - 7] == ' '
}

fn is_space_abbr_dmy(s: &str) -> bool {
    let mut it = s.split_whitespace();
    let d = match it.next() {
        Some(x) => x,
        None => return false,
    };
    let m = match it.next() {
        Some(x) => x,
        None => return false,
    };
    let y_raw = match it.next() {
        Some(x) => x,
        None => return false,
    };
    let y = y_raw.trim_end_matches(',');

    d.len() == 2
        && d.chars().all(|c| c.is_ascii_digit())
        && m.len() == 3
        && m.chars().all(|c| c.is_ascii_alphabetic())
        && y.len() == 4
        && y.chars().all(|c| c.is_ascii_digit())
}

// Parsing NodeJS page, to bypass limitation of %b (which must be 3-letter month)
fn date_normalization(date: &str) -> String {
    date.replace("Sept", "Sep")
}

// Returns format and regex string
fn guess_date_fmt(date: &str) -> (String, String) {
    let two_colons = contains_two_colons(date);
    let abbr_month = contains_abbreviated_month(date);
    let year_first = has_four_numeric_prefix(date);
    let has_timezone = has_timezone_suffix(date);
    let space_before_timezone = is_space_before_timezone(date);
    let has_colon_timezone = has_colon_timezone_suffix(date);
    let space_before_colon_timezone = is_space_before_colon_timezone(date);
    let abbr_space_dmy = is_space_abbr_dmy(date);
    let (dfmt, dfmt_regex) = if abbr_space_dmy {
        ("%d %b %Y", r"\d{2} \w{3} \d{4}")
    } else {
        match (abbr_month, year_first) {
            (true, true) => ("%Y-%b-%d", r"\d{4}-\w{3}-\d{2}"),
            (true, false) => ("%d-%b-%Y", r"\d{2}-\w{3}-\d{4}"),
            (false, true) => ("%Y-%m-%d", r"\d{4}-\d{2}-\d{2}"),
            (false, false) => ("%d-%m-%Y", r"\d{2}-\d{2}-\d{4}"),
        }
    };
    let (tfmt, tfmt_regex) = if two_colons {
        ("%H:%M:%S", r"\d{2}:\d{2}:\d{2}")
    } else {
        ("%H:%M", r"\d{2}:\d{2}")
    };

    let (dt_sep, dt_sep_regex) = if date.contains(',') {
        (", ", r", ")
    } else {
        (" ", " ")
    };

    let (zfmt, zfmt_regex) = if has_timezone {
        if space_before_timezone {
            (" %z", r" [+-]\d{4}")
        } else {
            ("%z", r"[+-]\d{4}")
        }
    } else if has_colon_timezone {
        if space_before_colon_timezone {
            (" %:z", r" [+-]\d{2}:\d{2}")
        } else {
            ("%:z", r"[+-]\d{2}:\d{2}")
        }
    } else {
        ("", "")
    };
    (
        format!("{}{}{}{}", dfmt, dt_sep, tfmt, zfmt),
        format!("{}{}{}{}", dfmt_regex, dt_sep_regex, tfmt_regex, zfmt_regex),
    )
}

fn date_fmt_has_timezone(datefmt: &str) -> bool {
    datefmt.contains("%z")
}

#[cfg(test)]
mod tests {
    use crate::client::HttpResponse;

    use super::*;

    pub(crate) struct TokioClient {
        client: reqwest::Client,
        runtime: tokio::runtime::Runtime,
    }

    pub(crate) fn init_client() -> TokioClient {
        TokioClient {
            client: reqwest::Client::new(),
            runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    impl HttpClient for TokioClient {
        fn head_with_type(
            &self,
            url: &Url,
            _req_type: crate::client::RequestType,
        ) -> anyhow::Result<HttpResponse> {
            let future = async {
                let resp = self.client.head(url.clone()).send().await?;
                let status_code = resp.status().as_u16();
                let final_url = resp.url().clone();
                let headers = resp.headers().clone();
                let content_length = resp.content_length();
                let modified_time = crate::utils::last_modified_from_header(&headers);
                Ok(HttpResponse {
                    body: String::new(),
                    final_url,
                    status_code,
                    headers,
                    content_length,
                    modified_time,
                })
            };
            self.runtime.block_on(future)
        }

        fn get_text_with_type(
            &self,
            url: &Url,
            _req_type: crate::client::RequestType,
        ) -> anyhow::Result<HttpResponse> {
            let future = async {
                let resp = self
                    .client
                    .get(url.clone())
                    .send()
                    .await?
                    .error_for_status()?;
                let status_code = resp.status().as_u16();
                let final_url = resp.url().clone();
                let headers = resp.headers().clone();
                let content_length = resp.content_length();
                let body = resp.text().await?;
                let modified_time = crate::utils::last_modified_from_header(&headers);
                Ok(HttpResponse {
                    body,
                    final_url,
                    status_code,
                    headers,
                    modified_time,
                    content_length,
                })
            };
            self.runtime.block_on(future)
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
        assert_eq!(
            guess_date_fmt("13-Feb-2023 04:21"),
            (
                "%d-%b-%Y %H:%M".to_owned(),
                r"\d{2}-\w{3}-\d{4} \d{2}:\d{2}".to_owned()
            )
        );
        assert_eq!(
            guess_date_fmt("2023-11-27 14:22:08 +00:00"),
            (
                "%Y-%m-%d %H:%M:%S %:z".to_owned(),
                r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [+-]\d{2}:\d{2}".to_owned()
            )
        );
        assert_eq!(
            guess_date_fmt("2023-11-27 14:22:08+00:00"),
            (
                "%Y-%m-%d %H:%M:%S%:z".to_owned(),
                r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}".to_owned()
            )
        );
        assert_eq!(
            guess_date_fmt("24 Sep 2025, 13:10"),
            (
                "%d %b %Y, %H:%M".to_owned(),
                r"\d{2} \w{3} \d{4}, \d{2}:\d{2}".to_owned()
            )
        )
    }

    #[test]
    fn test_date_normalization() {
        assert_eq!(
            date_normalization("15-Sept-2024 09:46"),
            "15-Sep-2024 09:46"
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
        assert_eq!(get_real_name_from_href("memtest+"), "memtest+");
    }
}
