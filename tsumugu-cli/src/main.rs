#![warn(clippy::cognitive_complexity)]
use std::{ops::Deref, path::PathBuf, sync::Mutex};

use clap::{Parser, Subcommand};

use tracing::{level_filters::LevelFilter, trace};
use tracing_subscriber::EnvFilter;
use tsumugu_parser::{
    client::{HttpClient, HttpResponse, RequestType},
    parser::{ParserType, ParserTypeMatch},
    regex_manager::ExpandedRegex,
};
use url::Url;

use shadow_rs::shadow;
use utils::{headers_to_headermap, Header};
shadow!(build);

mod bar;
mod cli;
mod compare;
mod utils;

struct TokioHttpClient {
    runtime: tokio::runtime::Runtime,
    listing_client: reqwest::Client,
    download_client: reqwest::Client,
}

fn tokio_resp_to_tsumugu_resp(resp: &reqwest::Response) -> anyhow::Result<HttpResponse> {
    let content_length = resp.content_length();
    let status_code = resp.status().as_u16();
    let final_url = resp.url().clone();
    let modified_time = utils::get_response_mtime(&resp);
    let headers = resp.headers().clone();
    Ok(HttpResponse {
        body: String::new(),
        final_url,
        status_code,
        content_length,
        modified_time,
        headers,
    })
}

impl TokioHttpClient {
    fn select_client(&self, req_type: RequestType) -> &reqwest::Client {
        match req_type {
            RequestType::List => &self.listing_client,
            RequestType::Download => &self.download_client,
        }
    }
}

impl HttpClient for TokioHttpClient {
    fn get_text_with_type(&self, url: &Url, req_type: RequestType) -> anyhow::Result<HttpResponse> {
        let future =
            async { crate::utils::get_async(self.select_client(req_type), url.clone()).await };
        let resp = self.runtime.block_on(future)?;
        let http_resp = tokio_resp_to_tsumugu_resp(&resp)?;
        let body_text = self.runtime.block_on(resp.text())?;
        Ok(HttpResponse {
            body: body_text,
            ..http_resp
        })
    }
    fn head_with_type(&self, url: &Url, req_type: RequestType) -> anyhow::Result<HttpResponse> {
        let future =
            async { crate::utils::head_async(self.select_client(req_type), url.clone()).await };
        let resp = self.runtime.block_on(future)?;
        tokio_resp_to_tsumugu_resp(&resp)
    }
}

#[allow(clippy::const_is_empty)]
fn get_version() -> &'static str {
    let tag = build::TAG;
    let clean = build::GIT_CLEAN;
    let short_commit = build::SHORT_COMMIT;
    if !clean {
        Box::leak(format!("{} (dirty)", build::SHORT_COMMIT).into_boxed_str())
    } else if tag.is_empty() {
        if short_commit.is_empty() {
            build::PKG_VERSION
        } else {
            short_commit
        }
    } else {
        tag
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

impl Deref for SyncArgs {
    type Target = CommonArgs;
    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

impl Deref for ListArgs {
    type Target = CommonArgs;
    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

#[derive(Parser, Debug)]
pub struct CommonArgs {
    /// Choose a main parser.
    #[clap(long, value_enum, default_value_t = ParserType::Nginx)]
    parser: ParserType,

    /// The upstream URL.
    #[clap(value_parser)]
    upstream: Url,

    /// Customize tsumugu's user agent.
    #[clap(long, default_value = "tsumugu")]
    user_agent: String,

    /// Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple.
    #[clap(long, value_parser)]
    header: Vec<Header>,

    /// The exclusion v2 mode. To keep compatibility, this is off by default.
    #[clap(long)]
    exclusion_v2: bool,

    /// Excluded relative path regex. Supports multiple.
    #[clap(long, value_parser)]
    exclude: Vec<ExpandedRegex>,

    /// Included relative path regex (even if excluded). Supports multiple.
    #[clap(long, value_parser)]
    include: Vec<ExpandedRegex>,

    /// Choose supplementary parsers. Format: "parsername:matchpattern".
    /// matchpattern is a relative path regex.
    /// Supports multiple.
    #[clap(long, value_parser)]
    parser_match: Vec<ParserTypeMatch>,

    /// Allow automatically choose fallback parser when ParseError occurred.
    #[clap(long)]
    auto_fallback: bool,
}

impl CommonArgs {
    pub fn headers(&self) -> reqwest::header::HeaderMap {
        headers_to_headermap(&self.header)
    }
}

#[derive(Parser, Debug)]
pub struct SyncArgs {
    #[clap(flatten)]
    common: CommonArgs,

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
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    #[clap(flatten)]
    common: CommonArgs,

    /// The upstream base starting with "/".
    #[clap(long, default_value = "/")]
    upstream_base: String,
}

fn main() {
    let enable_color = std::env::var("NO_COLOR").is_err();
    let pb_manager = kyuri::Manager::new(std::time::Duration::from_secs(1));
    pb_manager.set_ticker(true);
    let pb_writer = pb_manager.create_writer();
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_env_filter(
            // https://github.com/tokio-rs/tracing/issues/735
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
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
