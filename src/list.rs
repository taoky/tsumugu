// Module for handling directory listing

use anyhow::Result;
use chrono::{DateTime, Utc, NaiveDateTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use url::Url;

#[derive(Debug)]
enum FileType {
    File,
    Directory,
}

#[derive(Debug)]
pub struct ListItem {
    url: String,
    name: String,
    type_: FileType,
    size: Option<u64>,
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
