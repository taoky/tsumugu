use std::str::FromStr;

use tracing::debug;

use super::{Comparison, ExclusionManagerTrait, ExpandedRegex};

#[derive(Debug, Clone)]
enum RegexType {
    Include(ExpandedRegex),
    Exclude(ExpandedRegex),
}

#[derive(Debug, Clone)]
pub struct ExclusionManager {
    regexes: Vec<RegexType>,
}

impl ExclusionManager {
    pub fn new() -> Self {
        // TODO: how to get correct order with clap?
        let args = std::env::args().collect::<Vec<_>>();
        debug!("args: {:?}", args);
        let mut regexes = Vec::new();
        let mut iter = args.iter().peekable();
        // for arg in args.iter().peekable() {
        //     if let Some(stripped) = arg.strip_prefix("--exclude=") {
        //         regexes.push(RegexType::Exclude(
        //             ExpandedRegex::from_str(stripped).expect("unexpected exclude regex"),
        //         ));
        //     } else if let Some(stripped) = arg.strip_prefix("--include=") {
        //         regexes.push(RegexType::Include(
        //             ExpandedRegex::from_str(stripped).expect("unexpected include regex"),
        //         ));
        //     }
        // }
        while let Some(arg) = iter.next() {
            if let Some(stripped) = arg.strip_prefix("--exclude=") {
                regexes.push(RegexType::Exclude(
                    ExpandedRegex::from_str(stripped).expect("unexpected exclude regex"),
                ));
            } else if let Some(stripped) = arg.strip_prefix("--include=") {
                regexes.push(RegexType::Include(
                    ExpandedRegex::from_str(stripped).expect("unexpected include regex"),
                ));
            } else if arg == "--exclude" {
                if let Some(s) = iter.peek() {
                    regexes.push(RegexType::Exclude(
                        ExpandedRegex::from_str(s).expect("unexpected exclude regex"),
                    ));
                }
            } else if arg == "--include" {
                if let Some(s) = iter.peek() {
                    regexes.push(RegexType::Include(
                        ExpandedRegex::from_str(s).expect("unexpected include regex"),
                    ));
                }
            }
        }
        debug!("regexes: {:?}", regexes);
        Self { regexes }
    }
}

impl ExclusionManagerTrait for ExclusionManager {
    fn match_str(&self, text: &str) -> Comparison {
        for regex in &self.regexes {
            match regex {
                RegexType::Exclude(regex) if regex.is_match(text) => return Comparison::Stop,
                RegexType::Include(regex) if regex.is_match(text) => return Comparison::Ok,
                _ => {}
            }
        }
        Comparison::Ok
    }
}
