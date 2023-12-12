// https://httpd.apache.org/docs/2.4/mod/mod_autoindex.html
// > F=2 formats the listing as an HTMLTable FancyIndexed list

use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};

use super::{ListResult, Parser};
use anyhow::Result;
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
        assert!(
            url.path().ends_with('/'),
            "URL for listing should have a trailing slash"
        );
        let document = Html::parse_document(&body);
        // find #indexlist which contains file index
        let selector = Selector::parse("#indexlist").unwrap();
        let indexlist = document.select(&selector).next().unwrap();
        // iterate its child finding .odd and .even
        let selector = Selector::parse("tr.odd, tr.even").unwrap();
        let mut items = Vec::new();
        for element in indexlist.select(&selector) {
            // find <a> tag with indexcolname class
            let selector = Selector::parse("td.indexcolname a").unwrap();
            let a = element.select(&selector).next().unwrap();
            let displayed_filename = a.inner_html();
            if displayed_filename == "Parent Directory" {
                continue;
            }

            let href = a.value().attr("href").unwrap();
            let name: String = url::form_urlencoded::parse(href.as_bytes())
                .map(|(k, v)| [k, v].concat())
                .collect();
            let name = name.trim_end_matches('/');
            let href = url.join(href)?;
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            // lastmod
            let selector = Selector::parse("td.indexcollastmod").unwrap();
            let lastmod = element.select(&selector).next().unwrap().inner_html();
            let lastmod = lastmod.trim();
            // size
            let selector = Selector::parse("td.indexcolsize").unwrap();
            let size = element.select(&selector).next().unwrap().inner_html();
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
        let items = ApacheF2ListingParser::default()
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
}
