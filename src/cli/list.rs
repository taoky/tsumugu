use crate::{
    parser::ListResult,
    regex_manager::v1::ExclusionManager,
    regex_manager::{Comparison, ExclusionManagerTrait},
    utils::{build_client, relative_str_process},
    AsyncContext, ListArgs,
};

// TODO: clean code
pub fn list(args: &ListArgs, bind_address: Option<String>) -> ! {
    let parser = args.parser.build();
    let client = build_client(args, parser.is_auto_redirect(), bind_address.as_ref(), true);
    let async_context = AsyncContext {
        runtime: tokio::runtime::Runtime::new().unwrap(),
        listing_client: client.clone(),
        download_client: client,
    };
    let exclusion_manager = ExclusionManager::new(&args.exclude, &args.include);
    // get relative
    let upstream = &args.upstream;
    let upstream_path = parser.get_path(upstream);
    let relative = upstream_path
        .strip_prefix(&args.upstream_base)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let relative = relative_str_process(&relative);
    assert!(relative.starts_with('/') && relative.ends_with('/'));
    let list = parser.get_list(&async_context, upstream).unwrap();
    let match_cmp = exclusion_manager.match_str(&relative);

    println!("Relative: {relative}");
    println!("Exclusion: {:?}", match_cmp);
    if match_cmp == Comparison::Stop {
        tracing::warn!("This listing would NOT be accessed at all.");
    }
    match list {
        ListResult::Redirect(url) => {
            println!("Redirect to {url}");
        }
        ListResult::List(list) => {
            for item in list {
                print!("{item}");
                let new_relative = format!("{}{}", relative, item.name);
                tracing::debug!("new_relative: {new_relative}");
                println!(
                    "{}",
                    match exclusion_manager.match_str(new_relative.as_str()) {
                        crate::regex_manager::Comparison::Stop => " (stop)",
                        crate::regex_manager::Comparison::ListOnly => " (list only)",
                        crate::regex_manager::Comparison::Ok => "",
                    }
                );
            }
        }
    }

    std::process::exit(0);
}
