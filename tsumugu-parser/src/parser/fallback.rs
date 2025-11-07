// An inefficient fallback parser only for non-listing HTML.
// Read docs/parser.md for known limitations.

use crate::listing::{FileSize, FileType, ListItem};
use scraper::{Html, Selector};
use tracing::debug;

use super::*;

#[derive(Debug, Clone, Default)]
pub struct FallbackParser;

const INDEX: [&str; 2] = ["index.html", "index.htm"];

impl Parser for FallbackParser {
    fn name(&self) -> &'static str {
        "Fallback for non-listing directory HTML (index.html) only"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &Url) -> Result<ListResult, ParserError> {
        let url = if !url.path().ends_with('/') {
            Url::parse(&format!("{}/", url.path())).map_err(|e| {
                parse_error!(
                    "Failed to append trailing slash to URL {}: {e}",
                    url.as_str()
                )
            })?
        } else {
            url.clone()
        };
        let (name, resp) = {
            let mut final_resp = None;
            let mut final_name = None;
            for index in INDEX {
                let url = url.join(index).map_err(|e| {
                    parse_error!("Failed to join {index} to {url}: {e}, trying next index")
                })?;
                let resp = client.get_text(&url);
                match resp {
                    Ok(r) => {
                        final_resp = Some(r);
                        final_name = Some(index);
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to fetch {url}: {e}");
                        continue;
                    }
                }
            }
            (
                final_name,
                final_resp.ok_or(parse_error!("Does not match index list: {:?}", INDEX)),
            )
        };
        let resp = resp?;
        let name = name.unwrap();
        let mtime = resp
            .modified_time
            .unwrap_or(chrono::offset::Utc::now())
            .naive_utc();
        let url = resp.final_url;
        let body = resp.body;
        let size = body.len();
        let timezone = chrono::FixedOffset::east_opt(0);

        let document = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        // Add index file
        items.push(ListItem::new(
            url.clone(),
            name.to_string(),
            FileType::File,
            Some(FileSize::Precise(size as u64)),
            mtime,
            timezone,
        ));
        // Remove the "index.htm(l)" part in url
        let url = url
            .join("./")
            .map_err(|e| parse_error!("Failed to join ./ to {url}: {e}, this should not happen"))?;
        for element in document.select(&selector) {
            let href = match element.value().attr("href") {
                // well, what can I say... if you don't have href attribute?
                None => continue,
                Some(h) => h,
            };
            let href = match url.join(href) {
                Err(e) => {
                    warn!("cannot join {href} to {url}: {e}, skipping");
                    continue;
                }
                Ok(h) => h,
            };
            // Ignore if url is not a prefix of href
            if !href.as_str().starts_with(url.as_str()) {
                warn!("{href} is not inside {url}, skipping");
                continue;
            }
            let relative_href = match url.make_relative(&href) {
                None => {
                    warn!("cannot make relative of {href} from {url}");
                    continue;
                }
                Some(r) => r,
            };
            let relative_href = match relative_href.find('/') {
                Some(idx) => relative_href[..idx + 1].to_string(),
                None => relative_href,
            };
            if relative_href.is_empty() || relative_href == "/" {
                continue;
            }
            let name = get_real_name_from_href(&relative_href);
            if name.is_empty() {
                continue;
            }
            let href = url
                .join(&relative_href)
                .map_err(|e| parse_error!("unexpected error of handling URL: {}", e))?;
            let type_ = if relative_href.ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };

            // Try HEAD
            debug!("HEADing {href} in fallback parser");
            let resp = match client.head(&href) {
                Ok(r) => r,
                Err(e) => {
                    // TODO: what to do here?
                    warn!("Cannot get from {}: {}, skipping", href, e);
                    continue;
                }
            };

            let item: ListItem = if type_ == FileType::File {
                let size = resp.content_length;
                let mtime = match resp.modified_time {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Cannot get mtime from {href}: {e}, skipping");
                        continue;
                    }
                };
                let naive = mtime.naive_utc();

                ListItem::new(
                    href,
                    name.to_string(),
                    type_,
                    size.map(FileSize::Precise),
                    naive,
                    timezone,
                )
            } else {
                ListItem::new(
                    href,
                    name.to_string(),
                    type_,
                    None,
                    mtime, // mtime does not matter for dir
                    timezone,
                )
            };

            items.push(item);
        }

        Ok(ListResult::List(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::tests::*;

    #[test]
    fn test_mimalloc() {
        let context = init_client();
        let items = FallbackParser
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/buildroot/mimalloc/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0].name, "index.html");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(items[0].size, Some(FileSize::Precise(9369)));

                assert_eq!(items[3].name, "test");
                assert_eq!(items[3].type_, FileType::Directory);
            }
            _ => unreachable!(),
        }
    }
}
