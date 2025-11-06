/// A parser both suitable for default nginx autoindex and apache f1 format.
use crate::listing::{FileSize, FileType, ListItem, SizeUnit};
use chrono::{DateTime, NaiveDateTime};
use scraper::{Html, Selector};
use tracing::debug;

use super::*;
use anyhow::{anyhow, Result};
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct NginxListingParser {}

impl Parser for NginxListingParser {
    fn name(&self) -> &'static str {
        "Nginx autoindex"
    }

    fn get_list(&self, client: &dyn HttpClient, url: &url::Url) -> Result<ListResult, ParserError> {
        let resp = client.get(url.clone())?;
        let url = resp.url().clone();
        let body = client.get_text(resp)?;
        assert_if_url_has_no_trailing_slash(&url);
        let document = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut items = Vec::new();
        let mut date_fmt = None;
        let mut date_regex = None;
        for element in document.select(&selector) {
            if let Some(target) = element.value().attr("target") {
                if target == "_blank" {
                    continue;
                }
            };
            let href = match element.value().attr("href") {
                Some(href) => href,
                None => continue,
            };
            if href.starts_with('?') {
                // Apache autoindex commands, skip.
                continue;
            }
            // It's not proper to get filename by <a> text
            // As when it is too long, this could happen:
            // ceph-immutable-object-cache_17.2.6-pve1+3_amd64..> 03-May-2023 23:52              150048
            // So we should get filename from href
            let name: String = if href.contains('%') {
                get_real_name_from_href(href)
            } else {
                // A compromise for apache server (they will NOT url-encode the filename)
                // Just find the last '/' (if exists), and take substring after that
                get_last_part_from_href(href).to_string()
            };
            let href = url.join(href)?;

            let name = name.trim_end_matches('/');
            if name == ".." {
                continue;
            }
            // extra check for Apache server
            let inner = element.inner_html();
            if inner == "Parent Directory" {
                continue;
            }
            let type_ = if href.as_str().ends_with('/') {
                FileType::Directory
            } else {
                FileType::File
            };
            let metadata_raw = element
                .next_sibling()
                .ok_or(anyhow!("No metadata found for <a> element"))?
                .value()
                .as_text()
                .ok_or(anyhow!("No text found in next sibling of <a> element"))?
                .to_string();
            let metadata_raw = &date_normalization(metadata_raw.trim());
            debug!("{:?}", metadata_raw);
            // if it's a directory, and metadata are "- -", do some special handling for the date fmt...
            // as directory mtime is useless, and nodejs gives us something like this...
            let mut skip_date = false;
            if type_ == FileType::Directory
                && metadata_raw
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    == "--"
            {
                skip_date = true;
            }
            // guess date format...
            let date;
            let size;
            if !skip_date {
                if date_fmt.is_none() {
                    let (f, r) = guess_date_fmt(metadata_raw);
                    date_fmt = Some(f);
                    date_regex = Some(Regex::new(&format!(
                        r"({})\s+([\d\.\-]+ ?(?:[kKMGB]|Bytes)*)$",
                        r
                    ))?);
                    debug!("date_fmt: {:?} date_regex: {:?}", date_fmt, date_regex)
                }
                let metadata =
                    date_regex
                        .clone()
                        .unwrap()
                        .captures(metadata_raw)
                        .ok_or(anyhow!(
                            "Get '{}' for {} ({}) metadata, is this a nginx page?",
                            metadata_raw,
                            name,
                            href
                        ))?;
                date = NaiveDateTime::parse_from_str(
                    metadata
                        .get(1)
                        .ok_or(anyhow!("Cannot get date in metadata"))?
                        .as_str(),
                    &date_fmt.clone().unwrap(),
                )?;
                size = metadata
                    .get(2)
                    .ok_or(anyhow!("Cannot get size in metadata"))?
                    .as_str();
            } else {
                date = DateTime::UNIX_EPOCH.naive_utc();
                size = "-";
            }

            debug!("{} {} {:?} {} {:?}", href, name, type_, date, size);
            items.push(ListItem::new(
                href,
                name.to_string(),
                type_,
                {
                    if size == "-" {
                        None
                    } else if size.contains('k')
                        || size.contains('K')
                        || size.contains('M')
                        || size.contains('G')
                        || size.contains('B')
                    {
                        let (n_size, unit) = FileSize::get_humanized(size);
                        if unit != SizeUnit::B {
                            Some(FileSize::HumanizedBinary(n_size, unit))
                        } else {
                            Some(FileSize::Precise(n_size as u64)) // workaround
                        }
                    } else {
                        let n_size = size.parse::<u64>().map_err(|e| {
                            anyhow!("Failed to parse size '{}' for {} as u64: {}", size, name, e)
                        })?;
                        Some(FileSize::Precise(n_size))
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
    use test_log::test;
    use url::Url;

    use crate::listing::SizeUnit;

    use super::*;
    use crate::parser::tests::*;

    #[test]
    fn test_monitoring_plugins() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/monitoring-plugins").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 23);
                assert_eq!(items[0].name, "archive");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("09-Oct-2015 16:12", "%d-%b-%Y %H:%M").unwrap()
                );
                assert_eq!(items[4].name, "monitoring-plugins-2.0.tar.gz");
                assert_eq!(items[4].type_, FileType::File);
                assert_eq!(items[4].size, Some(FileSize::Precise(2610000)));
                assert_eq!(
                    items[4].mtime,
                    NaiveDateTime::parse_from_str("11-Jul-2014 23:17", "%d-%b-%Y %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_proxmox() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/proxmox").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                let target = "ceph-immutable-object-cache_17.2.6-pve1+3_amd64.deb";
                let find_res = items.iter().find(|item| item.name == target).unwrap();
                assert_eq!(find_res.name, target);
                assert_eq!(find_res.type_, FileType::File);
                // keep as-is
                assert_eq!(find_res.url, Url::parse("http://localhost:1921/proxmox/ceph-immutable-object-cache_17.2.6-pve1%2B3_amd64.deb").unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_mysql() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/mysql").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                let target = "mysql-connector-c++";
                let find_res = items.iter().find(|item| item.name == target).unwrap();
                assert_eq!(find_res.name, target);
                assert_eq!(find_res.type_, FileType::Directory);
                // keep as-is
                assert_eq!(
                    find_res.url,
                    Url::parse("http://localhost:1921/mysql/mysql-connector-c++/").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_ghettoforge() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/ghettoforge").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 8);
                assert_eq!(items[0].name, "RPM-GPG-KEY-gf.el7");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(3.0, SizeUnit::K))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2014-12-30 02:53", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[3].name, "archive");
                assert_eq!(items[3].type_, FileType::Directory);
                assert_eq!(items[3].size, None);
                assert_eq!(
                    items[3].mtime,
                    NaiveDateTime::parse_from_str("2020-12-21 02:34", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_artifactrepo() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/artifactrepo/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 44);
                assert_eq!(items[0].name, "10");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2021-08-22 15:18", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/artifactrepo/10/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].name, "openjdk-10_linux-x64_bin.tar.gz");
                assert_eq!(items[0].type_, FileType::File);
                assert_eq!(
                    items[0].size,
                    Some(FileSize::HumanizedBinary(195.38, SizeUnit::M))
                );
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2018-03-08 10:06", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_zabbix() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/zabbix/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 5);
                assert_eq!(items[0].name, "appliances");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2020-07-27 11:06", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_nodejs() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/nodejs/v4.9.1/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 37);
                assert_eq!(items[0].name, "docs");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    DateTime::UNIX_EPOCH.naive_utc(), // No mtime
                );
                assert_eq!(items[3].name, "SHASUMS256.txt.asc");
                assert_eq!(items[3].type_, FileType::File);
                assert_eq!(
                    items[3].size,
                    Some(FileSize::HumanizedBinary(4.1, SizeUnit::K))
                );
                assert_eq!(
                    items[3].mtime,
                    NaiveDateTime::parse_from_str("2024-11-04 17:40", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/nodejs/latest-jod/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 36);
                assert_eq!(items[0].name, "docs");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, None);
                assert_eq!(
                    items[0].mtime,
                    DateTime::UNIX_EPOCH.naive_utc(), // No mtime
                );
                assert_eq!(items[4].name, "SHASUMS256.txt.asc");
                assert_eq!(items[4].type_, FileType::File);
                assert_eq!(
                    items[4].size,
                    Some(FileSize::HumanizedBinary(4.7, SizeUnit::K))
                );
                assert_eq!(
                    items[4].mtime,
                    NaiveDateTime::parse_from_str("2025-09-24 13:12", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_vscode() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/vscode/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].name, "dists");
                assert_eq!(items[0].type_, FileType::Directory);
                assert_eq!(items[0].size, Some(FileSize::Precise(481)));
                assert_eq!(
                    items[0].mtime,
                    NaiveDateTime::parse_from_str("2025-03-06 07:19", "%Y-%m-%d %H:%M").unwrap()
                );
                assert_eq!(items[1].name, "pool");
                assert_eq!(items[1].type_, FileType::Directory);
                assert_eq!(
                    items[1].size,
                    Some(FileSize::HumanizedBinary(104.1, SizeUnit::M))
                );
                assert_eq!(
                    items[1].mtime,
                    NaiveDateTime::parse_from_str("2024-09-05 18:01", "%Y-%m-%d %H:%M").unwrap()
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_empty() {
        let context = init_client();
        let items = NginxListingParser::default()
            .get_list(
                &context,
                &url::Url::parse("http://localhost:1921/postgresql/empty/").unwrap(),
            )
            .unwrap();
        match items {
            ListResult::List(items) => {
                assert_eq!(items.len(), 0);
            }
            _ => unreachable!(),
        }
    }
}
