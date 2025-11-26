# tsumugu-cli

This is the CLI application for tsumugu. Please refer to [project README](../README.md) for more details.

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
      --parser <PARSER>
          Choose a main parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd, caddy, fancy-index, gradle, denoflare-r2, s3-indexbuilder, fallback]
      --user-agent <USER_AGENT>
          Customize tsumugu's user agent [default: tsumugu]
      --header <HEADER>
          Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple
      --exclusion-v2
          The exclusion v2 mode. To keep compatibility, this is off by default
      --exclude <EXCLUDE>
          Excluded relative path regex. Supports multiple
      --include <INCLUDE>
          Included relative path regex (even if excluded). Supports multiple
      --parser-match <PARSER_MATCH>
          Choose supplementary parsers. Format: "parsername:matchpattern". matchpattern is a relative path regex. Supports multiple
      --auto-fallback
          Allow automatically choose fallback parser when ParseError occurred
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
      --skip-if-exists <SKIP_IF_EXISTS>
          Skip relative path regex if they exist. Supports multiple
      --compare-size-only <COMPARE_SIZE_ONLY>
          Relative path regex for those compare size only **after** HEAD (head_before_get on) or GET (head_before_get off)
      --trust-mtime-from-parser
          Allow mtime from parser if not available from HTTP headers [aliases: --allow-mtime-from-parser]
      --apt-packages
          (Experimental) APT Packages file parser to find out missing packages
      --yum-packages
          (Experimental) YUM Packages file parser to find out missing packages
      --ignore-nonexist
          Ignore 404 NOT FOUND as error when downloading files
      --ignore-forbidden
          Ignore 403 FORBIDDEN as error when downloading files. It's recommended to use this with --no-delete if the upstream returns 403 non-deterministically or randomly
      --ignore-status <IGNORE_STATUS>
          Ignore given 4xx or 5xx status code as error when downloading files. Supports multiple
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
      --parser <PARSER>                Choose a main parser [default: nginx] [possible values: nginx, apache-f2, docker, directory-lister, lighttpd, caddy, fancy-index, gradle, denoflare-r2, s3-indexbuilder, fallback]
      --user-agent <USER_AGENT>        Customize tsumugu's user agent [default: tsumugu]
      --header <HEADER>                Custom header for HTTP(S) requests in format "Headerkey: headervalue". Supports multiple
      --exclusion-v2                   The exclusion v2 mode. To keep compatibility, this is off by default
      --exclude <EXCLUDE>              Excluded relative path regex. Supports multiple
      --include <INCLUDE>              Included relative path regex (even if excluded). Supports multiple
      --parser-match <PARSER_MATCH>    Choose supplementary parsers. Format: "parsername:matchpattern". matchpattern is a relative path regex. Supports multiple
      --auto-fallback                  Allow automatically choose fallback parser when ParseError occurred
      --upstream-base <UPSTREAM_BASE>  The upstream base starting with "/" [default: /]
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

The CI uses https://github.com/clux/muslrust to build statically linked binaries with musl libc.

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
