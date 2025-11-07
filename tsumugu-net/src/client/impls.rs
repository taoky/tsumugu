use anyhow::{Result, anyhow};
use tracing::trace;
use url::Url;

use super::*;
use crate::utils::parse_last_modified;

pub fn get_response_mtime(resp: &reqwest::Response) -> Result<DateTime<Utc>> {
    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .ok_or(anyhow!("Last-Modified header not found"))?
        .to_str()?;
    parse_last_modified(last_modified)
}

pub struct TokioHttpClient {
    pub runtime: tokio::runtime::Runtime,
    pub listing_client: reqwest::Client,
    pub download_client: reqwest::Client,
}

fn tokio_resp_to_tsumugu_resp(resp: &reqwest::Response) -> HttpResponse {
    let content_length = resp.content_length();
    let status_code = resp.status().as_u16();
    let final_url = resp.url().clone();
    let modified_time = get_response_mtime(resp);
    let headers = resp.headers().clone();
    HttpResponse {
        body: String::new(),
        final_url,
        status_code,
        content_length,
        modified_time,
        headers,
    }
}

impl TokioHttpClient {
    fn select_client(&self, req_type: RequestType) -> &reqwest::Client {
        match req_type {
            RequestType::List => &self.listing_client,
            RequestType::Download => &self.download_client,
        }
    }

    pub fn init_with_defaults() -> Self {
        let client = reqwest::Client::new();
        TokioHttpClient {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            listing_client: client.clone(),
            download_client: client,
        }
    }
}

impl HttpClient for TokioHttpClient {
    fn get_text_with_type(&self, url: &Url, req_type: RequestType) -> Result<HttpResponse> {
        let future = async {
            self.select_client(req_type)
                .get(url.clone())
                .send()
                .await?
                .error_for_status()
        };
        let resp = self.runtime.block_on(future)?;
        let http_resp = tokio_resp_to_tsumugu_resp(&resp);
        let body_text = self.runtime.block_on(resp.text())?;
        Ok(HttpResponse {
            body: body_text,
            ..http_resp
        })
    }
    fn head_with_type(&self, url: &Url, req_type: RequestType) -> Result<HttpResponse> {
        let future = async {
            self.select_client(req_type)
                .head(url.clone())
                .send()
                .await?
                .error_for_status()
        };
        let resp = self.runtime.block_on(future)?;
        trace!("HEAD {} -> {}: {:?}", url, resp.status(), resp);
        Ok(tokio_resp_to_tsumugu_resp(&resp))
    }
}
