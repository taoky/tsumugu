use url::Url;
use reqwest;

pub enum RequestType {
    List,
    Download,
}

pub trait HttpClient {
    fn get_with_type(&self, url: Url, req_type: RequestType) -> Result<reqwest::Response, reqwest::Error>;
    fn get(&self, url: Url) -> Result<reqwest::Response, reqwest::Error> {
        self.get_with_type(url, RequestType::List)
    }

    fn get_text(&self, response: reqwest::Response) -> Result<String, reqwest::Error>;

    fn head_with_type(&self, url: Url, req_type: RequestType) -> Result<reqwest::Response, reqwest::Error>;
    fn head(&self, url: Url) -> Result<reqwest::Response, reqwest::Error> {
        self.head_with_type(url, RequestType::List)
    }
}
