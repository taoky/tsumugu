use anyhow::Result;
use clap::ValueEnum;
use reqwest::blocking::Client;
use url::Url;

use crate::list::ListItem;

pub mod apache_f2;
pub mod nginx;

pub trait Parser: Sync {
    fn get_list(&self, client: &Client, url: &Url) -> Result<Vec<ListItem>>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ParserType {
    Nginx,
    ApacheF2,
}

impl ParserType {
    pub fn build(&self) -> Box<dyn Parser> {
        match self {
            Self::Nginx => Box::<nginx::NginxListingParser>::default(),
            Self::ApacheF2 => Box::<apache_f2::ApacheF2ListingParser>::default(),
        }
    }
}
