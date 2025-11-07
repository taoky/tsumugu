use anyhow::Result;
use chrono::{DateTime, Utc};
use http::HeaderMap;
use url::Url;

pub enum RequestType {
    List,
    Download,
}

pub struct HttpResponse {
    pub body: String,
    pub final_url: Url,
    pub status_code: u16,
    pub content_length: Option<u64>,
    pub modified_time: Result<DateTime<Utc>>,
    pub headers: HeaderMap,
}

/// A trait for HTTP clients used by the parser.
pub trait HttpClient {
    fn get_text_with_type(&self, url: &Url, req_type: RequestType) -> Result<HttpResponse>;
    fn get_text(&self, url: &Url) -> Result<HttpResponse> {
        self.get_text_with_type(url, RequestType::List)
    }

    fn head_with_type(&self, url: &Url, req_type: RequestType) -> Result<HttpResponse>;
    fn head(&self, url: &Url) -> Result<HttpResponse> {
        self.head_with_type(url, RequestType::List)
    }
}

#[cfg(feature = "with-impl")]
pub mod impls;
