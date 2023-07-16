use crate::list::{FileType, ListItem};
use async_trait::async_trait;
use chrono::NaiveDateTime;
use scraper::{Html, Selector};

use super::Parser;
use anyhow::Result;
use regex::Regex;

pub struct NginxListingParser {
    metadata_regex: Regex,
}

impl Default for NginxListingParser {
    fn default() -> Self {
        Self {
            metadata_regex: Regex::new(r#"(\d{2}-\w{3}-\d{4} \d{2}:\d{2})\s+([\d-]+)$"#).unwrap(),
        }
    }
}

#[async_trait]
impl Parser for NginxListingParser {
    async fn get_list(&self, client: &reqwest::Client, url: &url::Url) -> Result<Vec<ListItem>> {
        let body = client.get(url.as_str()).send().await?.text().await?;
        let document = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            let href = match element.value().attr("href") {
                Some(href) => href,
                None => continue,
            };
            let href = url.join(href)?;
            let name = element.text().collect::<Vec<_>>().join("");
            let name = name.trim_end_matches('/');
            if name == ".." {
                continue;
            }
            let type_ = if href.as_str().ends_with('/') {
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
            let metadata = self.metadata_regex.captures(metadata_raw).unwrap();
            let date = metadata.get(1).unwrap().as_str();
            let date = NaiveDateTime::parse_from_str(date, "%d-%b-%Y %H:%M")?;
            let size = metadata.get(2).unwrap().as_str().parse::<u64>().ok();
            // println!("{} {} {:?} {} {:?}", href, name, type_, date, size);
            items.push(ListItem {
                url: href,
                name: name.to_string(),
                type_,
                size,
                mtime: date,
            })
        }
        Ok(items)
    }
}
