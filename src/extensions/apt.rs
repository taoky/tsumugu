use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::warn;
use url::Url;

pub fn is_apt_package(p: &Path) -> bool {
    // check if basename is Packages
    let basename = p.file_name().unwrap().to_str().unwrap();
    if basename != "Packages" {
        return false;
    }
    // check if parents contain dists
    let parents = p.ancestors();
    for iter in parents {
        let basename = iter.file_name().unwrap().to_str().unwrap();
        if basename == "dists" {
            return true;
        }
    }
    false
}

// In every iter packages_path and packages_url be updated to their parents
// When they reach the dists directory, return the root of debian
// Otherwise when one of them reach the root, return error
fn get_debian_root(
    packages_path: &Path,
    relative: &[String],
    packages_url: &Url,
) -> Result<(PathBuf, Vec<String>, Url)> {
    fn pop(p: &mut PathBuf, r: Option<&mut Vec<String>>, u: &mut Url) -> Result<()> {
        if !p.pop() {
            return Err(anyhow::anyhow!(
                "Cannot find debian root (path can not be popped, path = {:?})",
                p
            ));
        }
        if u.path() == "/" {
            return Err(anyhow::anyhow!(
                "Cannot find debian root (url can not be popped, url = {:?})",
                u
            ));
        }
        if let Some(r) = r {
            if r.pop().is_none() {
                return Err(anyhow::anyhow!(
                    "Cannot find debian root (relative can not be popped, relative = {:?})",
                    r
                ));
            }
        }
        u.path_segments_mut().unwrap().pop();
        Ok(())
    }
    let mut packages_path = packages_path.to_path_buf();
    let mut relative = relative.to_owned();
    let mut packages_url = packages_url.clone();
    // first pop of file name to match relative
    pop(&mut packages_path, None, &mut packages_url)?;
    loop {
        let basename = packages_path.file_name().unwrap().to_str().unwrap();
        let url_basename = packages_url.path_segments().unwrap().last().unwrap();
        if basename == "dists" && url_basename == "dists" {
            // we don't wanna dists folder in return value
            pop(&mut packages_path, Some(&mut relative), &mut packages_url)?;
            // add trailing slash to packages_url
            packages_url.path_segments_mut().unwrap().push("");
            return Ok((packages_path, relative, packages_url));
        }
        if basename != url_basename {
            warn!(
                "basename = {}, url_basename = {}, relative = {:?}",
                basename, url_basename, relative
            );
        }
        pop(&mut packages_path, Some(&mut relative), &mut packages_url)?;
    }
}

#[derive(Debug)]
pub struct AptPackage {
    pub url: Url,
    pub relative: Vec<String>,
    pub size: usize,
    pub filename: String,
}

impl From<AptPackage> for super::ExtensionPackage {
    fn from(val: AptPackage) -> Self {
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
) -> Result<Vec<AptPackage>> {
    let data = std::fs::read_to_string(packages_path)?;
    let packages = apt_parser::Packages::from(&data);
    let (_, root_relative, debian_root_url) =
        get_debian_root(packages_path, relative, packages_url)?;
    // ignore errors
    let mut res = vec![];
    for package in packages {
        let pool_url = package.filename;
        let size = package.size;
        let url = debian_root_url.join(&pool_url)?;

        let mut pool_splited: Vec<String> = pool_url.split('/').map(|s| s.to_string()).collect();
        let mut relative = root_relative.clone();
        relative.append(&mut pool_splited);

        let basename = relative.pop().unwrap();

        res.push(AptPackage {
            url,
            relative,
            size: size as usize,
            filename: basename,
        })
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test]
    fn test_debian_root() {
        let packages_path = Path::new("/var/www/html/dists/buster/main/binary-amd64/Packages");
        let relative = vec![
            "dists".to_string(),
            "buster".to_string(),
            "main".to_string(),
            "binary-amd64".to_string(),
        ];
        let packages_url =
            Url::parse("http://localhost/dists/buster/main/binary-amd64/Packages").unwrap();
        let (debian_root_path, root_relative, debian_root_url) =
            get_debian_root(packages_path, &relative, &packages_url).unwrap();
        assert_eq!(debian_root_path, Path::new("/var/www/html/"));
        assert_eq!(root_relative, Vec::<String>::new());
        assert_eq!(debian_root_url, Url::parse("http://localhost/").unwrap());

        let packages_path =
            Path::new("/var/www/html/mysql/apt/ubuntu/dists/jammy/mysql-8.0/binary-amd64/Packages");
        let relative = vec![
            "apt".to_string(),
            "ubuntu".to_string(),
            "dists".to_string(),
            "jammy".to_string(),
            "mysql-8.0".to_string(),
            "binary-amd64".to_string(),
        ];
        let packages_url = Url::parse(
            "http://repo.mysql.com/apt/ubuntu/dists/jammy/mysql-8.0/binary-amd64/Packages",
        )
        .unwrap();
        let (debian_root_path, root_relative, debian_root_url) =
            get_debian_root(packages_path, &relative, &packages_url).unwrap();
        assert_eq!(
            debian_root_path,
            Path::new("/var/www/html/mysql/apt/ubuntu/")
        );
        assert_eq!(root_relative, vec!["apt".to_string(), "ubuntu".to_string()]);
        assert_eq!(
            debian_root_url,
            Url::parse("http://repo.mysql.com/apt/ubuntu/").unwrap()
        );
    }
}
