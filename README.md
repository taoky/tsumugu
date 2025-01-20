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
> ./tsumugu --help
A HTTP(S) syncing tool with lower overhead, for OSS mirrors

Usage: tsumugu <COMMAND>

Commands:
  sync  Sync files from upstream to local
  list  List files from upstream
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
> ./tsumugu sync --help
Sync files from upstream to local

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
          You can set a valid URL for guessing. Set it to "no" to disable this behavior. By default it would recursively find the first file to HEAD for guessing
      --timezone <TIMEZONE>
          Manually set timezone (+- hrs). This overrides timezone_file
      --retry <RETRY>
          Retry count for each request [default: 3]
      --head-before-get
          Do an HEAD before actual GET. Otherwise when head-before-get and allow-time-from-parser are not set, when GETting tsumugu would try checking if we still need to download it
      --parser <PARSER>
          Choose a main parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd, caddy, fancy-index, gradle, fallback]
      --parser-match <PARSER_MATCH>
          Choose supplementary parsers. Format: "parsername:matchpattern". matchpattern is a relative path regex. Supports multiple
      --exclude <EXCLUDE>
          Excluded relative path regex. Supports multiple
      --include <INCLUDE>
          Included relative path regex (even if excluded). Supports multiple
      --skip-if-exists <SKIP_IF_EXISTS>
          Skip relative path regex if they exist. Supports multiple
      --compare-size-only <COMPARE_SIZE_ONLY>
          Relative path regex for those compare size only **after** HEAD (head_before_get on) or GET (head_before_get off)
      --trust-mtime-from-parser
          Allow mtime from parser if not available from HTTP headers [aliases: allow-mtime-from-parser]
      --apt-packages
          (Experimental) APT Packages file parser to find out missing packages
      --yum-packages
          (Experimental) YUM Packages file parser to find out missing packages
      --ignore-nonexist
          Ignore 404 NOT FOUND as error when downloading files
      --auto-fallback
          Allow automatically choose fallback parser when ParseError occurred
      --header <HEADER>
          Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple
      --exclusion-v2
          The exclusion v2 mode. To keep compatibility, this is off by default
  -h, --help
          Print help
  -V, --version
          Print version
> ./tsumugu list --help
List files from upstream

Usage: tsumugu list [OPTIONS] <UPSTREAM>

Arguments:
  <UPSTREAM>  The upstream URL

Options:
      --user-agent <USER_AGENT>        Customize tsumugu's user agent [default: tsumugu]
      --parser <PARSER>                Choose a main parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd, caddy, fancy-index, gradle, fallback]
      --exclude <EXCLUDE>              Excluded relative path regex. Supports multiple
      --include <INCLUDE>              Included relative path regex (even if excluded). Supports multiple
      --upstream-base <UPSTREAM_BASE>  The upstream base starting with "/" [default: /]
      --header <HEADER>                Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple
      --exclusion-v2                   The exclusion v2 mode. To keep compatibility, this is off by default
  -h, --help                           Print help
  -V, --version                        Print version
```

For a very brief introduction of parser, see [./docs/parser.md](./docs/parser.md).

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

(Note: Please see [examples](./examples/) for latest commands to sync.)

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

See [./src/regex_manager/mod.rs](./src/regex_manager/mod.rs).

### Exclusion and inclusion

**There's a breaking change since 20240902. User regexes with `^` and `$` would be affected.**

See [./docs/exclusion.md](./docs/exclusion.md).

### Deduplication

Tsumugu relies on local file size and mtime to check if file shall be downloaded. Some file-level deduplicators like [jdupes](https://codeberg.org/jbruchon/jdupes) would ignore file mtime when deduplicating with hard links. This could be an issue for some repos, as some files would be redownloaded again and again every time as it does not have a correct mtime locally.

Workarounds:

- Set `--compare-size-only`.
- Use filesystem-level/block-level deduplication like `zfs dedup`.
- Use another file-level deduplicator which considers mtime (though I don't know which would do this).

Also, if you are sure that some directory is identical with another, you could manually create a symlink for that. Tsumugu would ignore symlinks during syncing.

## Acknowledgements

Special thanks to [NJU Mirror](https://mirrors.nju.edu.cn/) for extensive testing and bug reporting.

## Naming

The name "tsumugu", and current branch name "pudding", are derived from the manga *A Drift Girl and a Noble Moon*.

<details>
<summary>And...</summary>
<a href="https://github.com/taoky/paintings/blob/master/tsumugu_github_comic_20230721.png"><img alt="tsumugu, drawn as simplified version of hitori" src="https://github.com/taoky/paintings/blob/master/tsumugu_github_comic_20230721.png?raw=true"></img></a>

Tsumugu in the appearance of a very simplified version of Hitori (Obviously I am not very good at drawing though).
</details>

Old (2020), unfinished golang version is named as "traverse", under the `main-old` branch.
