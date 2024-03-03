use anyhow::Result;
use clap::ValueEnum;
use reqwest::blocking::Client;
use tracing::warn;
use url::Url;

use crate::listing::ListItem;

pub mod apache_f2;
pub mod directory_lister;
pub mod docker;
pub mod lighttpd;
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
    Lighttpd,
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
            Self::Lighttpd => Box::<lighttpd::LighttpdListingParser>::default(),
        }
    }
}

fn assert_if_url_has_no_trailing_slash(url: &Url) {
    assert!(
        url.path().ends_with('/'),
        "URL for listing should have a trailing slash"
    );
}

fn get_real_name_from_href(href: &str) -> String {
    let name: String = url::form_urlencoded::parse(href.as_bytes())
        .map(|(k, v)| [k, v].concat())
        .collect();
    name.trim_end_matches('/').to_string()
}
