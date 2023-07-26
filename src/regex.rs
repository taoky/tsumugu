use std::str::FromStr;

use regex::Regex;

// Submit an issue if you find this out-of-date!
const REGEX_REPLACEMENTS: &[(&str, &str)] = &[
    ("${DEBIAN_CURRENT}", "(buster|bullseye|bookworm)"),
    ("${UBUNTU_LTS}", "(bionic|focal|jammy)"),
    ("${FEDORA_CURRENT}", "(37|38)"),
    ("${CENTOS_CURRENT}", "(7)"),
    ("${RHEL_CURRENT}", "(7|8|9)"),
    ("${OPENSUSE_CURRENT}", "(15.4|15.5)"),
];

#[derive(Debug, Clone)]
pub struct ExpandedRegex {
    inner: Regex,
}

impl FromStr for ExpandedRegex {
    type Err = regex::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut s = s.to_string();
        for (from, to) in REGEX_REPLACEMENTS {
            s = s.replace(from, to);
        }
        Ok(Self {
            inner: Regex::new(&s)?,
        })
    }
}

// Delegate to inner
impl ExpandedRegex {
    pub fn is_match(&self, text: &str) -> bool {
        self.inner.is_match(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expanded_regex() {
        let regex = ExpandedRegex::from_str("^/deb/dists/${DEBIAN_CURRENT}").unwrap();
        assert!(regex.is_match("/deb/dists/bookworm/Release"));
        assert!(!regex.is_match("/deb/dists/wheezy/Release"));
    }
}
