use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use url::Url;

use crate::list::ListItem;

pub mod nginx;

#[async_trait]
pub trait Parser {
    async fn get_list(&self, client: &Client, url: &Url) -> Result<Vec<ListItem>>;
}
