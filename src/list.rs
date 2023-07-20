// Module for handling directory listing

use std::fmt::Display;

use anyhow::Result;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use reqwest::blocking::Client;
use tracing::{debug, info};
use url::Url;

use crate::parser;
use crate::utils;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub url: Url,
    pub name: String,
    pub type_: FileType,
    pub size: Option<u64>,
    // mtime is parsed from HTML, which is the local datetime of the "server" (not necessarily localtime or UTC)
    pub mtime: NaiveDateTime,
}

impl Display for ListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size_str = match self.size {
            Some(size) => size.to_string(),
            None => String::from("(none)"),
        };
        let mtime_str = self.mtime.format("%Y-%m-%d %H:%M:%S").to_string();
        write!(
            f,
            "{} {:?} {} {} {}",
            self.url, self.type_, size_str, mtime_str, self.name
        )
    }
}

pub fn guess_remote_timezone(
    parser: &Box<dyn parser::Parser>,
    client: &Client,
    file_url: Url,
) -> Result<FixedOffset> {
    assert!(!file_url.as_str().ends_with('/'));
    // trim after the latest '/'
    // TODO: improve this
    let file_url_str = file_url.as_str();
    let base_url = Url::parse(&file_url_str[..file_url_str.rfind('/').unwrap() + 1]).unwrap();

    info!("base: {:?}", base_url);
    info!("file: {:?}", file_url);

    let list = parser.get_list(client, &base_url)?;
    debug!("{:?}", list);
    for item in list {
        if item.url == file_url {
            // access file_url with HEAD
            let resp = client.head(file_url).send()?;
            let mtime = utils::get_blocking_response_mtime(&resp)?;

            // compare how many hours are there between mtime (FixedOffset) and item.mtime (Naive)
            // assuming that Naive one is UTC
            let unknown_mtime = DateTime::<Utc>::from_utc(item.mtime, Utc);
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
    Err(anyhow::anyhow!("File not found"))
}
