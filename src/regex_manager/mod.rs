pub mod v1;

use std::str::FromStr;

use regex::Regex;

// Submit an issue if you find this out-of-date!
// And assuming that all vars are distro_ver
const REGEX_REPLACEMENTS: &[(&str, &str)] = &[
    // https://endoflife.date/debian
    ("${DEBIAN_CURRENT}", "(?<distro_ver>bullseye|bookworm)"),
    // https://endoflife.date/ubuntu (excluding ESM)
    ("${UBUNTU_LTS}", "(?<distro_ver>focal|jammy|noble)"),
    ("${UBUNTU_NONLTS}", "(?<distro_ver>oracular)"),
    // https://endoflife.date/fedora
    ("${FEDORA_CURRENT}", "(?<distro_ver>39|40|41)"),
    // CentOS is no longer supported -- this regex is replaced to something that could match nothing
    (
        "${CENTOS_CURRENT}",
        "(?<distro_ver>NONEXISTFILENAMESOITCOULDNEVERMATCHANYTHING)",
    ),
    // https://endoflife.date/rhel (excluding ELCS)
    ("${RHEL_CURRENT}", "(?<distro_ver>8|9)"),
    // https://endoflife.date/opensuse
    ("${OPENSUSE_CURRENT}", "(?<distro_ver>15.5|15.6)"),
    // https://endoflife.date/sles
    ("${SLES_CURRENT}", "(?<distro_ver>12|15)"),
];

/// ExpandedRegex contains inner and rev_inner, and would transparently add '/' before string
/// (and convert regex with ^). A warning would be given if text input contains '/' at front.
#[derive(Debug, Clone)]
pub struct ExpandedRegex {
    pub inner: Regex,
    /// v1 compatibility field
    rev_inner: Regex,
}

impl FromStr for ExpandedRegex {
    type Err = regex::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // If starts with ^ and not ^/, change start matching character from ^ to ^/
        let s = if s.starts_with('^') && !s.starts_with("^/") {
            &format!("^/{}", &s[1..])
        } else {
            s
        };
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
    fn text_transform(text: &str) -> String {
        if !text.starts_with('/') {
            tracing::warn!("(unexpected internal input: string given to match_str shall start with /, anything wrong?)");
            format!("/{}", text)
        } else {
            text.to_string()
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.inner.is_match(&Self::text_transform(text))
    }

    /// v1 compatibility method
    pub fn is_others_match(&self, text: &str) -> bool {
        let text = &Self::text_transform(text);
        !self.inner.is_match(text) && self.rev_inner.is_match(text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comparison {
    Stop,
    /// v1 compatibility field
    ListOnly,
    Ok,
}

pub trait ExclusionManagerTrait {
    fn match_str(&self, text: &str) -> Comparison;
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
