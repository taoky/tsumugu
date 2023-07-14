use url::Url;

mod list;

#[tokio::main]
async fn main() {
    let client = reqwest::Client::builder()
        .user_agent("tsumugu 0.0.1")
        .build()
        .unwrap();
    let items = list::get_list(
        &client,
        &Url::parse("https://mirrors.ustc.edu.cn/rfc/").unwrap(),
    )
    .await;
    println!("{:?}", items);
}
