use anyhow::{Result, anyhow};
use http::HeaderMap;
use tracing::trace;
use url::Url;

use super::*;
use crate::utils::parse_last_modified;

fn get_reqwest_response_mtime(resp: &reqwest::Response) -> Result<DateTime<Utc>> {
    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .ok_or(anyhow!("Last-Modified header not found"))?
        .to_str()?;
    parse_last_modified(last_modified)
}

pub fn error_status_code(err: &anyhow::Error) -> Option<u16> {
    err.downcast_ref::<reqwest::Error>()
        .and_then(|reqwest_err| reqwest_err.status())
        .map(|status| status.as_u16())
}

pub fn build_client(
    user_agent: &str,
    bind_address: Option<&str>,
    headers: HeaderMap,
    redirect: bool,
    auto_compress: bool,
) -> reqwest::Client {
    proxy_precheck();
    let minute = std::time::Duration::new(60, 0);
    let mut builder = reqwest::Client::builder()
        .user_agent(user_agent)
        .local_address(bind_address.map(|x| x.parse::<std::net::IpAddr>().unwrap()))
        .default_headers(headers)
        .connect_timeout(minute)
        .read_timeout(minute)
        .gzip(auto_compress)
        .brotli(auto_compress)
        .deflate(auto_compress);
    if !redirect {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    builder.build().unwrap()
}

pub struct DownloadResponse {
    response: reqwest::Response,
    http_response: HttpResponse,
}

impl DownloadResponse {
    pub fn http_response(&self) -> &HttpResponse {
        &self.http_response
    }

    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.response.chunk().await?.map(|chunk| chunk.to_vec()))
    }
}

pub struct TokioHttpClient {
    pub runtime: tokio::runtime::Runtime,
    pub listing_client: reqwest::Client,
    // The download client SHOULD NOT enable compression, or it might get incorrect file size in HTTP header.
    pub download_client: reqwest::Client,
}

fn tokio_resp_to_tsumugu_resp(resp: &reqwest::Response) -> HttpResponse {
    // Note: Old reqwest document is misleading about content_length.
    // See https://github.com/seanmonstar/reqwest/pull/2588
    let content_length = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    let status_code = resp.status().as_u16();
    let final_url = resp.url().clone();
    let modified_time = get_reqwest_response_mtime(resp);
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

    pub fn new(listing_client: reqwest::Client, download_client: reqwest::Client) -> Self {
        TokioHttpClient {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            listing_client,
            download_client,
        }
    }

    pub fn init_with_defaults() -> Self {
        let client = reqwest::Client::new();
        Self::new(client.clone(), client)
    }

    pub async fn download(&self, url: Url) -> Result<DownloadResponse> {
        let response = self
            .download_client
            .get(url)
            .send()
            .await?
            .error_for_status()?;
        let http_response = tokio_resp_to_tsumugu_resp(&response);
        Ok(DownloadResponse {
            response,
            http_response,
        })
    }

    pub fn head_download(&self, url: &Url) -> Result<HttpResponse> {
        let future = async {
            self.download_client
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

fn proxy_precheck() {
    let mut http_proxy = std::env::var("HTTP_PROXY");
    if http_proxy.is_err() {
        http_proxy = std::env::var("http_proxy");
    }
    let mut https_proxy = std::env::var("HTTPS_PROXY");
    if https_proxy.is_err() {
        https_proxy = std::env::var("https_proxy");
    }
    let mut all_proxy = std::env::var("ALL_PROXY");
    if all_proxy.is_err() {
        all_proxy = std::env::var("all_proxy");
    }
    if http_proxy.is_err() && https_proxy.is_err() && all_proxy.is_err() {
        trace!("No proxy environment is given.");
        return;
    }

    fn check_format(s: &str) {
        if s.starts_with("socks://") {
            tracing::warn!(
                "Typo in proxy env detected: use socks5:// or socks5h:// protocol please."
            );
            return;
        }
        if !s.starts_with("http://")
            && !s.starts_with("https://")
            && !s.starts_with("socks5://")
            && !s.starts_with("socks5h://")
        {
            tracing::warn!(
                "Unknown protocol in proxy env, this might be silently ignored by reqwest."
            );
            return;
        }
        let url = match Url::parse(s) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to parse proxy URL {}: {}", s, e);
                return;
            }
        };
        if url.scheme() == "socks5" || url.scheme() == "socks5h" {
            if let Err(e) = url.socket_addrs(|| Some(1080)) {
                tracing::warn!("Failed to get socket addr from {}: {}", url, e);
                tracing::warn!("This might be silently ignored later.");
                return;
            }
        }
        trace!("Seems OK with this proxy URL: {}", url);
    }

    if let Ok(s) = http_proxy {
        check_format(&s);
    }
    if let Ok(s) = https_proxy {
        check_format(&s);
    }
    if let Ok(s) = all_proxy {
        check_format(&s);
    }
}
