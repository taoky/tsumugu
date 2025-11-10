use std::{io::Read, path::Path};

use anyhow::Result;
use flate2::read::GzDecoder;
use tracing::info;
use url::Url;

fn get_locations_from_xml(s: &str) -> Vec<String> {
    let re = regex::Regex::new(r#"<location href="(.+?)".*/>"#).unwrap();
    let mut urls = Vec::new();
    for line in s.lines() {
        if let Some(caps) = re.captures(line) {
            let url = caps.get(1).unwrap().as_str();
            urls.push(url.to_string());
        }
    }
    urls
}

pub fn is_yum_primary_xml(p: &Path) -> bool {
    p.file_name()
        .map(|f| f.to_str().unwrap())
        .map(|f| f.ends_with("primary.xml.gz"))
        .unwrap_or(false)
}

// read and extract location
pub fn read_primary_xml(p: &Path) -> Result<Vec<String>> {
    let bytes = std::fs::read(p)?;
    let mut gzd = GzDecoder::new(&bytes[..]);
    let mut s = String::new();
    gzd.read_to_string(&mut s)?;

    Ok(get_locations_from_xml(&s))
}

pub enum YumXmlType {
    Primary,
    Repomd,
}

#[derive(Debug)]
pub struct YumPackage {
    pub url: Url,
    pub relative: Vec<String>,
    pub filename: String,
}

impl From<YumPackage> for super::ExtensionPackage {
    fn from(val: YumPackage) -> Self {
        super::ExtensionPackage {
            url: val.url,
            relative: val.relative,
            filename: val.filename,
        }
    }
}

pub fn parse_package(
    packages_path: &Path,
    relative: &[String],
    packages_url: &Url,
    xml_type: YumXmlType,
) -> Result<Vec<YumPackage>> {
    let packages = match xml_type {
        YumXmlType::Primary => read_primary_xml(packages_path)?,
        YumXmlType::Repomd => read_yum_repomd_xml(packages_path)?,
    };
    let mut relative = relative.to_owned();
    relative.pop(); // pop "repodata"

    let mut base_url = packages_url.clone();
    base_url.path_segments_mut().unwrap().pop().pop().push("");
    info!("base_url = {:?}", base_url);
    info!("relative = {:?}", relative);

    let mut res = vec![];
    for package in packages {
        let url = base_url.join(&package)?;
        let split: Vec<String> = package.split('/').map(|s| s.to_string()).collect();
        let mut relative = relative.clone();
        relative.append(&mut split.clone());

        let basename = relative.pop().unwrap();
        res.push(YumPackage {
            url,
            relative,
            filename: basename,
        })
    }

    Ok(res)
}

// Well, brain-damaged mysql-repo even cannot show all primary.xml.gz...
// So I have to use repomd.xml to get primary.xml.gz...
// Good news is that it seems like existing functions for handling primary.xml.gz can be reused.
pub fn is_yum_repomd_xml(p: &Path) -> bool {
    p.file_name()
        .map(|f| f.to_str().unwrap())
        .map(|f| f == "repomd.xml")
        .unwrap_or(false)
}

pub fn read_yum_repomd_xml(p: &Path) -> Result<Vec<String>> {
    let bytes = std::fs::read(p)?;
    let s = String::from_utf8_lossy(&bytes);

    Ok(get_locations_from_xml(s.as_ref()))
}
