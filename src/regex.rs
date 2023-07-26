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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comparison {
    Stop,
    ListOnly,
    Ok,
}

#[derive(Debug, Clone)]
pub struct ExclusionManager {
    /// Stop the task immediately if any of these regexes match.
    instant_stop_regexes: Vec<ExpandedRegex>,
    /// Continue, but don't download anything if any of these regexes match.
    list_only_regexes: Vec<ExpandedRegex>,
    /// Include only these regexes.
    include_regexes: Vec<ExpandedRegex>,
}

impl ExclusionManager {
    pub fn new(exclusions: Vec<ExpandedRegex>, inclusions: Vec<ExpandedRegex>) -> Self {
        let mut instant_stop_regexes = Vec::new();
        let mut list_only_regexes = Vec::new();

        for exclusion in exclusions {
            let regex_str = exclusion.inner.as_str();
            let mut flag = false;
            for inclusion in &inclusions {
                if inclusion.inner.as_str().starts_with(regex_str) {
                    list_only_regexes.push(exclusion.clone());
                    flag = true;
                    break;
                }
            }
            if !flag {
                instant_stop_regexes.push(exclusion.clone());
            }
        }

        Self {
            instant_stop_regexes,
            list_only_regexes,
            include_regexes: inclusions,
        }
    }

    pub fn match_str(&self, text: &str) -> Comparison {
        for regex in &self.include_regexes {
            if regex.is_match(text) {
                return Comparison::Ok;
            }
        }
        for regex in &self.instant_stop_regexes {
            if regex.is_match(text) {
                return Comparison::Stop;
            }
        }
        for regex in &self.list_only_regexes {
            if regex.is_match(text) {
                return Comparison::ListOnly;
            }
        }
        Comparison::Ok
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
