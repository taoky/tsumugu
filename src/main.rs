use std::path::PathBuf;

use clap::{Parser, Subcommand};

use parser::ParserType;
use tracing_subscriber::EnvFilter;
use url::Url;

mod cli;
mod compare;
mod listing;
mod parser;
mod regex_process;
mod term;
mod utils;

use crate::regex_process::ExpandedRegex;

#[derive(Parser, Debug)]
#[command(about, version)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sync files from upstream to local.
    Sync(SyncArgs),

    /// List files from upstream.
    List(ListArgs),
}

#[derive(Parser, Debug)]
pub struct SyncArgs {
    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// Do not download files and cleanup.
    #[clap(long)]
    dry_run: bool,

    /// Threads at work.
    #[clap(long, default_value_t = 2)]
    threads: usize,

    /// Do not clean up after sync.
    #[clap(long)]
    no_delete: bool,

    /// Set max delete count.
    #[clap(long, default_value_t = 100)]
    max_delete: usize,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream: Url,

    /// The local directory.
    #[clap(value_parser)]
    local: PathBuf,

    /// Default: auto. You can set a valid URL for guessing, or an invalid one for disabling.
    #[clap(long)]
    timezone_file: Option<String>,

    /// Retry count for each request.
    #[clap(long, default_value_t = 3)]
    retry: usize,

    /// Do an HEAD before actual GET. Add this if you are not sure if the results from parser is correct.
    #[clap(long)]
    head_before_get: bool,

    /// Choose a parser.
    #[clap(long, value_enum, default_value_t = ParserType::Nginx)]
    parser: ParserType,

    /// Excluded file regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included file regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,

    /// Skip file regex if they exist. Supports multiple.
    #[clap(long, value_parser)]
    skip_if_exists: Vec<ExpandedRegex>,

    /// Allow mtime from parser if not available from HTTP headers.
    #[clap(long)]
    allow_mtime_from_parser: bool,
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream_folder: Url,

    /// Choose a parser.
    #[clap(long, value_enum, default_value_t=ParserType::Nginx)]
    parser: ParserType,

    /// Excluded file regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included file regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,

    /// The upstream base ending with "/".
    #[clap(long, default_value = "/")]
    upstream_base: String,
}

fn main() {
    // https://github.com/tokio-rs/tracing/issues/735#issuecomment-957884930
    std::env::set_var(
        "RUST_LOG",
        format!("info,{}", std::env::var("RUST_LOG").unwrap_or_default()),
    );
    let enable_color = std::env::var("NO_COLOR").is_err();
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(enable_color)
        .init();

    let bind_address = std::env::var("BIND_ADDRESS").ok();

    // terminate whole process when a thread panics
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(3);
    }));

    let args = Cli::parse();
    match args.command {
        Commands::Sync(args) => {
            cli::sync(args, bind_address);
        }
        Commands::List(args) => {
            // extra arg check
            if !args.upstream_folder.path().ends_with('/') {
                panic!("upstream_folder should end with /");
            }
            cli::list(args, bind_address);
        }
    };
}
