use super::{Comparison, ExclusionManagerTrait, ExpandedRegex};

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
    pub fn new(exclusions: &[ExpandedRegex], inclusions: &[ExpandedRegex]) -> Self {
        let mut instant_stop_regexes = Vec::new();
        let mut list_only_regexes = Vec::new();

        for exclusion in exclusions {
            let regex_str = exclusion.inner.as_str();
            let mut flag = false;
            for inclusion in inclusions {
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
            include_regexes: inclusions.to_vec(),
        }
    }
}

impl ExclusionManagerTrait for ExclusionManager {
    fn match_str(&self, text: &str) -> Comparison {
        for regex in &self.instant_stop_regexes {
            if regex.is_match(text) {
                return Comparison::Stop;
            }
        }
        for regex in &self.include_regexes {
            if regex.is_match(text) {
                return Comparison::Ok;
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
    use std::str::FromStr;

    use test_log::test;
    use tracing::debug;

    use super::*;

    #[test]
    fn test_exclusion() {
        let target =
            "/debian/pmg/dists/stretch/pmgtest/binary-amd64/grub-efi-amd64-bin_2.02-pve6.changelog";
        let exclusions =
            vec![ExpandedRegex::from_str("pmg/dists/.+/pmgtest/.+changelog$").unwrap()];
        let inclusions = vec![];
        let exclusion_manager = ExclusionManager::new(&exclusions, &inclusions);
        assert_eq!(exclusion_manager.match_str(target), Comparison::Stop);
    }

    #[test]
    fn test_partial() {
        let target1 = "/yum/mysql-tools-community/fc/24/x86_64";
        let target2 = "/yum/mysql-tools-community/fc/42/x86_64";
        let target3 = "/yum/mysql-tools-community/fc/";
        let target4 = "/yum/mysql-tools-community/fc/24/";
        let target5 = "/yum/mysql-tools-community/fc/42/";
        let exclusions = vec![ExpandedRegex::from_str("/fc/").unwrap()];
        let inclusions = vec![ExpandedRegex::from_str("/fc/${FEDORA_CURRENT}").unwrap()];
        debug!("exclusions: {:?}", exclusions);
        debug!("inclusions: {:?}", inclusions);
        let exclusion_manager = ExclusionManager::new(&exclusions, &inclusions);
        assert_eq!(exclusion_manager.match_str(target1), Comparison::Stop);
        assert_eq!(exclusion_manager.match_str(target2), Comparison::Ok);
        assert_eq!(exclusion_manager.match_str(target3), Comparison::ListOnly);
        assert_eq!(exclusion_manager.match_str(target4), Comparison::Stop);
        assert_eq!(exclusion_manager.match_str(target5), Comparison::Ok);
    }

    #[test]
    fn test_exclude_dbg() {
        let target1 = "/yum/mysql-8.0-community/docker/el/8/aarch64/mysql-community-server-minimal-8.0.33-1.el8.aarch64.rpm";
        let target2 = "/yum/mysql-8.0-community/docker/el/8/debuginfo/x86_64/mysql-community-server-minimal-debuginfo-8.0.24-1.el8.x86_64.rpm";
        let exclusions = vec![
            ExpandedRegex::from_str("/el/").unwrap(),
            ExpandedRegex::from_str("debuginfo").unwrap(),
        ];
        let inclusions = vec![ExpandedRegex::from_str("/el/${RHEL_CURRENT}").unwrap()];
        let exclusion_manager = ExclusionManager::new(&exclusions, &inclusions);
        assert_eq!(exclusion_manager.match_str(target1), Comparison::Ok);
        assert_eq!(exclusion_manager.match_str(target2), Comparison::Stop);
    }
}
