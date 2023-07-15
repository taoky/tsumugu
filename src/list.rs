// Module for handling directory listing

use anyhow::Result;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use tracing::{debug, info};
use url::Url;

#[derive(Debug, PartialEq)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug)]
pub struct ListItem {
    pub url: String,
    name: String,
    pub type_: FileType,
    size: Option<u64>,
    // mtime is parsed from HTML, which is the local datetime of the "server" (not necessarily localtime or UTC)
    mtime: NaiveDateTime,
}

pub async fn get_list(client: &Client, url: &Url) -> Result<Vec<ListItem>> {
    let metadata_regex = Regex::new(r#"(\d{2}-\w{3}-\d{4} \d{2}:\d{2})\s+([\d-]+)$"#).unwrap();

    let body = client.get(url.as_str()).send().await?.text().await?;
    let document = Html::parse_document(&body);
    let selector = Selector::parse("a").unwrap();
    let mut items = Vec::new();
    for element in document.select(&selector) {
        let href = element.value().attr("href").unwrap();
        let href = url.join(href).unwrap();
        let href = href.as_str();
        let name = element.text().collect::<Vec<_>>().join("");
        if name == "../" {
            continue;
        }
        let type_ = if href.ends_with('/') {
            FileType::Directory
        } else {
            FileType::File
        };
        let metadata_raw = element
            .next_sibling()
            .unwrap()
            .value()
            .as_text()
            .unwrap()
            .to_string();
        let metadata_raw = metadata_raw.trim();
        // println!("{:?}", metadata_raw);
        let metadata = metadata_regex.captures(metadata_raw).unwrap();
        let date = metadata.get(1).unwrap().as_str();
        let date = NaiveDateTime::parse_from_str(date, "%d-%b-%Y %H:%M").unwrap();
        let size = metadata.get(2).unwrap().as_str().parse::<u64>().ok();
        // println!("{} {} {:?} {} {:?}", href, name, type_, date, size);
        items.push(ListItem {
            url: href.to_string(),
            name,
            type_,
            size,
            mtime: date,
        })
    }
    Ok(items)
}

pub async fn guess_remote_timezone(client: &Client, file_url: &Url) -> Result<FixedOffset> {
    assert!(!file_url.as_str().ends_with("/"));
    // trim after the latest '/'
    let file_url = file_url.as_str();
    let base_url = &file_url[..file_url.rfind('/').unwrap() + 1];

    info!("base: {:?}", base_url);
    info!("file: {:?}", file_url);

    let list = get_list(client, &Url::parse(base_url).unwrap()).await?;
    debug!("{:?}", list);
    for item in list {
        if item.url == file_url {
            // access file_url with HEAD
            let resp = client.head(file_url).send().await?;
            let mtime = resp.headers().get("Last-Modified").unwrap();
            let mtime = DateTime::parse_from_rfc2822(mtime.to_str().unwrap())?.with_timezone(&Utc);

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
