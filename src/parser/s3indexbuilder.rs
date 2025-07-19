// For https://github.com/mhagander/s3indexbuilder

use crate::{
    listing::{FileSize, FileType, ListItem},
    parser::{
        assert_if_url_has_no_trailing_slash, get_real_name_from_href, ListResult, Parser,
        ParserError,
    },
    utils::{get, get_text},
};
use anyhow::{anyhow, Result};
use chrono::{FixedOffset, NaiveDateTime};
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct S3Indexbuilder;

impl Parser for S3Indexbuilder {
    fn name(&self) -> &'static str {
        "s3indexbuilder format"
    }

    fn get_list(
        &self,
        async_context: &crate::AsyncContext,
        url: &url::Url,
    ) -> Result<ListResult, ParserError> {
        let resp = get(
            &async_context.runtime,
            &async_context.listing_client,
            url.clone(),
        )?;
        let url = resp.url().clone();
        let body = get_text(&async_context.runtime, resp)?;
        assert_if_url_has_no_trailing_slash(&url);
        let document = Html::parse_document(&body);
        let selector = Selector::parse("table").unwrap();
        let table = document
            .select(&selector)
            .next()
            .ok_or(anyhow!("No <table> found in document"))?;
        let selector = Selector::parse("tr").unwrap();
        let mut items = Vec::new();
        for element in table.select(&selector) {
            let td_selector = Selector::parse("td").unwrap();
            let tds: Vec<_> = element.select(&td_selector).collect();
            assert_eq!(
                tds.len(),
                3,
                "Expected 3 <td> elements, found {}",
                tds.len()
            );
            let a = tds[0]
                .child_elements()
                .next()
                .ok_or(anyhow!("No <a> element found in first <td>"))?;
            if a.inner_html() == "../" {
                continue;
            }
            let href = a
                .value()
                .attr("href")
                .ok_or(anyhow!("No href found in <a> element in first <td>"))?;
            let name = get_real_name_from_href(href);
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            let date = tds[1].inner_html();
            let date = date.trim(); // %d-%b-%Y %H:%M
            let mtime = match type_ {
                FileType::File => NaiveDateTime::parse_from_str(date, "%d-%b-%Y %H:%M")?,
                FileType::Directory => NaiveDateTime::default(),
            };
            let size = tds[2].inner_html();
            let size = size.trim();
            let size = if size.is_empty() {
                None
            } else {
                let size = size.parse::<u64>().expect("Expected size to be u64");
                Some(FileSize::Precise(size))
            };
            items.push(ListItem::new(
                href,
                name,
                type_,
                size,
                mtime,
                FixedOffset::east_opt(0),
            ));
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::tests::*;
    use url::Url;

    #[test]
    fn test_postgresql_srpms_testing() {
        let context = init_async_context();
        let items = S3Indexbuilder
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/postgresql/srpms/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 149);
                assert_eq!(items[0].name, "bgw_replstatus_13-1.0.6-5PGDG.f42.src.rpm");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(items[0].size, Some(FileSize::Precise(19340)));
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2025-03-26 13:41", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
