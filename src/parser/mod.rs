use anyhow::Result;
use clap::ValueEnum;
use reqwest::blocking::Client;
use url::Url;

use crate::list::ListItem;

pub mod nginx;

pub trait Parser: Clone {
    fn get_list(&self, client: &Client, url: &Url) -> Result<Vec<ListItem>>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ParserType {
    Nginx,
}

impl ParserType {
    pub fn build(&self) -> impl Parser {
        match self {
            Self::Nginx => nginx::NginxListingParser::default(),
        }
    }
}
