use tsumugu_parser::{
    parser::{ListResult, ParserMux},
    regex_manager::Comparison,
    utils::relative_str_process,
};

use crate::{
    utils::{build_client, get_exclusion_manager},
    ListArgs,
};

use tsumugu_net::client::impls::TokioHttpClient;

// TODO: clean code
pub(crate) fn list(args: &ListArgs, bind_address: Option<String>) -> ! {
    let parser = ParserMux::new(
        args.parser.clone(),
        args.parser_match.clone(),
        args.auto_fallback,
    );
    let req_client = build_client(args, parser.is_auto_redirect(), bind_address.as_ref(), true);
    let client = TokioHttpClient {
        runtime: tokio::runtime::Runtime::new().unwrap(),
        listing_client: req_client.clone(),
        download_client: req_client,
    };
    let exclusion_manager = get_exclusion_manager(args);
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
    let list = parser
        .get_list_with_filter(&client, upstream, &relative)
        .unwrap();
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
                        Comparison::Stop => " (stop)",
                        Comparison::ListOnly => " (list only)",
                        Comparison::Ok => "",
                    }
                );
            }
        }
    }

    std::process::exit(0);
}
