use std::{collections::VecDeque, fs::File, io::Write, path::Path};

use tracing::{info, warn};
use url::Url;

mod list;
mod parser;
use list::ListItem;

use crate::{compare::should_download, parser::Parser};

mod compare;
mod utils;

enum TaskType {
    Listing,
    Download(ListItem),
}

struct Task {
    task: TaskType,
    relative: Vec<String>,
    url: Url,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let client = reqwest::Client::builder()
        .user_agent("tsumugu 0.0.1")
        .build()
        .unwrap();
    let parser = parser::nginx::NginxListingParser::default();

    let timezone = list::guess_remote_timezone(
        &parser,
        &client,
        Url::parse("https://mirrors.ustc.edu.cn/monitoring-plugins/timestamp").unwrap(),
    )
    .await;
    let timezone = match timezone {
        Ok(tz) => Some(tz),
        Err(e) => {
            warn!("Failed to guess timezone: {:?}", e);
            None
        }
    };
    info!("Guessed timezone: {:?}", timezone);

    let mut tasks: VecDeque<Task> = VecDeque::new();
    tasks.push_back(Task {
        task: TaskType::Listing,
        relative: vec![],
        url: Url::parse("https://mirrors.ustc.edu.cn/monitoring-plugins/").unwrap(),
    });

    let download_dir = Path::new("/tmp/tsumugu/");
    std::fs::create_dir_all(download_dir).unwrap();

    while let Some(task) = tasks.pop_front() {
        let cwd = download_dir.join(task.relative.join("/"));
        info!("CWD: {:?}", cwd);
        match task.task {
            TaskType::Listing => {
                info!("Listing {}", task.url);
                let items = parser.get_list(&client, &task.url).await.unwrap();
                for item in items {
                    if item.type_ == list::FileType::Directory {
                        let mut relative = task.relative.clone();
                        relative.push(item.name);
                        tasks.push_back(Task {
                            task: TaskType::Listing,
                            relative,
                            url: item.url,
                        });
                    } else {
                        tasks.push_back(Task {
                            task: TaskType::Download(item.clone()),
                            relative: task.relative.clone(),
                            url: item.url,
                        });
                    }
                }
            }
            TaskType::Download(item) => {
                // create path in case for first sync
                std::fs::create_dir_all(&cwd).unwrap();
                let expected_path = cwd.join(&item.name);
                if !should_download(&expected_path, &item, timezone) {
                    info!("Skipping {}", task.url);
                    continue;
                }
                info!("Downloading {}", task.url);
                let resp = client
                    .get(item.url)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap();
                let mtime = utils::get_response_mtime(&resp).unwrap();
                let mut dest_file = File::create(&expected_path).unwrap();
                let content = resp.bytes().await.unwrap();
                dest_file.write_all(&content).unwrap();
                filetime::set_file_handle_times(
                    &dest_file,
                    None,
                    Some(filetime::FileTime::from_system_time(mtime.into())),
                )
                .unwrap();
            }
        }
    }
}
