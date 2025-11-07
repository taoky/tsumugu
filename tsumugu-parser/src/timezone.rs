use crate::listing::FileType;
use crate::parser;
use crate::parser::{ListResult, ParserMux};
use crate::regex_manager::{Comparison, ExclusionManagerTrait};
use crate::utils::relative_to_str;
use crate::utils::{again, relative_str_process};

use tsumugu_net::client::{HttpClient, RequestType};

use anyhow::{bail, Result};
use chrono::{DateTime, FixedOffset, Utc};
use tracing::{debug, info};
use url::Url;

pub fn determinate_timezone(
    upstream: &Url,
    timezone: Option<i32>,
    timezone_file: Option<&str>,
    retries: usize,
    parser: &ParserMux,
    exclusion_manager: &dyn ExclusionManagerTrait,
    client: &dyn HttpClient,
) -> Option<FixedOffset> {
    match timezone {
        None => {
            // Check if to guess timezone
            // Some parsers like directory-lister, requires special handling for URL --
            // we cannot deduce "base" from file URL. Most normal websites work like this
            // File http://example.com/d1/f1 => Listing http://example.com/d1/
            // However directory-lister:
            // File https://example.com/d1/f1 => Listing https://example.com/?dir=d1/
            // So we have to remember the listing URL here, too
            let timezone_base_and_url = match timezone_file {
                Some(f) => {
                    if f == "no" {
                        None
                    } else {
                        // Currently timezone_file could be given from CLI
                        // In this case, we still use the old logic to "guess" the listing
                        // by setting the base (listing) url to None
                        Some((None, Url::parse(f).expect("Invalid timezone file URL")))
                    }
                }
                None => {
                    // eek, try getting first file in root index
                    fn find_first_file(
                        parser: &ParserMux,
                        client: &dyn HttpClient,
                        url: &Url,
                        retries: usize,
                        relative: Vec<String>,
                        exclusion_manager: &dyn ExclusionManagerTrait,
                    ) -> Option<(Option<Url>, Url)> {
                        let relative_str = relative_to_str(&relative, None);
                        if exclusion_manager.match_str(&relative_str) == Comparison::Stop {
                            info!("Excluded by exclusion manager: {}", relative_str);
                            return None;
                        }
                        info!("Try finding first File in {}", url);
                        let list = again(|| parser.get_list_with_filter(client, url, &relative_str), retries)
                            .unwrap_or_else(|_| panic!("Failed to get list for {}. Maybe you shall disable timezone guessing?", url));
                        match list {
                            ListResult::List(list) => {
                                if let Some(item) = list.iter().find(|x| x.type_ == FileType::File)
                                {
                                    info!("Find a file! URL: {}", item.url);
                                    return Some((Some(url.clone()), item.url.clone()));
                                }
                                for item in list.iter().filter(|x| x.type_ == FileType::Directory) {
                                    let mut relative = relative.clone();
                                    relative.push(item.name.clone());
                                    if let Some(res) = find_first_file(
                                        parser,
                                        client,
                                        &item.url,
                                        retries,
                                        relative,
                                        exclusion_manager,
                                    ) {
                                        return Some(res);
                                    }
                                }
                                None
                            }
                            ListResult::Redirect(_) => {
                                info!("Get a manual redirect instead of a file");
                                None
                            }
                        }
                    }
                    find_first_file(
                        parser,
                        client,
                        upstream,
                        retries,
                        [].to_vec(),
                        exclusion_manager,
                    )
                }
            };
            match timezone_base_and_url {
                Some((timezone_base_url, timezone_url)) => {
                    let timezone = guess_remote_timezone(
                        parser,
                        client,
                        upstream,
                        timezone_base_url,
                        timezone_url,
                    )
                    .expect("Failed to guess timezone");
                    info!("Guessed timezone: {:?}", timezone);
                    Some(timezone)
                }
                None => None,
            }
        }
        Some(tz) => {
            info!("Using timezone from argument: {:?} hrs", tz);
            Some(FixedOffset::east_opt(tz * 3600).unwrap())
        }
    }
}

fn guess_remote_timezone(
    parser: &ParserMux,
    client: &dyn HttpClient,
    upstream: &Url,
    base_url: Option<Url>,
    file_url: Url,
) -> Result<FixedOffset> {
    assert!(!file_url.as_str().ends_with('/'));
    // trim after the latest '/'
    // TODO: improve this

    let file_url_str = file_url.as_str();
    let base_url = match base_url {
        Some(b) => b,
        None => Url::parse(&file_url_str[..=file_url_str.rfind('/').unwrap()]).unwrap(),
    };
    let relative = base_url.path().strip_prefix(upstream.path()).unwrap();
    let relative = relative_str_process(relative);
    debug!("get {relative} as relative for parser in guess remote timezone");

    info!("base: {:?}", base_url);
    info!("file: {:?}", file_url);

    let list = parser.get_list_with_filter(client, &base_url, &relative)?;
    let list = match list {
        parser::ListResult::Redirect(_) => {
            anyhow::bail!("Redirection not supported");
        }
        parser::ListResult::List(list) => list,
    };
    debug!("{:?}", list);
    for item in list {
        if item.url == file_url {
            // access file_url with HEAD
            let mtime = client
                .head_with_type(&item.url, RequestType::Download)?
                .modified_time?;

            // compare how many hours are there between mtime (FixedOffset) and item.mtime (Naive)
            // assuming that Naive one is UTC
            let unknown_mtime = DateTime::<Utc>::from_naive_utc_and_offset(item.mtime, Utc);
            let offset = unknown_mtime - mtime;
            let offset_minutes = offset.num_minutes();
            let hrs = (offset_minutes as f64 / 60.0).round() as i32;

            let minute_delta = (hrs as i64 * 60 - offset_minutes).abs();
            if minute_delta > 20 {
                bail!("File mtime got from parser and response does not match.");
            }

            // Construct timezone by hrs
            let timezone = FixedOffset::east_opt(hrs * 3600).ok_or(anyhow::anyhow!(
                "Cannot convert to timezone (offset hour = {hrs})."
            ))?;
            info!(
                "html time: {:?}, head time: {:?}, timezone: {:?}",
                item.mtime, mtime, timezone
            );
            return Ok(timezone);
        }
    }
    anyhow::bail!("File not found")
}
