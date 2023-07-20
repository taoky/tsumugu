use anyhow::Result;
use clap::ValueEnum;
use reqwest::blocking::Client;
use url::Url;

use crate::list::ListItem;

pub mod nginx;
pub mod winehq;

pub trait Parser: Sync {
    fn get_list(&self, client: &Client, url: &Url) -> Result<Vec<ListItem>>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ParserType {
    Nginx,
    Winehq,
}

impl ParserType {
    pub fn build(&self) -> Box<dyn Parser> {
        match self {
            Self::Nginx => Box::<nginx::NginxListingParser>::default(),
            Self::Winehq => Box::<winehq::WineHQListingParser>::default(),
        }
    }
}
