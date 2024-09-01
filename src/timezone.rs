use crate::listing::FileType;
use crate::parser::{ListResult, ParserMux};
use crate::utils::head;
use crate::utils::{self, again};
use crate::AsyncContext;
use crate::{parser, SyncArgs};

use anyhow::Result;
use chrono::{DateTime, FixedOffset, Utc};
use tracing::{debug, info};
use url::Url;

pub fn determinate_timezone(
    args: &SyncArgs,
    parser: &ParserMux,
    async_context: &AsyncContext,
) -> Option<FixedOffset> {
    match args.timezone {
        None => {
            // Check if to guess timezone
            // Some parsers like directory-lister, requires special handling for URL --
            // we cannot deduce "base" from file URL. Most normal websites work like this
            // File http://example.com/d1/f1 => Listing http://example.com/d1/
            // However directory-lister:
            // File https://example.com/d1/f1 => Listing https://example.com/?dir=d1/
            // So we have to remember the listing URL here, too
            let timezone_base_and_url = match &args.timezone_file {
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
                        args: &SyncArgs,
                        parser: &ParserMux,
                        async_context: &AsyncContext,
                        url: &Url,
                        relative: Vec<String>,
                    ) -> Option<(Option<Url>, Url)> {
                        info!("Try finding first File in {}", url);
                        let relative_str = relative.join("/");
                        let list = again(|| Ok(parser.get_list_with_filter(async_context, url, &relative_str)?), args.retry)
                            .unwrap_or_else(|_| panic!("Failed to get list for {}. Maybe you shall disable timezone guessing?", url));
                        match list {
                            ListResult::List(list) => {
                                for item in list {
                                    match item.type_ {
                                        FileType::File => {
                                            info!("Find a file! URL: {}", item.url);
                                            return Some((Some(url.clone()), item.url));
                                        }
                                        FileType::Directory => {
                                            let mut relative = relative.clone();
                                            relative.push(item.name);
                                            if let Some(res) = find_first_file(
                                                args,
                                                parser,
                                                async_context,
                                                &item.url,
                                                relative,
                                            ) {
                                                return Some(res);
                                            }
                                        }
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
                    find_first_file(args, parser, async_context, &args.upstream, [].to_vec())
                }
            };
            match timezone_base_and_url {
                Some((timezone_base_url, timezone_url)) => {
                    let timezone = guess_remote_timezone(
                        parser,
                        async_context,
                        &args.upstream,
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
    async_context: &AsyncContext,
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
    debug!("get {relative} as relative for parser in guess remote timezone");

    info!("base: {:?}", base_url);
    info!("file: {:?}", file_url);

    let list = parser.get_list_with_filter(async_context, &base_url, relative)?;
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
            let resp = head(
                &async_context.runtime,
                &async_context.download_client,
                file_url,
            )?;
            let mtime = utils::get_response_mtime(&resp)?;

            // compare how many hours are there between mtime (FixedOffset) and item.mtime (Naive)
            // assuming that Naive one is UTC
            let unknown_mtime = DateTime::<Utc>::from_naive_utc_and_offset(item.mtime, Utc);
            let offset = unknown_mtime - mtime;
            let hrs = (offset.num_minutes() as f64 / 60.0).round() as i32;

            // Construct timezone by hrs
            let timezone = FixedOffset::east_opt(hrs * 3600).unwrap();
            info!(
                "html time: {:?}, head time: {:?}, timezone: {:?}",
                item.mtime, mtime, timezone
            );
            return Ok(timezone);
        }
    }
    anyhow::bail!("File not found")
}
