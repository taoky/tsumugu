# tsumugu

\[WIP\] A HTTP(S) syncing tool with lower overhead, for OSS mirrors.

Instead of `HEAD`ing every single file, tsumugu parses directory listing HTML and downloads only files that do not seem to be up-to-date.

## Design goals

To successfully sync from these domains, where lftp/rclone fails or finds difficulties:

- [ ] http://download.proxmox.com/
- [ ] https://download.docker.com/
- [ ] https://dl.winehq.org/wine-builds/

## Usage

```console
> cargo run -- --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/tsumugu --help`
A HTTP(S) syncing tool with lower overhead, for OSS mirrors

Usage: tsumugu <COMMAND>

Commands:
  sync  
  list  
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
> cargo run -- sync --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.07s
     Running `target/debug/tsumugu sync --help`
Usage: tsumugu sync [OPTIONS] <UPSTREAM> <LOCAL>

Arguments:
  <UPSTREAM>  
  <LOCAL>     

Options:
      --user-agent <USER_AGENT>        [default: tsumugu]
      --dry-run                        Do not download files and cleanup
      --threads <THREADS>              [default: 2]
      --no-delete                      
      --max-delete <MAX_DELETE>        [default: 100]
      --timezone-file <TIMEZONE_FILE>  Default: auto. You can set a valid URL for guessing, or an invalid one for disabling
      --retry <RETRY>                  [default: 3]
      --head-before-get                
      --parser <PARSER>                [default: nginx] [possible values: nginx]
      --exclude <EXCLUDE>              
  -h, --help                           Print help
  -V, --version                        Print version
> cargo run -- list --help
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/tsumugu list --help`
Usage: tsumugu list [OPTIONS] <UPSTREAM>

Arguments:
  <UPSTREAM>  

Options:
      --user-agent <USER_AGENT>  [default: tsumugu]
      --parser <PARSER>          [default: nginx] [possible values: nginx]
  -h, --help                     Print help
  -V, --version                  Print version
```

## Building with musl

Unfortunately, this requires openssl-sys, which is not included in cross's prebuilt images. Try https://github.com/clux/muslrust.

## Evaluation

### http://download.proxmox.com/

Proxmox uses a self-hosted CDN server architecture, and unfortunately its server limits concurrency to only 1 (as far as I could test). With traditional lftp/rclone it could take > 10 hours to sync once (even when your local files are identical with remote ones).

Note: Consider using [Proxmox Offline Mirror](https://pom.proxmox.com/) or other tools like `apt-mirror` if you only need its APT repository.

```
> time ./tsumugu sync --threads 1 --dry-run --exclude '^temp' http://download.proxmox.com/ /srv/repo/proxmox/
...

real	1m48.746s
user	0m3.468s
sys	0m3.385s
```

todo!()

## Naming

The name "tsumugu", and current branch name "pudding", are derived from the manga *A Drift Girl and a Noble Moon*.

Old (2020), unfinished golang version is named as "traverse", under the `main-old` branch.
