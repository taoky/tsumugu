/// A parser for default caddy file_server format
use crate::listing::{FileSize, FileType, ListItem};

use tsumugu_net::client::HttpClient;

use super::*;
use anyhow::Result;
use chrono::NaiveDateTime;
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct CaddyListingParser;

impl Parser for CaddyListingParser {
    fn name(&self) -> &'static str {
        "Caddy"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &url::Url) -> Result<ListResult, ParserError> {
        let resp = handle_net!(client.get_text(url))?;
        let url: &Url = &resp.final_url;
        assert_if_url_has_no_trailing_slash(url);
        let document = Html::parse_document(&resp.body);
        let selector = Selector::parse("tr.file").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            // name and herf
            let selector = Selector::parse("td a").unwrap();
            let a = element
                .select(&selector)
                .next()
                .ok_or(parse_error!("td a not found in <tr> element"))?;
            let href = a
                .value()
                .attr("href")
                .ok_or(parse_error!("no href found in <a> element"))?;
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
                    let size_text = s.inner_html();
                    // ↱&nbsp; would be added by caddy when it's a symlink
                    // https://github.com/caddyserver/caddy/commit/9338741ca79a74247ced86bc26e4994138470852
                    let size_text = size_text.trim().trim_start_matches("↱&nbsp;");
                    let (n_size, unit) = FileSize::get_humanized(size_text);
                    Some(FileSize::HumanizedBinary(n_size, unit))
                }
                None => None,
            };
            // date
            let selector = Selector::parse("td.timestamp time").unwrap();
            let mtime = element
                .select(&selector)
                .next()
                .ok_or(parse_error!("td.timestamp time not found in <tr> element"))?
                .value()
                .attr("datetime")
                .ok_or(parse_error!("no datetime found in <time> element"))?
                .trim();
            // Store UTC time
            let date = NaiveDateTime::parse_from_str(mtime, "%Y-%m-%dT%H:%M:%S%Z")?;

            items.push(ListItem::new(href, name, type_, size, date, None))
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use crate::listing::SizeUnit;

    use super::*;
    use crate::parser::tests::*;

    #[test]
    fn test_sdumirror_ubuntu() {
        let context = init_client();
        let items = CaddyListingParser
            .get_list(
                &context,
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

    #[test]
    fn test_caddy_symlink() {
        let context = init_client();
        let items = CaddyListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/caddy-symlink").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].name, "aoi.png");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(32.0, SizeUnit::K))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2022-11-19T19:15:45Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
                assert_eq!(items[1].name, "index.html.bak");
                assert_eq!(items[1].type_, FileType::File);
                assert_eq!(
                    items[1].size,
                    Some(FileSize::HumanizedBinary(143.0, SizeUnit::B))
                );
                assert_eq!(
                    items[1].mtime,
                    NaiveDateTime::parse_from_str("2022-11-19T19:14:38Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
                assert_eq!(items[2].name, "symlink");
                assert_eq!(items[2].type_, FileType::File);
                assert_eq!(
                    items[2].size,
                    Some(FileSize::HumanizedBinary(143.0, SizeUnit::B))
                );
                assert_eq!(
                    items[2].mtime,
                    NaiveDateTime::parse_from_str("2025-02-27T10:45:49Z", "%Y-%m-%dT%H:%M:%S%Z")
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
