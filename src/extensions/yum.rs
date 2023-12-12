use std::{io::Read, path::Path};

use anyhow::Result;
use flate2::read::GzDecoder;
use tracing::info;
use url::Url;

pub fn is_yum_primary_xml(p: &Path) -> bool {
    p.file_name()
        .map(|f| f.to_str().unwrap())
        .map(|f| f.ends_with("primary.xml.gz"))
        .unwrap_or(false)
}

// read and extract location
pub fn read_primary_xml(p: &Path) -> Result<Vec<String>> {
    let re = regex::Regex::new(r#"<location href="(.+?)".*/>"#).unwrap();
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
    let packages = read_primary_xml(packages_path)?;
    let mut relative = relative.clone();
    relative.pop(); // pop "repodata"

    let mut base_url = packages_url.clone();
    base_url.path_segments_mut().unwrap().pop().pop().push("");
    info!("base_url = {:?}", base_url);
    info!("relative = {:?}", relative);

    let mut res = vec![];
    for package in packages {
        let url = base_url.join(&package)?;
        let splited: Vec<String> = package.split('/').map(|s| s.to_string()).collect();
        let mut relative = relative.clone();
        relative.append(&mut splited.clone());

        let basename = relative.pop().unwrap();
        res.push(YumPackage {
            url,
            relative,
            filename: basename,
        })
    }

    Ok(res)
}
