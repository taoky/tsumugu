# tsumugu

\[WIP\] A HTTP(S) syncing tool with lower overhead, for OSS mirrors.

Instead of `HEAD`ing every single file, tsumugu parses directory listing HTML and `HEAD`s only the files that do not seem to be up-to-date.

## Design goals

To successfully sync from these domains, where lftp/rclone fails or finds difficulties:

- [ ] http://download.proxmox.com
- [ ] https://download.docker.com/
- [ ] https://dl.winehq.org/wine-builds/

## Usage

todo!()

## Evaluation

todo!()

## Naming

The name "tsumugu", and current branch name "pudding", are derived from the manga *A Drift Girl and a Noble Moon*.

Old (2020), unfinished golang version is named as "traverse", under the `main-old` branch.
