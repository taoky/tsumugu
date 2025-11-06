use crate::{
    client::HttpClient,
    listing::{FileSize, FileType, ListItem},
    parser::{assert_if_url_has_no_trailing_slash, get_real_name_from_href},
};

use super::{ListResult, Parser, ParserError};
use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use scraper::CaseSensitivity::*;
use scraper::{Html, Selector};
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct DenoFlareR2ListingParser;

// Ref: https://github.com/skymethod/denoflare/blob/2e89fb33972a924dd9c5078bb2b2834a1f619081/examples/r2-public-read-worker/listing.ts

impl Parser for DenoFlareR2ListingParser {
    fn name(&self) -> &'static str {
        "DenoFlare R2 Public Read Worker example"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &url::Url) -> Result<ListResult, ParserError> {
        let mut documents = vec![];
        assert_if_url_has_no_trailing_slash(url);
        let mut inner_url = url.clone();
        loop {
            info!("(in paging loop) Fetching: {}", inner_url);
            let resp = client.get(inner_url.clone())?;
            let body = client.get_text(resp)?;
            let document = Html::parse_document(&body);
            documents.push((inner_url.clone(), document.clone()));

            // Check if last element of #contents is next ➜
            let selector = Selector::parse("div#contents").unwrap();
            let contents = document
                .select(&selector)
                .next()
                .ok_or(anyhow!("<div id=\"contents\"> not found"))?;
            // <div class="full"><a href="...">next ➜</a></div>
            let last_child = contents
                .child_elements()
                .last()
                .ok_or(anyhow!("Expected last child"))?;
            // <a href="...">next ➜</a>
            let last_child = match last_child.last_child() {
                Some(child) => child,
                None => break,
            };
            // next ➜
            let textnode = match last_child.first_child() {
                Some(child) => child,
                _ => break,
            };
            if textnode.value().as_text().map_or("", |t| t) != "next ➜" {
                break;
            }
            let href = last_child
                .value()
                .as_element()
                .ok_or(anyhow!("Expected <a> element"))?
                .attr("href")
                .ok_or(anyhow!("href not found in <a> element"))?;
            inner_url = url.join(href)?;
        }
        let mut items = Vec::new();
        for (url, document) in documents {
            let selector = Selector::parse("div#contents").unwrap();
            let contents = document
                .select(&selector)
                .next()
                .ok_or(anyhow!("<div id=\"contents\"> not found"))?;

            enum State {
                Start,
                Dirs,
                Files,
            }
            let mut state = State::Start;

            let mut iter = contents.child_elements().peekable();
            while let Some(child) = iter.next() {
                match state {
                    State::Start => {
                        if child.value().name() == "div"
                            && child.value().has_class("full", CaseSensitive)
                            && child.text().next().unwrap_or_default() == "\u{a0}"
                        // &nbsp;
                        {
                            // peek
                            let next_elem = iter.peek().ok_or(anyhow!("Expected next element"))?;
                            let class_is_full = next_elem.value().has_class("full", CaseSensitive);
                            if class_is_full {
                                state = State::Dirs;
                            } else {
                                state = State::Files;
                            }
                        }
                    }
                    State::Dirs => {
                        if child.value().name() == "div" {
                            assert!(
                                child.value().has_class("full", CaseSensitive),
                                "Expected class=\"full\" as end of dirs"
                            );
                            assert!(
                                child.text().next().unwrap_or_default() == "\u{a0}",
                                "Expected &nbsp; as end of dirs"
                            );
                            state = State::Files;
                            continue;
                        }
                        assert!(child.value().name() == "a", "Expected <a> in dirs");
                        let href = child
                            .value()
                            .attr("href")
                            .ok_or(anyhow!("href not found"))?;
                        let name = get_real_name_from_href(href);
                        let href = url.join(href)?;
                        items.push(ListItem::new(
                            href,
                            name,
                            FileType::Directory,
                            None,
                            DateTime::UNIX_EPOCH.naive_utc(),
                            None,
                        ));
                    }
                    State::Files => {
                        if child.value().name() == "div" {
                            assert!(
                                child.value().has_class("full", CaseSensitive),
                                "Expected class=\"full\" as end of files, if paging required."
                            );
                            break;
                        }
                        assert!(child.value().name() == "a", "Expected <a> in files");
                        let href = child
                            .value()
                            .attr("href")
                            .ok_or(anyhow!("href not found"))?;
                        if href.ends_with('/') {
                            for _ in 0..3 {
                                iter.next();
                            }
                            continue;
                        }
                        let name = get_real_name_from_href(href);
                        let child = iter.next().ok_or(anyhow!("Expected next child"))?;
                        let size = child
                            .text()
                            .next()
                            .ok_or(anyhow!("Expected size text"))?
                            .replace(',', ""); // bytes
                        let size = size
                            .parse::<u64>()
                            .map_err(|e| anyhow!("Expected size to be u64: {}", e))?;
                        iter.next(); // skip estimated size
                        let mtime = iter
                            .next()
                            .ok_or(anyhow!("Expected mtime"))?
                            .text()
                            .next()
                            .ok_or(anyhow!("Expected mtime text"))?;
                        let mtime = NaiveDateTime::parse_from_str(mtime, "%Y-%m-%dT%H:%M:%S.%3fZ")
                            .map_err(|e| anyhow!("Expected mtime to be NaiveDateTime: {}", e))?;
                        let href = url.join(href)?;
                        items.push(ListItem::new(
                            href,
                            name,
                            FileType::File,
                            Some(FileSize::Precise(size)),
                            mtime,
                            FixedOffset::east_opt(0),
                        ));
                    }
                }
            }
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::parser::tests::*;

    use super::*;

    #[test]
    fn test_clickhouse() {
        let context = init_client();
        let items = DenoFlareR2ListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/clickhouse/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0].name, "deb");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[1].name, "rpm");
                assert_eq!(items[1].type_, FileType::Directory);
                assert_eq!(items[2].name, "tgz");
                assert_eq!(items[2].type_, FileType::Directory);
                assert_eq!(items[3].name, "CLICKHOUSE-KEY.GPG");
                assert_eq!(items[3].type_, FileType::File);
                assert_eq!(items[3].size, Some(FileSize::Precise(3133)));
                assert_eq!(
                    items[3].mtime,
                    NaiveDateTime::parse_from_str(
                        "2022-09-23 13:53:51.925",
                        "%Y-%m-%d %H:%M:%S.%3f"
                    )
                    .unwrap()
                );
                assert_eq!(items[3].timezone, FixedOffset::east_opt(0));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_clickhouse_fileonly() {
        let context = init_client();
        let items = DenoFlareR2ListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/clickhouse/clickhouse-client/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 61);
                assert_eq!(items[0].name, "clickhouse-client_22.3.10.22_amd64.deb");
                assert_eq!(items[0].type_, FileType::File);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_clickhouse_multipage() {
        let context = init_client();
        let items = DenoFlareR2ListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/clickhouse/stable/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0].name, "clickhouse-client-21.1.9.41.tgz.sha512");
                assert_eq!(items[3].name, "clickhouse-client-23.7.3.14-arm64.tgz");
            }
            _ => unreachable!(),
        }
    }
}
