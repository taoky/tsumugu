use anyhow::Result;
use reqwest::blocking::Client;
use url::Url;

use crate::list::ListItem;

pub mod nginx;

pub trait Parser {
    fn get_list(&self, client: &Client, url: &Url) -> Result<Vec<ListItem>>;
}
