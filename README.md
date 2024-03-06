# tsumugu

A HTTP(S) syncing tool with lower overhead, for OSS mirrors.

Instead of `HEAD`ing every single file, tsumugu parses directory listing HTML and downloads only files that do not seem to be up-to-date.

## Design goals

To successfully sync from these domains, where lftp/rclone fails or finds difficulties:

- [x] http://download.proxmox.com/
- [x] https://download.docker.com/
- [x] https://dl.winehq.org/wine-builds/

## TODOs

- [x] Add "--include": Sync even if the file is excluded by `--exclude` regex.
- [x] Add supported Debian, Ubuntu, Fedora and RHEL versions support to `--include` regex.
  - Something like `--include debian/${DEBIAN_VERSIONS}`?
- [x] Check for APT/YUM repo integrity (avoid keeping old invalid metadata files)
  - (This is experimental and may not work well)

## Usage

```console
> cargo run -- --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/tsumugu --help`
A HTTP(S) syncing tool with lower overhead, for OSS mirrors

Usage: tsumugu <COMMAND>

Commands:
  sync  Sync files from upstream to local
  list  List files from upstream
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
> cargo run -- sync --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.07s
     Running `target/debug/tsumugu sync --help`
Usage: tsumugu sync [OPTIONS] <UPSTREAM> <LOCAL>

Arguments:
  <UPSTREAM>  The upstream URL
  <LOCAL>     The local directory

Options:
      --user-agent <USER_AGENT>
          Customize tsumugu's user agent [default: tsumugu]
      --dry-run
          Do not download files and cleanup
      --threads <THREADS>
          Threads at work [default: 2]
      --no-delete
          Do not clean up after sync
      --max-delete <MAX_DELETE>
          Set max delete count [default: 100]
      --timezone-file <TIMEZONE_FILE>
          Default: auto. You can set a valid URL for guessing, or an invalid one for disabling
      --timezone <TIMEZONE>
          Manually set timezone (+- hrs). This overrides timezone_file
      --retry <RETRY>
          Retry count for each request [default: 3]
      --head-before-get
          Do an HEAD before actual GET. Add this if you are not sure if the results from parser is correct
      --parser <PARSER>
          Choose a parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd]
      --exclude <EXCLUDE>
          Excluded file regex. Supports multiple
      --include <INCLUDE>
          Included file regex (when it startswith any exclude regexes). Supports multiple
      --skip-if-exists <SKIP_IF_EXISTS>
          Skip file regex if they exist. Supports multiple
      --compare-size-only <COMPARE_SIZE_ONLY>
          File regex for those compare size only in HEAD requests. This only works with head_before_get
      --allow-mtime-from-parser
          Allow mtime from parser if not available from HTTP headers
      --apt-packages
          (Experimental) APT Packages file parser to find out missing packages
      --yum-packages
          (Experimental) YUM Packages file parser to find out missing packages
  -h, --help
          Print help
  -V, --version
          Print version
> cargo run -- list --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/tsumugu list --help`
Usage: tsumugu list [OPTIONS] <UPSTREAM>

Arguments:
  <UPSTREAM>  The upstream URL

Options:
      --user-agent <USER_AGENT>        Customize tsumugu's user agent [default: tsumugu]
      --parser <PARSER>                Choose a parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd]
      --exclude <EXCLUDE>              Excluded file regex. Supports multiple
      --include <INCLUDE>              Included file regex (even if excluded). Supports multiple
      --upstream-base <UPSTREAM_BASE>  The upstream base ending with "/" [default: /]
  -h, --help                           Print help
  -V, --version                        Print version
```

For a very brief introduction of parser, see [./src/parser/README.md](./src/parser/README.md).

## Exit code

- 0: Success
- 1: Failed to list
- 2: Failed to download
- 3: A panic!() occurred
- 4: Error when cleaning up
- 25: The limit stopped deletions

## Building with musl

Unfortunately, this requires openssl-sys, which is not included in cross's prebuilt images. Try https://github.com/clux/muslrust.

## Evaluation

Default concurrency is 2 threads.

### http://download.proxmox.com/

Proxmox uses a self-hosted CDN server architecture, and unfortunately its server limits concurrency to only 1 (as far as I could test). With traditional lftp/rclone it could take > 10 hours to sync once (even when your local files are identical with remote ones).

