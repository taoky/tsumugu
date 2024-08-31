// An inefficient fallback parser only for non-listing HTML.
// Limitations:
// 1. It requires /index.html or /index.htm available.
// Parser cannot write to disk, so index file would be accessed twice during sync.
// 2. Currently it ignores files in directories.
// For example, it recognizes "static/css.css" as contains a "static" directory only.
// In future it might be implemented when we have another parser returning a full file tree.
// 3. It would always try HEAD to get file mtime & size. Files with 403/404 code would be ignored.
// 4. It does not try parse other html files.
// 5. It only looks for <a>. <img>, <script> and other tags are ignored.

// Remember that tsumugu is NOT a nice tools when upstream does NOT show its file with size & mtime in HTML.
// This parser shall be used only as a supplementary parser.

use crate::{
    listing::{FileSize, FileType, ListItem},
    utils::{get, get_response_mtime, head},
};
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

    fn get_list(&self, async_context: &AsyncContext, url: &Url) -> Result<ListResult, ParserError> {
        let url = if !url.path().ends_with('/') {
            Url::parse(&format!("{}/", url.path())).unwrap()
        } else {
            url.clone()
        };
        let (name, resp) = {
            let mut final_resp = None;
            let mut final_name = None;
            for index in INDEX {
                let url = url.join(index).unwrap();
                let resp = get(
                    &async_context.runtime,
                    &async_context.listing_client,
                    url.clone(),
                );
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
                final_resp.ok_or(anyhow!("Does not match index list: {:?}", INDEX)),
            )
        };
        let resp = resp?;
        let name = name.unwrap();
        let mtime = get_response_mtime(&resp)
            .unwrap_or(chrono::offset::Utc::now())
            .naive_utc();
        let url = resp.url().clone();
        let body = get_text(&async_context.runtime, resp)?;
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
        let url = url.join("./").unwrap();
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
                .expect("unexpected error of handling URL");
            let type_ = if relative_href.ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };

            let item = if type_ == FileType::File {
                // Try HEAD it if it's a file
                debug!("HEADing {href} in fallback parser");
                let resp = match head(
                    &async_context.runtime,
                    &async_context.listing_client,
                    href.clone(),
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        let status = e.status();
                        if status == Some(reqwest::StatusCode::NOT_FOUND)
                            || status == Some(reqwest::StatusCode::FORBIDDEN)
                        {
                            continue;
                        }

                        // TODO: what to do here?
                        warn!("Cannot get from {}, skipping", href);
                        continue;
                    }
                };
                let size = resp.content_length();
                let mtime = match get_response_mtime(&resp) {
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
        let context = init_async_context();
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
