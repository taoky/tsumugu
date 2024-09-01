// Module for handling directory listing

use std::fmt::Display;

use chrono::{FixedOffset, NaiveDateTime};
use url::Url;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SizeUnit {
    B,
    K,
    M,
    G,
    T,
    P,
}

impl Display for SizeUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unit = match self {
            SizeUnit::B => "B",
            SizeUnit::K => "K",
            SizeUnit::M => "M",
            SizeUnit::G => "G",
            SizeUnit::T => "T",
            SizeUnit::P => "P",
        };
        write!(f, "{unit}")
    }
}

impl SizeUnit {
    pub fn get_exp(self) -> u32 {
        match self {
            SizeUnit::B => 0,
            SizeUnit::K => 1,
            SizeUnit::M => 2,
            SizeUnit::G => 3,
            SizeUnit::T => 4,
            SizeUnit::P => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileSize {
    Precise(u64),
    /// 1024B -> 1KiB
    HumanizedBinary(f64, SizeUnit),
    #[allow(dead_code)]
    /// 1000B -> 1KB
    HumanizedDecimal(f64, SizeUnit),
}

impl Display for FileSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSize::Precise(size) => write!(f, "{}", size),
            FileSize::HumanizedBinary(size, unit) => write!(f, "{size} {unit}"),
            FileSize::HumanizedDecimal(size, unit) => write!(f, "{size} {unit}"),
        }
    }
}

impl FileSize {
    pub fn get_humanized(s: &str) -> (f64, SizeUnit) {
        // seperate numeric and unit
        let mut numeric = String::new();
        let mut unit = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() || c == '.' {
                numeric.push(c);
            } else {
                unit.push(c);
            }
        }
        let unit = unit.to_lowercase();
        let unit = unit.trim();

        let numeric = numeric.parse::<f64>().unwrap();
        let unit = match unit.chars().next() {
            None => SizeUnit::B,
            Some(u) => match u {
                'b' => SizeUnit::B,
                'k' => SizeUnit::K,
                'm' => SizeUnit::M,
                'g' => SizeUnit::G,
                't' => SizeUnit::T,
                'p' => SizeUnit::P,
                _ => panic!("Unknown unit: {unit}"),
            },
        };

        (numeric, unit)
    }

    pub fn get_estimated(&self) -> u64 {
        match self {
            FileSize::Precise(size) => *size,
            FileSize::HumanizedBinary(size, unit) => {
                let exp = unit.get_exp();
                (size * 1024_f64.powi(exp as i32)) as u64
            }
            FileSize::HumanizedDecimal(size, unit) => {
                let exp = unit.get_exp();
                (size * 1000_f64.powi(exp as i32)) as u64
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub url: Url,
    pub name: String,
    pub type_: FileType,
    pub size: Option<FileSize>,
    /// mtime is parsed from HTML, which is the local datetime of the "server" (not necessarily localtime or UTC)
    pub mtime: NaiveDateTime,
    /// Some HTML provides "timezone", parser shall set this if so (otherwise just None)
    pub timezone: Option<FixedOffset>,
    /// Don't check size and mtime: download only if the file doesn't exist.
    /// This is expected to be set by apt/yum parser extension (parser will not use this).
    pub skip_check: bool,
}

impl ListItem {
    pub fn new(
        url: Url,
        name: String,
        type_: FileType,
        size: Option<FileSize>,
        mtime: NaiveDateTime,
        timezone: Option<FixedOffset>,
    ) -> Self {
        Self {
            url,
            name,
            type_,
            size,
            mtime,
            timezone,
            skip_check: false,
        }
    }
}

impl Display for ListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size_str = match self.size {
            Some(size) => size.to_string(),
            None => String::from("(none)"),
        };
        let mtime_str = self.mtime.format("%Y-%m-%d %H:%M:%S").to_string();
        let timezone = match self.timezone {
            None => "",
            Some(tz) => &format!("({})", tz),
        };
        write!(
            f,
            "{} {:?} {} {}{} {}",
            self.url, self.type_, size_str, mtime_str, timezone, self.name
        )
    }
}
