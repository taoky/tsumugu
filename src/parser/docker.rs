use crate::{
    list::{FileSize, FileType, ListItem},
    utils::get,
};
use chrono::NaiveDateTime;
use scraper::{Html, Selector};
// use tracing::debug;

use super::{ListResult, Parser};
use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct DockerListingParser {
    metadata_regex: Regex,
}

impl Default for DockerListingParser {
    fn default() -> Self {
        Self {
            metadata_regex: Regex::new(r#"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+([\d \w\.]+)$"#)
                .unwrap(),
        }
    }
}

impl Parser for DockerListingParser {
    fn is_auto_redirect(&self) -> bool {
        false
    }

    fn get_list(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<ListResult> {
        assert!(
            url.path().ends_with('/'),
            "URL for listing should have a trailing slash"
        );
        let resp = get(client, url.clone())?;
        // if is a redirect?
        if let Some(url) = resp.headers().get("location") {
            return Ok(ListResult::Redirect(url.to_str()?.to_string()));
        }
        let body = resp.text()?;
        let document = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            let href = match element.value().attr("href") {
                Some(href) => href,
                None => continue,
            };
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
            let (size, date) = if type_ == FileType::File {
                let metadata_raw = element
                    .next_sibling()
                    .unwrap()
                    .value()
                    .as_text()
                    .unwrap()
                    .to_string();
                let metadata_raw = metadata_raw.trim();
                let metadata = self.metadata_regex.captures(metadata_raw).unwrap();
                let date = metadata.get(1).unwrap().as_str();
                let date = NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S")?;
                let size = metadata.get(2).unwrap().as_str();
                let (n_size, unit) = FileSize::get_humanized(size);
                (Some(FileSize::HumanizedBinary(n_size, unit)), date)
            } else {
                (None, NaiveDateTime::default())
            };
            items.push(ListItem {
                url: href,
                name: name.to_string(),
                type_,
                size,
                mtime: date,
            })
        }
        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use crate::list::SizeUnit;

    use super::*;

    #[test]
    fn test_docker() {
        let client = reqwest::blocking::Client::new();
        let items = DockerListingParser::default()
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/docker/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 45);
                assert_eq!(items[0].name, "7.0");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::default()
                );
                assert_eq!(items[42].name, "docker-ce-staging.repo");
                assert_eq!(items[42].type_, FileType::File);
                assert_eq!(items[42].size, Some(FileSize::HumanizedBinary(2.0, SizeUnit::K)));
                assert_eq!(
                    items[42].mtime,
                    NaiveDateTime::parse_from_str("2023-07-07 20:20:56", "%Y-%m-%d %H:%M:%S").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
