// Nginx fancyindex parser

use crate::{
    client::HttpClient,
    listing::{FileSize, FileType, ListItem},
};

use super::*;
use anyhow::Result;
use chrono::{DateTime, NaiveDateTime};
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct FancyIndexListingParser;

impl Parser for FancyIndexListingParser {
    fn name(&self) -> &'static str {
        "Fancyindex"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &url::Url) -> Result<ListResult, ParserError> {
        let resp = client.get(url.clone())?;
        let url = resp.url().clone();
        let body = client.get_text(resp)?;
        assert_if_url_has_no_trailing_slash(&url);
        let document = Html::parse_document(&body);
        let selector = Selector::parse("tbody tr").unwrap();
        let mut items = Vec::new();
        for element in document.select(&selector) {
            // let link_selector = Selector::parse("td.link a").unwrap();
            // let size_selector = Selector::parse("td.size").unwrap();
            // let date_selector = Selector::parse("td.date").unwrap();

            // Select <td> in order, instead of using class name, to improve compatibility for strange pages
            let td_selector = Selector::parse("td").unwrap();
            let mut td_iterator = element.select(&td_selector);

            let td_a = match td_iterator.next() {
                Some(tda) => tda,
                None => {
                    warn!("Cannot find <td> in this <tr> (header maybe?), skipping...");
                    continue;
                }
            };
            let a = match td_a.select(&Selector::parse("a").unwrap()).next() {
                Some(a) => a,
                None => {
                    return Err(anyhow!("Cannot find <a> in first cell.").into());
                }
            };
            let href = a
                .value()
                .attr("href")
                .ok_or(anyhow!("No href found in <a> element in first cell"))?;
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
            let size = td_iterator
                .next()
                .ok_or(anyhow!("Cannot get size in td"))?
                .inner_html();
            let size = size.trim();
            let date = td_iterator
                .next()
                .ok_or(anyhow!("Cannot get date in td"))?
                .inner_html();
            let date = &date_normalization(date.trim());

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

    use super::*;
    use crate::listing::SizeUnit;
    use crate::parser::tests::*;

    #[test]
    fn test_njumirrors() {
        let context = init_client();
        let items = FancyIndexListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/bmclapi/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items[0].name, "bouncycastle");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2024-04-23 19:01:54", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                assert_eq!(items[items.len() - 1].name, "lwjgURL");
                assert_eq!(items[items.len() - 1].type_, FileType::File);
                assert_eq!(
                    items[items.len() - 1].size,
                    Some(FileSize::HumanizedBinary(1767.0, SizeUnit::B))
                );
                assert_eq!(
                    items[items.len() - 1].mtime,
                    NaiveDateTime::parse_from_str("2021-04-30 20:55:32", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_loongnix() {
        let context = init_client();
        let items = FancyIndexListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/loongnix/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items[0].name, "contrib");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2023-08-15 05:48", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[items.len() - 1].name, "Release.gpg");
                assert_eq!(items[items.len() - 1].type_, FileType::File);
                assert_eq!(
                    items[items.len() - 1].size,
                    Some(FileSize::HumanizedBinary(659.0, SizeUnit::B))
                );
                assert_eq!(
                    items[items.len() - 1].mtime,
                    NaiveDateTime::parse_from_str("2023-08-15 05:48", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_misc_1() {
        // In fact this is NOT a fancyindex page, but it basically match the layout of that.
        let context = init_client();
        let items = FancyIndexListingParser
            .get_list(
                &context,
                &Url::parse("http://localhost:1921/misc/1/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "passwd");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(3.3, SizeUnit::K))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2024-08-24 15:04:11", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                assert_eq!(items[0].timezone, FixedOffset::east_opt(0),);
            }
            _ => unreachable!(),
        }
    }
}
