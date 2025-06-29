// https://httpd.apache.org/docs/2.4/mod/mod_autoindex.html
// > F=2 formats the listing as an HTMLTable FancyIndexed list

use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};

use super::*;
use anyhow::{anyhow, Result};
use chrono::NaiveDateTime;
use scraper::{Html, Selector};
use tracing::debug;

#[derive(Debug, Clone, Default)]
pub struct ApacheF2ListingParser;

impl Parser for ApacheF2ListingParser {
    fn name(&self) -> &'static str {
        "Apache-f2 format"
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
        // find the indexlist which contains file index
        let selector = Selector::parse("table").unwrap();
        let mut selector_iter = document.select(&selector);
        let indexlist;
        loop {
            let t = selector_iter
                .next()
                .ok_or(anyhow!("No more <table> matched"))?;
            let t_html = t.html().to_lowercase();
            if t_html.contains("name")
                && t_html.contains("last modified")
                && t_html.contains("size")
            {
                indexlist = t;
                break;
            }
        }
        // find all <tr> inside -- there might have titlebar or <hr>, filter them later
        let selector = Selector::parse("tr").unwrap();
        let mut items = Vec::new();

        let mut lastmod_before_size = true;
        for element in indexlist.select(&selector) {
            // skip divider
            let hr_selector = Selector::parse("hr").unwrap();
            if element.select(&hr_selector).next().is_some() {
                continue;
            }
            // skip table title
            let a_selector = Selector::parse("a").unwrap();
            let hrefs: Vec<&str> = element
                .select(&a_selector)
                .map(|a| a.value().attr("href").unwrap_or("?"))
                .collect();
            // Empty or all query string hrefs
            if hrefs.iter().all(|h| h.starts_with('?')) {
                let lastmod_pos = element.inner_html().to_lowercase().find("last modified");
                let size_pos = element.inner_html().to_lowercase().find("size");
                if let (Some(lastmod_pos), Some(size_pos)) = (lastmod_pos, size_pos) {
                    lastmod_before_size = lastmod_pos < size_pos;
                }
                continue;
            }

            let td_selector = Selector::parse("td").unwrap();
            let mut td_iterator = element.select(&td_selector);
            let td_count = td_iterator.clone().count();
            // skip icon (first col)
            if td_count > 3 {
                td_iterator.next();
            }
            let td = td_iterator
                .next()
                .ok_or(anyhow!("no more td after first iterate"))?;
            let a = td.select(&a_selector).next().unwrap();
            let displayed_filename = a.inner_html();
            if displayed_filename == "Parent Directory" || displayed_filename == ".." {
                continue;
            }

            let href = a.value().attr("href").unwrap();
            let name = get_real_name_from_href(href);
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') || displayed_filename.ends_with('/') {
                // check displayed filename here to workaround some servers
                FileType::Directory
            } else {
                FileType::File
            };
            let col2 = td_iterator
                .next()
                .ok_or(anyhow!("no more td after second iterate"))?
                .inner_html();
            let col2 = col2.trim();
            let col3 = td_iterator
                .next()
                .ok_or(anyhow!("no more td after third iterate"))?
                .inner_html();
            let col3 = col3.trim();

            let (lastmod, size) = if lastmod_before_size {
                (col2, col3)
            } else {
                (col3, col2)
            };

            let lastmod = if lastmod == "-" {
                // if lastmod is "-", it means the file is not modified
                ""
            } else {
                lastmod
            };

            debug!("{} {} {} {}", href, name, lastmod, size);

            let date = if lastmod.is_empty() && type_ == FileType::Directory {
                // if it's a directory, it's okay to have empty lastmod
                NaiveDateTime::default()
            } else {
                debug!("lastmod: {}", lastmod);
                let (date_fmt, _) = guess_date_fmt(lastmod);
                debug!("date_fmt: {}", date_fmt);
                NaiveDateTime::parse_from_str(lastmod, &date_fmt)?
            };

            items.push(ListItem::new(
                href,
                name.to_string(),
                type_,
                {
                    if size == "-" || size.is_empty() {
                        None
                    } else {
                        let (n_size, unit) = FileSize::get_humanized(size);
                        Some(FileSize::HumanizedBinary(n_size, unit))
                    }
                },
                date,
                None,
            ))
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
    fn test_winehq_root() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/wine-builds").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 8);
                assert_eq!(items[0].name, "android");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2022-01-18 15:14", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[6].name, "Release.key");
                assert_eq!(items[6].type_, FileType::File);
                assert_eq!(
                    items[6].size,
                    Some(FileSize::HumanizedBinary(3.0, SizeUnit::K))
                );
                assert_eq!(
                    items[6].mtime,
                    NaiveDateTime::parse_from_str("2017-03-28 14:54", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_raspberrypi_root() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/raspberrypi/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 61);
                assert_eq!(items[0].name, "AstroPi");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2017-09-04 15:41", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[6].name, "Raspberry_Pi_Education_Manual.pdf");
                assert_eq!(items[6].type_, FileType::File);
                assert_eq!(
                    items[6].size,
                    Some(FileSize::HumanizedBinary(2.8, SizeUnit::M))
                );
                assert_eq!(
                    items[6].mtime,
                    NaiveDateTime::parse_from_str("2013-09-16 13:51", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_mozilla_root() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/mozilla/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 46);
                assert_eq!(items[0].name, "OJI");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("1970-01-01 00:00", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_mozilla_oji() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/mozilla/OJI/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].name, "MRJPlugin");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("1970-01-01 00:00", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[1].name, "MRJPlugin.sit.hqx");
                assert_eq!(items[1].type_, FileType::File);
                assert_eq!(
                    items[1].size,
                    Some(FileSize::HumanizedBinary(234.0, SizeUnit::K))
                );
                assert_eq!(
                    items[1].mtime,
                    NaiveDateTime::parse_from_str("2023-02-13 04:21", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_grml() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/grml/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 10);
                // Test "+"
                assert_eq!(items[3].name, "memtest86+");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_influxdata() {
        let context = init_async_context();
        let items = ApacheF2ListingParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/influxdata/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 11);
                assert_eq!(items[0].name, "centos");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("1970-01-01 00:00", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[6].name, "influxdata-archive.key");
                assert_eq!(items[6].type_, FileType::File);
                assert_eq!(
                    items[6].size,
                    // Some(FileSize::Precise(3935))
                    Some(FileSize::HumanizedBinary(3935.0, SizeUnit::B))
                );
                assert_eq!(
                    items[6].mtime,
                    NaiveDateTime::parse_from_str("2023-01-26 21:11:34", "%Y-%m-%d %H:%M:%S").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }
}
