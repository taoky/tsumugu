use crate::listing::{FileSize, FileType, ListItem};
use chrono::{DateTime, NaiveDateTime};
use scraper::{Html, Selector};
use tracing::info;

use super::*;
use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct GradleListingParser {}

impl Parser for GradleListingParser {
    fn name(&self) -> &'static str {
        "services.gradle.org"
    }

    fn get_list(
        &self,
        async_context: &AsyncContext,
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
        let selector = Selector::parse("ul li").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            // Select <a> first, then <span>s
            let a_selector = Selector::parse("a").unwrap();
            let span_selector = Selector::parse("span").unwrap();
            let size_selector = Selector::parse("span.size").unwrap();
            let date_selector = Selector::parse("span.date").unwrap();

            if element.select(&span_selector).next().is_none() {
                info!("No <span> in this <li>. Maybe it's a header");
                continue;
            }

            let a = match element.select(&a_selector).next() {
                Some(a) => a,
                None => {
                    return Err(anyhow!("No <a> in given <li>").into());
                }
            };
            let href = a
                .value()
                .attr("href")
                .ok_or(anyhow!("No href found in <a> element"))?;
            let displayed_filename = a.inner_html();

            if displayed_filename == "Parent Directory/" || href == "../" {
                continue;
            }

            let name = get_real_name_from_href(href);
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            let size = element
                .select(&size_selector)
                .next()
                .ok_or(anyhow!("Cannot get size"))?
                .inner_html();
            let size = size.trim();
            let date = element
                .select(&date_selector)
                .next()
                .ok_or(anyhow!("Cannot get date"))?
                .inner_html();
            let date = date.trim();

            // decide (guess) which time format to use
            let (date_fmt, _) = guess_date_fmt(date);
            let naive_date;
            let timezone;
            if !date_fmt_has_timezone(&date_fmt) {
                naive_date = NaiveDateTime::parse_from_str(date, &date_fmt)?;
                timezone = None;
            } else {
                let date = DateTime::parse_from_str(date, &date_fmt)?;
                naive_date = date.naive_utc();
                timezone = Some(date.offset().to_owned());
            }

            items.push(ListItem::new(
                href,
                name,
                type_,
                {
                    if size == "-" {
                        None
                    } else {
                        let (n_size, unit) = FileSize::get_humanized(size);
                        Some(FileSize::HumanizedBinary(n_size, unit))
                    }
                },
                naive_date,
                timezone,
            ));
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use test_log::test;

    use crate::listing::SizeUnit;

    use super::*;
    use crate::parser::tests::*;

    #[test]
    fn test_gradle() {
        let context = init_async_context();
        let items = GradleListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/gradle").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 64);
                assert_eq!(items[0].name, "gradle-8.10-wrapper.jar.sha256");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(64.0, SizeUnit::B))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("14-Aug-2024 11:18", "%d-%b-%Y %H:%M").unwrap()
                );
                assert_eq!(items[0].timezone, FixedOffset::east_opt(0),);
            }
            _ => unreachable!(),
        }
    }
}
