/// A parser both suitable for default nginx autoindex and apache f1 format.
use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};
use chrono::NaiveDateTime;
use scraper::{Html, Selector};
use tracing::debug;

use super::*;
use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct NginxListingParser {
    metadata_regex: Regex,
}

impl Default for NginxListingParser {
    fn default() -> Self {
        Self {
            metadata_regex: Regex::new(r"(\d{2}-\w{3}-\d{4} \d{2}:\d{2})\s+([\d\.\-kMG]+)$")
                .unwrap(),
        }
    }
}

impl Parser for NginxListingParser {
    fn get_list(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<ListResult> {
        let resp = get(client, url.clone())?;
        let url = resp.url().clone();
        let body = resp.text()?;
        assert_if_url_has_no_trailing_slash(&url);
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
            let name: String = if href.contains('%') {
                get_real_name_from_href(href)
            } else {
                // A compromise for apache server (they will NOT url-encode the filename)
                href.to_string()
            };
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
            debug!("{:?}", metadata_raw);
            let metadata = self.metadata_regex.captures(metadata_raw).unwrap();
            let date = metadata.get(1).unwrap().as_str();
            let date = NaiveDateTime::parse_from_str(date, "%d-%b-%Y %H:%M")?;
            let size = metadata.get(2).unwrap().as_str();
            debug!("{} {} {:?} {} {:?}", href, name, type_, date, size);
            items.push(ListItem::new(
                href,
                name.to_string(),
                type_,
                {
                    if size == "-" {
                        None
                    } else if size.contains('k') || size.contains('M') || size.contains('G') {
                        let (n_size, unit) = FileSize::get_humanized(size);
                        Some(FileSize::HumanizedBinary(n_size, unit))
                    } else {
                        let n_size = size.parse::<u64>().unwrap();
                        Some(FileSize::Precise(n_size))
                    }
                },
                date,
            ))
        }
        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

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

    #[test]
    fn test_proxmox() {
        let client = reqwest::blocking::Client::new();
        let items = NginxListingParser::default()
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/proxmox").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                let target = "ceph-immutable-object-cache_17.2.6-pve1+3_amd64.deb";
                let find_res = items.iter().find(|item| item.name == target).unwrap();
                assert_eq!(find_res.name, target);
                assert_eq!(find_res.type_, FileType::File);
                // keep as-is
                assert_eq!(find_res.url, Url::parse("http://localhost:1921/proxmox/ceph-immutable-object-cache_17.2.6-pve1%2B3_amd64.deb").unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_mysql() {
        let client = reqwest::blocking::Client::new();
        let items = NginxListingParser::default()
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/mysql").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                let target = "mysql-connector-c++";
                let find_res = items.iter().find(|item| item.name == target).unwrap();
                assert_eq!(find_res.name, target);
                assert_eq!(find_res.type_, FileType::Directory);
                // keep as-is
                assert_eq!(
                    find_res.url,
                    Url::parse("http://localhost:1921/mysql/mysql-connector-c++/").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
