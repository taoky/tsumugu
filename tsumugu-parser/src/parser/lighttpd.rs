use crate::listing::{FileSize, FileType, ListItem};
use chrono::NaiveDateTime;
use scraper::{Html, Selector};
// use tracing::debug;

use tsumugu_net::client::HttpClient;

use super::*;
use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default)]
pub struct LighttpdListingParser;

impl Parser for LighttpdListingParser {
    fn name(&self) -> &'static str {
        "Lighttpd"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &url::Url) -> Result<ListResult, ParserError> {
        let resp = handle_net!(client.get_text(url))?;
        let url: &Url = &resp.final_url;
        assert_if_url_has_no_trailing_slash(url);
        let document = Html::parse_document(&resp.body);
        let selector = Selector::parse("tbody").unwrap();
        let indexlist = document
            .select(&selector)
            .next()
            .ok_or_else(|| parse_error!("Cannot find <tbody>"))?;
        let selector = Selector::parse("tr").unwrap();
        let mut items = Vec::new();
        for element in indexlist.select(&selector) {
            let a = element
                .select(&Selector::parse("a").unwrap())
                .next()
                .ok_or_else(|| parse_error!("Cannot find <a>"))?;
            let mtime = element
                .select(&Selector::parse(".m").unwrap())
                .next()
                .ok_or_else(|| parse_error!("Cannot find .m"))?;
            let size = element
                .select(&Selector::parse(".s").unwrap())
                .next()
                .ok_or_else(|| parse_error!("Cannot find .s"))?;

            let displayed_filename = a.inner_html();
            if displayed_filename == ".." {
                continue;
            }
            let href = a
                .value()
                .attr("href")
                .ok_or_else(|| parse_error!("Cannot find href inside <a>"))?;
            let name = get_real_name_from_href(href);
            let href = url.join(href)?;

            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };

            let mtime = mtime.inner_html();
            let mtime = mtime.trim();
            let mtime = NaiveDateTime::parse_from_str(mtime, "%Y-%b-%d %H:%M:%S")?;

            let size = size.inner_html();
            // Currently we just use simple replace to handle HTML entities
            // if we need a more sophisticated way to handle it, we should use a crate
            // like https://crates.io/crates/htmlentity
            let size = size.replace("&nbsp;", "");
            let size = size.trim();
            let size = if size == "-" {
                None
            } else {
                let (n_size, unit) = FileSize::get_humanized(size);
                Some(FileSize::HumanizedBinary(n_size, unit))
            };

            // debug!("{} {} {} {:?} {:?}", href, name, mtime, size, type_);
            items.push(ListItem::new(href, name, type_, size, mtime, None))
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
    fn test_buildroot_root() {
        let context = init_client();
        let items = LighttpdListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/buildroot/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items[0].name, "18xx-ti-utils");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2021-01-11 15:59:23", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                let last_item = items.last().unwrap();
                assert_eq!(last_item.name, "zyre-v2.0.0.tar.gz");
                assert_eq!(last_item.type_, FileType::File);
                assert_eq!(
                    last_item.size,
                    Some(FileSize::HumanizedBinary(262.1, SizeUnit::K))
                );
                assert_eq!(
                    last_item.mtime,
                    NaiveDateTime::parse_from_str("2018-03-08 11:18:46", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_buildroot_subfolder() {
        let context = init_client();
        let items = LighttpdListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/buildroot/acl/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0].name, "acl-2.2.52.src.tar.gz");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(377.5, SizeUnit::K))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2013-05-19 06:10:38", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                assert_eq!(items[3].name, "acl-2.3.2.tar.xz");
                assert_eq!(items[3].type_, FileType::File);
                assert_eq!(
                    items[3].size,
                    Some(FileSize::HumanizedBinary(362.9, SizeUnit::K))
                );
                assert_eq!(
                    items[3].mtime,
                    NaiveDateTime::parse_from_str("2024-02-07 03:04:10", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
