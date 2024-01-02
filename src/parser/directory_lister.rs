use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::get,
};

use super::{ListResult, Parser};
use anyhow::Result;
use chrono::NaiveDateTime;
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct DirectoryListerListingParser;

impl Parser for DirectoryListerListingParser {
    fn get_list(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<ListResult> {
        let resp = get(client, url.clone())?;
        let url = resp.url().clone();
        let body = resp.text()?;
        assert!(
            url.path().ends_with('/'),
            "URL for listing should have a trailing slash"
        );
        let document = Html::parse_document(&body);
        // https://github.com/DirectoryLister/DirectoryLister/blob/0283f14aa1fbd97796f753e8d6105c752546050f/app/views/components/file.twig

        // find <ul> which contains file index
        let selector = Selector::parse("ul").unwrap();
        let indexlist = document.select(&selector).next().unwrap();
        // find second <li>
        let selector = Selector::parse("li").unwrap();
        let indexlist = indexlist.select(&selector).nth(1).unwrap();
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        for element in indexlist.select(&selector) {
            let href = element.value().attr("href").unwrap();
            let href = url.join(href)?;
            // displayed file name, class = "flex-1 truncate"
            let selector = Selector::parse("div.flex-1.truncate").unwrap();
            let displayed_filename = element.select(&selector).next().unwrap().inner_html();
            let displayed_filename = displayed_filename.trim();
            // size, class = "hidden whitespace-nowrap text-right mx-2 w-1/6 sm:block"
            let selector = Selector::parse("div.hidden.whitespace-nowrap.text-right.mx-2").unwrap();
            let size = element.select(&selector).next().unwrap().inner_html();
            let size = size.trim();
            // mtime, class = "hidden whitespace-nowrap text-right truncate ml-2 w-1/4 sm:block"
            let selector =
                Selector::parse("div.hidden.whitespace-nowrap.text-right.truncate.ml-2").unwrap();
            let mtime = element.select(&selector).next().unwrap().inner_html();
            let mtime = mtime.trim();

            if displayed_filename == ".." {
                continue;
            }
            let type_ = if size == "—" {
                FileType::Directory
            } else {
                FileType::File
            };
            let date = NaiveDateTime::parse_from_str(mtime, "%Y-%m-%d %H:%M:%S")?;
            items.push(ListItem::new(
                href,
                displayed_filename.to_string(),
                type_,
                {
                    if size == "—" {
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
    use url::Url;

    use crate::listing::SizeUnit;

    use super::*;

    #[test]
    fn test_vyos() {
        let client = reqwest::blocking::Client::new();
        let items = DirectoryListerListingParser
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/vyos/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 7);
                assert_eq!(items[0].name, "main");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2023-08-07 21:11:02", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                assert_eq!(
                    items[0].url,
                    Url::parse(
                        "http://localhost:1921/vyos/?dir=repositories/current/dists/current/main"
                    )
                    .unwrap()
                );
                assert_eq!(items[4].name, "Contents-amd64.gz");
                assert_eq!(items[4].type_, FileType::File);
                assert_eq!(
                    items[4].size,
                    Some(FileSize::HumanizedBinary(1.80, SizeUnit::M))
                );
                assert_eq!(
                    items[4].mtime,
                    NaiveDateTime::parse_from_str("2023-08-07 21:10:57", "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                );
                assert_eq!(items[4].url, Url::parse("http://localhost:1921/vyos/repositories/current/dists/current/Contents-amd64.gz").unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_vyos_2() {
        let client = reqwest::blocking::Client::new();
        let items = DirectoryListerListingParser
            .get_list(
                &client,
                &url::Url::parse("http://localhost:1921/vyos/vyos-accel-ppp/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => unreachable!(),
        }
    }
}
