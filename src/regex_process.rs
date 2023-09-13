use std::str::FromStr;

use regex::Regex;

// Submit an issue if you find this out-of-date!
// And assuming that all vars are distro_ver
const REGEX_REPLACEMENTS: &[(&str, &str)] = &[
    // https://en.wikipedia.org/wiki/Debian_version_history#Release_table
    (
        "${DEBIAN_CURRENT}",
        "(?<distro_ver>buster|bullseye|bookworm)",
    ),
    // https://en.wikipedia.org/wiki/Ubuntu_version_history#Table_of_versions
    ("${UBUNTU_LTS}", "(?<distro_ver>bionic|focal|jammy)"),
    ("${UBUNTU_NONLTS}", "(?<distro_ver>lunar|mantic)"),
    // https://en.wikipedia.org/wiki/Fedora_Linux#Releases
    ("${FEDORA_CURRENT}", "(?<distro_ver>37|38|39|40)"),
    ("${CENTOS_CURRENT}", "(?<distro_ver>7)"),
    // https://en.wikipedia.org/wiki/Red_Hat_Enterprise_Linux#Version_history_and_timeline
    ("${RHEL_CURRENT}", "(?<distro_ver>7|8|9)"),
    // https://en.wikipedia.org/wiki/OpenSUSE#Version_history
    ("${OPENSUSE_CURRENT}", "(?<distro_ver>15.4|15.5)"),
];

#[derive(Debug, Clone)]
pub struct ExpandedRegex {
    inner: Regex,
    rev_inner: Regex,
}

impl FromStr for ExpandedRegex {
    type Err = regex::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut s1 = s.to_string();
        for (from, to) in REGEX_REPLACEMENTS {
            s1 = s1.replace(from, to);
        }
        let mut s2 = s.to_string();
        for (from, _) in REGEX_REPLACEMENTS.iter().rev() {
            s2 = s2.replace(from, "(?<distro_ver>.+)");
        }
        Ok(Self {
            inner: Regex::new(&s1)?,
            rev_inner: Regex::new(&s2)?,
        })
    }
}

// Delegate to inner
impl ExpandedRegex {
    pub fn is_match(&self, text: &str) -> bool {
        self.inner.is_match(text)
    }

    pub fn is_others_match(&self, text: &str) -> bool {
        !self.inner.is_match(text) && self.rev_inner.is_match(text)
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
        // Performance: it is possible that a regex for inclusion shown like this:
        // ^fedora/${FEDORA_CURRENT}
        // And the remote corresponding folder has a lot of subfolders.
        // This is a "shortcut" to avoid checking all subfolders.
        for regex in &self.include_regexes {
            if regex.is_others_match(text) {
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

    #[test]
    fn test_exclusion() {
        let target =
            "/debian/pmg/dists/stretch/pmgtest/binary-amd64/grub-efi-amd64-bin_2.02-pve6.changelog";
        let exclusions =
            vec![ExpandedRegex::from_str("pmg/dists/.+/pmgtest/.+changelog$").unwrap()];
        let inclusions = vec![];
        let exclusion_manager = ExclusionManager::new(exclusions, inclusions);
        assert_eq!(exclusion_manager.match_str(target), Comparison::Stop);
    }
}
