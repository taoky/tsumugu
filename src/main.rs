#![warn(clippy::cognitive_complexity)]
use std::{path::PathBuf, sync::Mutex};

use clap::{Parser, Subcommand};

use parser::{ParserType, ParserTypeMatch};
use tracing_subscriber::EnvFilter;
use url::Url;

use shadow_rs::shadow;
use utils::{headers_to_headermap, Header};
shadow!(build);

mod bar;
mod cli;
mod compare;
mod listing;
mod parser;
mod regex_manager;
mod timezone;
mod utils;

mod extensions;

use crate::regex_manager::ExpandedRegex;

#[allow(clippy::const_is_empty)]
fn get_version() -> &'static str {
    let tag = build::TAG;
    let clean = build::GIT_CLEAN;
    let short_commit = build::SHORT_COMMIT;
    if !clean {
        Box::leak(format!("{} (dirty)", build::SHORT_COMMIT).into_boxed_str())
    } else if tag.is_empty() {
        if short_commit.is_empty() {
            return build::PKG_VERSION;
        } else {
            return short_commit;
        }
    } else {
        return tag;
    }
}

#[derive(Parser, Debug)]
#[command(about)]
#[command(propagate_version = true)]
#[command(version = get_version())]
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

trait SharedArgs {
    fn user_agent(&self) -> &str;
    fn headers(&self) -> reqwest::header::HeaderMap;
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

    /// You can set a valid URL for guessing. Set it to "no" to disable this behavior.
    /// By default it would recursively find the first file to HEAD for guessing
    #[clap(long)]
    timezone_file: Option<String>,

    /// Manually set timezone (+- hrs). This overrides timezone_file.
    #[clap(long)]
    timezone: Option<i32>,

    /// Retry count for each request.
    #[clap(long, default_value_t = 3)]
    retry: usize,

    /// Do an HEAD before actual GET.
    /// Otherwise when head-before-get and allow-time-from-parser are not set,
    /// when GETting tsumugu would try checking if we still need to download it.
    #[clap(long)]
    head_before_get: bool,

    /// Choose a main parser.
    #[clap(long, value_enum, default_value_t = ParserType::Nginx)]
    parser: ParserType,

    /// Choose supplementary parsers. Format: "parsername:matchpattern".
    /// matchpattern is a relative path regex.
    /// Supports multiple.
    #[clap(long, value_parser)]
    parser_match: Vec<ParserTypeMatch>,

    /// Excluded relative path regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included relative path regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,

    /// Skip relative path regex if they exist. Supports multiple.
    #[clap(long, value_parser)]
    skip_if_exists: Vec<ExpandedRegex>,

    /// Relative path regex for those compare size only **after** HEAD (head_before_get on) or GET (head_before_get off)
    #[clap(long, value_parser)]
    compare_size_only: Vec<ExpandedRegex>,

    /// Allow mtime from parser if not available from HTTP headers.
    #[clap(long, visible_alias = "allow-mtime-from-parser")]
    trust_mtime_from_parser: bool,

    /// (Experimental) APT Packages file parser to find out missing packages.
    #[clap(long)]
    apt_packages: bool,

    /// (Experimental) YUM Packages file parser to find out missing packages.
    #[clap(long)]
    yum_packages: bool,

    /// Ignore 404 NOT FOUND as error when downloading files.
    #[clap(long)]
    ignore_nonexist: bool,

    /// Allow automatically choose fallback parser when ParseError occurred.
    #[clap(long)]
    auto_fallback: bool,

    /// Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple.
    #[clap(long, value_parser)]
    header: Vec<Header>,

    /// The exclusion v2 mode. To keep compatibility, this is off by default.
    #[clap(long)]
    exclusion_v2: bool,
}

impl SharedArgs for &SyncArgs {
    fn user_agent(&self) -> &str {
        &self.user_agent
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers_to_headermap(&self.header)
    }
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream: Url,

    /// Choose a main parser.
    #[clap(long, value_enum, default_value_t=ParserType::Nginx)]
    parser: ParserType,

    /// Excluded relative path regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included relative path regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,

    /// The upstream base starting with "/".
    #[clap(long, default_value = "/")]
    upstream_base: String,

    /// Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple.
    #[clap(long, value_parser)]
    header: Vec<Header>,
}

impl SharedArgs for &ListArgs {
    fn user_agent(&self) -> &str {
        &self.user_agent
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers_to_headermap(&self.header)
    }
}

pub struct AsyncContext {
    pub listing_client: reqwest::Client,
    pub download_client: reqwest::Client,
    pub runtime: tokio::runtime::Runtime,
}

fn main() {
    // https://github.com/tokio-rs/tracing/issues/735#issuecomment-957884930
    std::env::set_var(
        "RUST_LOG",
        format!("info,{}", std::env::var("RUST_LOG").unwrap_or_default()),
    );
    let enable_color = std::env::var("NO_COLOR").is_err();
    let pb_manager = kyuri::Manager::new(std::time::Duration::from_secs(1));
    pb_manager.set_ticker(true);
    let pb_writer = pb_manager.create_writer();
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(enable_color)
        .with_writer(Mutex::new(pb_writer))
        .init();

    // Print version info in debug mode
    tracing::debug!("{}", build::CLAP_LONG_VERSION);

    let bind_address = match std::env::var("BIND_ADDRESS").ok() {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_owned())
            }
        }
        None => None,
    };

    // terminate whole process when a thread panics
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(3);
    }));

    let args = Cli::parse();
    match args.command {
        Commands::Sync(args) => {
            if !args.upstream.path().ends_with('/') {
                tracing::warn!("It's suggested to append backslash to upstream, though this also works in most cases (most web servers redirects this to URL with backslash at end).")
            }
            cli::sync(&args, bind_address, pb_manager);
        }
        Commands::List(args) => {
            // extra arg check
            if !args.upstream.path().ends_with('/') {
                panic!("upstream should end with /");
            }
            if !args.upstream_base.starts_with('/') {
                panic!("upstream_base does not start with /")
            }
            cli::list(&args, bind_address);
        }
    };
}
