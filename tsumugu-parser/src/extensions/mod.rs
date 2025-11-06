use std::path::Path;
use tracing::{info, warn};
use url::Url;

mod apt;
mod yum;

pub struct ExtensionPackage {
    pub url: Url,
    pub relative: Vec<String>,
    pub filename: String,
}

pub fn extension_handler<F>(
    apt_packages: bool,
    yum_packages: bool,
    path: &Path,
    relative: &[String],
    url: &Url,
    push_func: F,
) where
    F: Fn(&ExtensionPackage),
{
    if apt_packages && crate::extensions::apt::is_apt_package(path) {
        let packages = apt::parse_package(path, relative, url);
        match packages {
            Err(e) => {
                warn!("Failed to parse APT package {:?}: {:?}", path, e);
            }
            Ok(packages) => {
                for package in packages {
                    info!("APT package: {:?}", package);
                    push_func(&package.into());
                }
            }
        }
    }
    if yum_packages {
        let is_primary = crate::extensions::yum::is_yum_primary_xml(path);
        let is_repomd = crate::extensions::yum::is_yum_repomd_xml(path);
        match (is_primary, is_repomd) {
            (false, false) => (),
            (p, r) => {
                assert!(!(p && r), "File is both primary and repomd");
                let xml_type = if p {
                    crate::extensions::yum::YumXmlType::Primary
                } else {
                    crate::extensions::yum::YumXmlType::Repomd
                };
                let packages = yum::parse_package(path, relative, url, xml_type);
                match packages {
                    Err(e) => {
                        warn!("Failed to parse YUM file {:?}: {:?}", path, e);
                    }
                    Ok(packages) => {
                        for package in packages {
                            info!("YUM package: {:?}", package);
                            push_func(&package.into());
                        }
                    }
                }
            }
        }
    }
}
