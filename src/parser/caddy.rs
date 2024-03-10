/// A parser for default caddy file_server format
use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};

use super::*;
use anyhow::Result;
use chrono::NaiveDateTime;
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct CaddyListingParser;

impl Parser for CaddyListingParser {
    fn get_list(&self, client: &Client, url: &Url) -> Result<ListResult> {
        let resp = get(client, url.clone())?;
        let url = resp.url().clone();
        let body = resp.text()?;
        assert_if_url_has_no_trailing_slash(&url);
        let document = Html::parse_document(&body);
        let selector = Selector::parse("tr.file").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            // name and herf
            let selector = Selector::parse("td a").unwrap();
            let a = element.select(&selector).next().unwrap();
            let href = a.value().attr("href").unwrap();
            // Caddy file_server will append "./" to href
            let name = get_real_name_from_href(href)
                .trim_start_matches("./")
                .to_string();
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            // size
            let selector = Selector::parse("td.size div.sizebar div.sizebar-text").unwrap();
            let size = match element.select(&selector).next() {
                Some(s) => {
                    let (n_size, unit) = FileSize::get_humanized(s.inner_html().trim());
                    Some(FileSize::HumanizedBinary(n_size, unit))
                }
                None => None,
            };
            // date
            let selector = Selector::parse("td.timestamp time").unwrap();
            let mtime = element
                .select(&selector)
                .next()
                .unwrap()
                .value()
                .attr("datetime")
                .unwrap()
                .trim();
            // Store UTC time
            let date = NaiveDateTime::parse_from_str(mtime, "%Y-%m-%dT%H:%M:%S%Z")?;

            items.push(ListItem::new(href, name, type_, size, date))
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use crate::listing::SizeUnit;

    use super::*;

    #[test]
    fn test_sdumirror_ubuntu() {
        let client = reqwest::blocking::Client::new();
        let items = CaddyListingParser
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/sdumirror-ubuntu").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 7);
                assert_eq!(items[0].name, ".trace");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2023-07-10T13:07:52Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
                assert_eq!(items[5].name, "ubuntu");
                assert_eq!(items[5].type_, FileType::Directory);
                assert_eq!(items[5].size, None);
                assert_eq!(
                    items[5].mtime,
                    NaiveDateTime::parse_from_str("2010-11-24T11:01:53Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
                assert_eq!(items[6].name, "ls-lR.gz");
                assert_eq!(items[6].type_, FileType::File);
                assert_eq!(
                    items[6].size,
                    Some(FileSize::HumanizedBinary(26.0, SizeUnit::M))
                );
                assert_eq!(
                    items[6].mtime,
                    NaiveDateTime::parse_from_str("2024-03-10T04:45:24Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
