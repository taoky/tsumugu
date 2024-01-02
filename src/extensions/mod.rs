use crate::SyncArgs;
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
    args: &SyncArgs,
    path: &Path,
    relative: &[String],
    url: &Url,
    push_func: F,
) where
    F: Fn(&ExtensionPackage),
{
    if args.apt_packages && crate::extensions::apt::is_apt_package(path) {
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
    if args.yum_packages && crate::extensions::yum::is_yum_primary_xml(path) {
        let packages = yum::parse_package(path, relative, url);
        match packages {
            Err(e) => {
                warn!("Failed to parse YUM primary.xml {:?}: {:?}", path, e);
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
