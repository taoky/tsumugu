use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};
use chrono::NaiveDateTime;
use scraper::{Html, Selector};

use super::{ListResult, Parser};
use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct NginxListingParser {
    metadata_regex: Regex,
}

impl Default for NginxListingParser {
    fn default() -> Self {
        Self {
            metadata_regex: Regex::new(r"(\d{2}-\w{3}-\d{4} \d{2}:\d{2})\s+([\d-]+)$").unwrap(),
        }
    }
}

impl Parser for NginxListingParser {
    fn get_list(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<ListResult> {
        let resp = get(client, url.clone())?;
        let url = resp.url().clone();
        let body = resp.text()?;
        assert!(
            url.path().ends_with('/'),
            "URL for listing should have a trailing slash"
        );
        let document = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            let href = match element.value().attr("href") {
                Some(href) => href,
                None => continue,
            };
            // It's not proper to get filename by <a> text
            // As when it is too long, this could happen:
            // ceph-immutable-object-cache_17.2.6-pve1+3_amd64..> 03-May-2023 23:52              150048
            // So we should get filename from href
            let name: String = url::form_urlencoded::parse(href.as_bytes())
                .map(|(k, v)| [k, v].concat())
                .collect();
            let href = url.join(href)?;

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
            items.push(ListItem::new(
                href,
                name.to_string(),
                type_,
                size.map(FileSize::Precise),
                date,
            ))
        }
        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitoring_plugins() {
        let client = reqwest::blocking::Client::new();
        let items = NginxListingParser::default()
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/monitoring-plugins").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 23);
                assert_eq!(items[0].name, "archive");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("09-Oct-2015 16:12", "%d-%b-%Y %H:%M").unwrap()
                );
                assert_eq!(items[4].name, "monitoring-plugins-2.0.tar.gz");
                assert_eq!(items[4].type_, FileType::File);
                assert_eq!(items[4].size, Some(FileSize::Precise(2610000)));
                assert_eq!(
                    items[4].mtime,
                    NaiveDateTime::parse_from_str("11-Jul-2014 23:17", "%d-%b-%Y %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
