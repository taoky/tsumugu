use std::{path::Path, io::Read};

use anyhow::Result;
use flate2::read::GzDecoder;
use url::Url;

pub fn is_yum_primary_xml(p: &Path) -> bool {
    p.file_name()
        .map(|f| f.to_str().unwrap())
        .map(|f| f.ends_with("primary.xml.gz"))
        .unwrap_or(false)
}

// read and extract location
pub fn read_primary_xml(p: &Path) -> Result<Vec<String>> {
    let re = regex::Regex::new(r#"<location href="(.+?)" />"#).unwrap();
    let bytes = std::fs::read(p)?;
    let mut gzd = GzDecoder::new(&bytes[..]);
    let mut s = String::new();
    gzd.read_to_string(&mut s)?;

    let mut urls = Vec::new();
    for line in s.lines() {
        if let Some(caps) = re.captures(line) {
            let url = caps.get(1).unwrap().as_str();
            urls.push(url.to_string());
        }
    }
    Ok(urls)
}

#[derive(Debug)]
pub struct YumPackage {
    pub url: Url,
    pub relative: Vec<String>,
    pub filename: String,
}

pub fn parse_package(
    packages_path: &Path,
    relative: Vec<String>,
    packages_url: &Url,
) -> Result<Vec<YumPackage>> {

    unimplemented!()
}