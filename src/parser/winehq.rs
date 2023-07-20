use crate::utils::get;

use super::Parser;
use anyhow::Result;
use scraper::{Html, Selector};

#[derive(Debug, Clone)]
pub struct WineHQListingParser;

impl Default for WineHQListingParser {
    fn default() -> Self {
        Self {}
    }
}

impl Parser for WineHQListingParser {
    fn get_list(
        &self,
        client: &reqwest::blocking::Client,
        url: &url::Url,
    ) -> Result<Vec<crate::list::ListItem>> {
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
        // let mut items = Vec::new();
        for element in indexlist.select(&selector) {
            // find <a> tag with indexcolname class
            let selector = Selector::parse("td.indexcolname a").unwrap();
            let a = element.select(&selector).next().unwrap();
            let displayed_filename = a.inner_html();
            if displayed_filename == "Parent Directory" {
                continue;
            }
            // lastmod
            let selector = Selector::parse("td.indexcollastmod").unwrap();
            let lastmod = element.select(&selector).next().unwrap();
            // size
            let selector = Selector::parse("td.indexcolsize").unwrap();
            let size = element.select(&selector).next().unwrap();

            println!(
                "{} {} {} {}",
                a.value().attr("href").unwrap(),
                a.inner_html(),
                lastmod.inner_html(),
                size.inner_html()
            );
        }
        unimplemented!()
    }
}
