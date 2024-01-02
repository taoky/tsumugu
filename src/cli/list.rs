use std::path::PathBuf;

use crate::{build_client, parser::ListResult, regex_process::ExclusionManager, ListArgs};

// TODO: clean code
pub fn list(args: &ListArgs, bind_address: Option<String>) -> ! {
    let parser = args.parser.build();
    let client = build_client!(reqwest::blocking::Client, args, parser, bind_address);
    let exclusion_manager = ExclusionManager::new(&args.exclude, &args.include);
    // get relative
    let upstream = &args.upstream_folder;
    let upstream_path = PathBuf::from(upstream.path());
    let relative = upstream_path
        .strip_prefix(&args.upstream_base)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let list = parser.get_list(&client, upstream).unwrap();

    println!("Relative: {}", relative);
    println!("Exclusion: {:?}", exclusion_manager.match_str(&relative));
    match list {
        ListResult::Redirect(url) => {
            println!("Redirect to {}", url);
        }
        ListResult::List(list) => {
            for item in list {
                print!("{}", item);
                let new_relative = format!("{}/{}", relative, item.name);
                println!(
                    "{}",
                    match exclusion_manager.match_str(new_relative.as_str()) {
                        crate::regex_process::Comparison::Stop => " (stop)",
                        crate::regex_process::Comparison::ListOnly => " (list only)",
                        crate::regex_process::Comparison::Ok => "",
                    }
                );
            }
        }
    }

    std::process::exit(0);
}
