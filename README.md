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

## Project structure

This project uses cargo workspace with the following crates:

- [tsumugu-parser](./tsumugu-parser/): A parser crate for various directory listing formats. Can be reused by other projects.
- [tsumugu-cli](./tsumugu-cli/): The CLI tool for syncing. For historical reasons, the crate name (and binary name) is called `tsumugu`.

## Common notes

### Regex variables

See [./tsumugu-parser/src/regex_manager/mod.rs](./tsumugu-parser/src/regex_manager/mod.rs) for available variables to use in inclusion and exclusion regexes.

### Exclusion and inclusion rules

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
