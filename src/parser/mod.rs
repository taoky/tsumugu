use anyhow::Result;
use clap::ValueEnum;
use reqwest::blocking::Client;
use tracing::warn;
use url::Url;

use crate::listing::ListItem;

pub mod apache_f2;
pub mod directory_lister;
pub mod docker;
pub mod nginx;

#[derive(Debug)]
pub enum ListResult {
    List(Vec<ListItem>),
    Redirect(String),
}

pub trait Parser: Sync {
    fn get_list(&self, client: &Client, url: &Url) -> Result<ListResult>;
    fn is_auto_redirect(&self) -> bool {
        true
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ParserType {
    Nginx,
    ApacheF2,
    Docker,
    DirectoryLister,
}

impl ParserType {
    pub fn build(&self) -> Box<dyn Parser> {
        match self {
            Self::Nginx => Box::<nginx::NginxListingParser>::default(),
            Self::ApacheF2 => Box::<apache_f2::ApacheF2ListingParser>::default(),
            Self::Docker => Box::<docker::DockerListingParser>::default(),
            Self::DirectoryLister => {
                warn!("html5ever parser does not support foster parenting. The result may be incorrect.");
                Box::<directory_lister::DirectoryListerListingParser>::default()
            }
        }
    }
}
