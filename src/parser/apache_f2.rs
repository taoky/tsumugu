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
// use tracing::debug;

#[derive(Debug, Clone, Default)]
pub struct ApacheF2ListingParser;

impl Parser for ApacheF2ListingParser {
    fn get_list(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<ListResult> {
        let resp = get(client, url.clone())?;
        let url = resp.url().clone();
        let body = resp.text()?;
        assert_if_url_has_no_trailing_slash(&url);
        let document = Html::parse_document(&body);
        // find #indexlist which contains file index
        let selector = Selector::parse("table").unwrap();
        let indexlist;
        loop {
            let t = document
                .select(&selector)
                .next()
                .ok_or(anyhow!("No more <table> matched"))?;
            let t_html = t.html();
            if t_html.contains("Name")
                && t_html.contains("Last modified")
                && t_html.contains("Size")
            {
                indexlist = t;
                break;
            }
        }
        // find all <tr> inside -- there might have titlebar or <hr>, filter them later
        let selector = Selector::parse("tr").unwrap();
        let mut items = Vec::new();
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
            if hrefs.iter().all(|h| h.starts_with('?')) {
                continue;
            }

            let td_selector = Selector::parse("td").unwrap();
            let mut td_iterator = element.select(&td_selector);
            // skip icon (first col)
            td_iterator.next();
            let td = td_iterator
                .next()
                .ok_or(anyhow!("no more td after first iterate"))?;
            let a = td.select(&a_selector).next().unwrap();
            let displayed_filename = a.inner_html();
            if displayed_filename == "Parent Directory" {
                continue;
            }

            let href = a.value().attr("href").unwrap();
            let name = get_real_name_from_href(href);
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            // lastmod
            let lastmod = td_iterator
                .next()
                .ok_or(anyhow!("no more td after second iterate"))?
                .inner_html();
            let lastmod = lastmod.trim();
            // size
            let size = td_iterator
                .next()
                .ok_or(anyhow!("no more td after third iterate"))?
                .inner_html();
            let size = size.trim();

            // debug!("{} {} {} {}", href, name, lastmod, size);

            let date = NaiveDateTime::parse_from_str(lastmod, "%Y-%m-%d %H:%M")?;

            items.push(ListItem::new(
                href,
                name.to_string(),
                type_,
                {
                    if size == "-" {
                        None
                    } else {
                        let (n_size, unit) = FileSize::get_humanized(size);
                        Some(FileSize::HumanizedBinary(n_size, unit))
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
    use crate::listing::SizeUnit;

    use super::*;

    #[test]
    fn test_winehq_root() {
        let client = reqwest::blocking::Client::new();
        let items = ApacheF2ListingParser
            .get_list(
                &client,
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
        let client = reqwest::blocking::Client::new();
        let items = ApacheF2ListingParser
            .get_list(
                &client,
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
}
