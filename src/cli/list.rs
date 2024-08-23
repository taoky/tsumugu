use std::path::PathBuf;

use crate::{
    parser::ListResult, regex_process::ExclusionManager, utils::build_client, AsyncContext,
    ListArgs,
};

// TODO: clean code
pub fn list(args: &ListArgs, bind_address: Option<String>) -> ! {
    let parser = args.parser.build();
    let client = build_client(args, parser.as_ref(), bind_address.as_ref(), true);
    let async_context = AsyncContext {
        runtime: tokio::runtime::Runtime::new().unwrap(),
        listing_client: client.clone(),
        download_client: client,
    };
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
    let list = parser.get_list(&async_context, upstream).unwrap();

    println!("Relative: {relative}");
    println!("Exclusion: {:?}", exclusion_manager.match_str(&relative));
    match list {
        ListResult::Redirect(url) => {
            println!("Redirect to {url}");
        }
        ListResult::List(list) => {
            for item in list {
                print!("{item}");
                let new_relative = format!("{}/{}", relative, item.name);
                tracing::debug!("new_relative: {new_relative}");
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