Note: Consider using [Proxmox Offline Mirror](https://pom.proxmox.com/) or other tools like `apt-mirror` if you only need its APT repository.

```console
> time ./tsumugu sync --threads 1 --dry-run --exclude '^temp' http://download.proxmox.com/ /srv/repo/proxmox/
...

real	1m48.746s
user	0m3.468s
sys	0m3.385s
```

### https://download.docker.com/

We use [a special script](https://github.com/ustclug/ustcmirror-images/blob/master/docker-ce/tunasync/sync.py) for syncing docker-ce before, but tsumugu can also handle this now. And also, for 30x inside linux/centos/ and linux/rhel/, tsumugu could create symlinks as what this script do before.

```console
> time ./tsumugu sync --timezone-file https://download.docker.com/linux/centos/docker-ce-staging.repo --parser docker --dry-run https://download.docker.com/ /srv/repo/docker-ce/
...

real	8m32.674s
user	0m4.532s
sys	0m2.855s
```

### https://dl.winehq.org/wine-builds/

lftp/rclone fails to handle complex HTML.

```console
> time ./tsumugu sync --parser apache-f2 --dry-run --exclude '^mageia' --exclude '^macosx' --exclude '^debian' --exclude '^ubuntu' --exclude '^fedora' --include '^debian/dists/${DEBIAN_CURRENT}' --include '^ubuntu/dists/${UBUNTU_LTS}' --include '^fedora/${FEDORA_CURRENT}' https://dl.winehq.org/wine-builds/ /srv/repo/wine/wine-builds/
...

<TIMESTAMP>  INFO ThreadId(01) tsumugu: (Estimated) Total objects: 17514, total size: 342.28 GiB

real	0m5.664s
user	0m1.475s
sys	0m0.294s
```

## Notes

### Yuki integration

See <https://github.com/ustclug/ustcmirror-images#tsumugu>.

YAML example:

```yaml
envs:
  UPSTREAM: http://download.proxmox.com/
  # tsumugu is not in yuki supported upstream image yet, so this is a workaround to correctly display the upstream URL
  $UPSTREAM: http://download.proxmox.com/
  TSUMUGU_EXCLUDE: --exclude ^temp --exclude pmg/dists/.+changelog$ --exclude devel/dists/.+changelog$
  TSUMUGU_TIMEZONEFILE: http://download.proxmox.com/images/aplinfo.dat
  TSUMUGU_THREADS: 1
image: ustcmirror/tsumugu:latest
interval: 12 3 * * *
logRotCycle: 10
name: proxmox
storageDir: /srv/repo/proxmox/
```

More examples in [examples/](./examples/).

### Regex variables

See [./src/regex_process.rs](./src/regex_process.rs).

### Exclusion and inclusion

Currently tsumugu follows a simple algorithm to determine whether a path should be completely excluded, partially excluded, or included:

0. When parsing regex, a "rev_inner" regex will be generated by replacing variables (`${UBUNTU_LTS}`, etc.) to `(?<distro_ver>.+)`.
1. First, users' exclusions and inclusions are preprocessed. For all exclusions, if it is a prefix of any inclusion, it will be put into the `list_only_regexes`, otherwise it will be put into `instant_stop_regexes`.
2. While working threads are handling listing requests, they will first check if it matches any `instant_stop_regexes`. If not so, check if the path matches any inclusion regexes. If so, the path will be completely excluded. Then, the path will be checked with "rev_inner" regex, and also completely excluded if matches (a fast shortcut). Finally, if the path matches `list_only_regexes`, files under this directory will be ignored, but subdirectories will still be listed. Paths that are not matched by any regexes will be included as usual.

In this process some paths, which would be unnecessary, will still be listed. Consider it as a trade-off for simplicity and performance.

Also note that logic like this is used when generating relative path for comparison:

```rust
// Before working on one task:
let relative = task.relative.join("/");
// Before downloading a file:
let relative_filepath = PathBuf::from(&task_context.relative).join(&item.name);
let relative_filepath = relative_filepath.to_string_lossy();
```

`item.name`, or strings inside `task.relative`, do not have trailing slash, so if you want to exclude a directory completely, you should not put a trailing slash in your regex.

You might see arguments like `--exclude debian/ --include debian/dists/${DEBIAN_CURRENT}`, with trailing slash exclusion in examples. This is just because we don't need to exclude directory listing of `debian` folder out.

## Naming

The name "tsumugu", and current branch name "pudding", are derived from the manga *A Drift Girl and a Noble Moon*.

<details>
<summary>And...</summary>
<a href="https://github.com/taoky/paintings/blob/master/tsumugu_github_comic_20230721.png"><img alt="tsumugu, drawn as simplified version of hitori" src="https://github.com/taoky/paintings/blob/master/tsumugu_github_comic_20230721.png?raw=true"></img></a>

Tsumugu in the appearance of a very simplified version of Hitori (Obviously I am not very good at drawing though).
</details>

Old (2020), unfinished golang version is named as "traverse", under the `main-old` branch.
