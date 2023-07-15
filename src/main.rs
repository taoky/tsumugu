use std::collections::VecDeque;

use tracing::info;
use url::Url;

mod list;

enum TaskType {
    Listing,
    Download
}

struct Task {
    type_: TaskType,
    url: Url
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let client = reqwest::Client::builder()
        .user_agent("tsumugu 0.0.1")
        .build()
        .unwrap();

    let timezone = list::guess_remote_timezone(
        &client,
        &Url::parse("https://mirrors.ustc.edu.cn/rfc/beta/errata/css/errata-base.css").unwrap(),
    )
    .await;
    info!("Guessed timezone: {:?}", timezone);

    let mut tasks: VecDeque<Task> = VecDeque::new();
    tasks.push_back(Task { type_: TaskType::Listing, url: Url::parse("https://mirrors.ustc.edu.cn/rfc/").unwrap() });

    let download_dir = "/tmp/tsumugu/";

    while let Some(task) = tasks.pop_front() {
        match task.type_ {
            TaskType::Listing => {
                println!("Listing {}", task.url);
                let items = list::get_list(&client, &task.url).await.unwrap();
                for item in items {
                    if item.type_ == list::FileType::Directory {
                        tasks.push_back(Task { type_: TaskType::Listing, url: Url::parse(&item.url).unwrap() });
                    } else {
                        tasks.push_back(Task { type_: TaskType::Download, url: Url::parse(&item.url).unwrap() });
                    }
                }
            },
            TaskType::Download => {
                println!("Downloading {}", task.url);
            }
        }
    }
}
