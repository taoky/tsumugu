# Parsers of tsumugu

This is a list of parsers that tsumugu supports:

- apache_f2: [Apache2's autoindex](https://httpd.apache.org/docs/2.4/mod/mod_autoindex.html) with HTMLTable FancyIndexed list (`F=2`).
- directory_lister: [Directory Lister](https://www.directorylister.com/).
- docker: A specialized parser for <https://download.docker.com/>.
- lighttpd: [lighttpd's mod_dirlisting](https://redmine.lighttpd.net/projects/lighttpd/wiki/Docs_ModDirlisting).
- nginx: [Nginx's autoindex](https://nginx.org/en/docs/http/ngx_http_autoindex_module.html). It should also work with Apache2's autoindex `F=1` mode.
- caddy: [Caddy's file_server](https://caddyserver.com/docs/caddyfile/directives/file_server).
- fancyindex: [Nginx fancyindex](https://github.com/aperezdc/ngx-fancyindex).
- gradle: A specialized parser for <https://services.gradle.org/distributions/>, might suitable for other websites like this:

    ```html
    <li>
    <a href="/distributions/gradle-8.10-wrapper.jar.sha256"><img src="/images/file.gif">
    <span class="name">gradle-8.10-wrapper.jar.sha256</span>
    <span class="date">14-Aug-2024 11:18 +0000</span>
    <span class="size">64.00B</span>
    </a>
    </li>
    ```

- denoflare-r2: Specialized parser for <https://github.com/skymethod/denoflare/blob/2e89fb33972a924dd9c5078bb2b2834a1f619081/examples/r2-public-read-worker/listing.ts>.
- fallback: An inefficient fallback parser for `index.htm(l)` which is NOT a file listing:

    ```rust
    // An inefficient fallback parser only for non-listing HTML.
    // Limitations:
    // 1. It requires /index.html or /index.htm available.
    // Parser cannot write to disk, so index file would be accessed twice during sync.
    // 2. Currently it ignores files in directories.
    // For example, it recognizes "static/css.css" as contains a "static" directory only.
    // If "static/" is inaccessible, "static/css.css" would NOT be synced.
    // In future it might be implemented when we have another parser returning a full file tree.
    // 3. It would always try HEAD to confirm existence and get file mtime & size. Items with 403/404 code would be ignored.
    // 4. It does not try parse other html files.
    // 5. It only looks for <a>. <img>, <script> and other tags are ignored.

    // Remember that tsumugu is NOT a nice tools when upstream does NOT show its file with size & mtime in HTML.
    // This parser shall be used only as a supplementary parser.
    ```

You could also check every parser's testing code and corresponding HTML files in `fixtures/` to get ideas of what every parser could parse.

## Debugging

You could use `tsumugu list` to help you debug the parser (and behavior of [exclusion/inclusion](./exclusion.md)).

For `--upstream-base`, if your upstream is like `https://some.example.com/`, it would be just `/` (default value). Otherwise if upstream is `https://some.example.com/somedir/`, then it would be `/somedir/` (or `/somedir`). `upstream_base` is used to show if an item would be included/excluded if `--exclude` or `--include` is set.

Example 1:

```console
$ ./tsumugu list --parser lighttpd --exclude /edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma/ --upstream-base / https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/
Relative: /edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/
Exclusion: Ok
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma/ Directory (none) 2023-09-07 20:21:46 oniguruma (stop)
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/OnigurumaUefiPort.c File 2.9 K 2023-09-07 20:21:19 OnigurumaUefiPort.c
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/OnigurumaUefiPort.h File 3 K 2023-09-07 20:21:19 OnigurumaUefiPort.h
```

Example 2:

```console
$ ./tsumugu list --parser apache-f2 --exclude ^fedora --upstream-base /wine-builds/ https://dl.winehq.org/wine-builds/fedora/
Relative: /fedora/
Exclusion: Stop
2024-09-01T18:47:29.966453Z  WARN ThreadId(01) tsumugu::cli::list: This listing would NOT be accessed at all.
https://dl.winehq.org/wine-builds/fedora/24/ Directory (none) 2018-12-16 15:16:00 24 (stop)
https://dl.winehq.org/wine-builds/fedora/25/ Directory (none) 2018-12-16 15:18:00 25 (stop)
https://dl.winehq.org/wine-builds/fedora/26/ Directory (none) 2018-12-16 15:19:00 26 (stop)
https://dl.winehq.org/wine-builds/fedora/27/ Directory (none) 2019-01-23 04:46:00 27 (stop)
https://dl.winehq.org/wine-builds/fedora/28/ Directory (none) 2019-05-16 03:14:00 28 (stop)
https://dl.winehq.org/wine-builds/fedora/29/ Directory (none) 2019-11-30 14:44:00 29 (stop)
https://dl.winehq.org/wine-builds/fedora/30/ Directory (none) 2020-05-09 12:33:00 30 (stop)
https://dl.winehq.org/wine-builds/fedora/31/ Directory (none) 2020-11-11 03:11:00 31 (stop)
https://dl.winehq.org/wine-builds/fedora/32/ Directory (none) 2021-05-08 10:55:00 32 (stop)
https://dl.winehq.org/wine-builds/fedora/33/ Directory (none) 2021-10-27 14:25:00 33 (stop)
https://dl.winehq.org/wine-builds/fedora/34/ Directory (none) 2022-05-21 16:41:00 34 (stop)
https://dl.winehq.org/wine-builds/fedora/35/ Directory (none) 2022-11-14 04:31:00 35 (stop)
https://dl.winehq.org/wine-builds/fedora/36/ Directory (none) 2023-04-30 09:55:00 36 (stop)
https://dl.winehq.org/wine-builds/fedora/37/ Directory (none) 2023-11-25 10:34:00 37 (stop)
https://dl.winehq.org/wine-builds/fedora/38/ Directory (none) 2024-05-04 12:55:00 38 (stop)
https://dl.winehq.org/wine-builds/fedora/39/ Directory (none) 2024-07-29 03:48:00 39 (stop)
https://dl.winehq.org/wine-builds/fedora/40/ Directory (none) 2024-07-29 03:49:00 40 (stop)
$ ./tsumugu list --parser apache-f2 --exclude ^fedora --include '^fedora/${FEDORA_CURRENT}' --upstream-base /wine-builds/ https://dl.winehq.org/wine-builds/fedora/
Relative: /fedora/
Exclusion: ListOnly
https://dl.winehq.org/wine-builds/fedora/24/ Directory (none) 2018-12-16 15:16:00 24 (stop)
https://dl.winehq.org/wine-builds/fedora/25/ Directory (none) 2018-12-16 15:18:00 25 (stop)
https://dl.winehq.org/wine-builds/fedora/26/ Directory (none) 2018-12-16 15:19:00 26 (stop)
https://dl.winehq.org/wine-builds/fedora/27/ Directory (none) 2019-01-23 04:46:00 27 (stop)
https://dl.winehq.org/wine-builds/fedora/28/ Directory (none) 2019-05-16 03:14:00 28 (stop)
https://dl.winehq.org/wine-builds/fedora/29/ Directory (none) 2019-11-30 14:44:00 29 (stop)
https://dl.winehq.org/wine-builds/fedora/30/ Directory (none) 2020-05-09 12:33:00 30 (stop)
https://dl.winehq.org/wine-builds/fedora/31/ Directory (none) 2020-11-11 03:11:00 31 (stop)
https://dl.winehq.org/wine-builds/fedora/32/ Directory (none) 2021-05-08 10:55:00 32 (stop)
https://dl.winehq.org/wine-builds/fedora/33/ Directory (none) 2021-10-27 14:25:00 33 (stop)
https://dl.winehq.org/wine-builds/fedora/34/ Directory (none) 2022-05-21 16:41:00 34 (stop)
https://dl.winehq.org/wine-builds/fedora/35/ Directory (none) 2022-11-14 04:31:00 35 (stop)
https://dl.winehq.org/wine-builds/fedora/36/ Directory (none) 2023-04-30 09:55:00 36 (stop)
https://dl.winehq.org/wine-builds/fedora/37/ Directory (none) 2023-11-25 10:34:00 37 (stop)
https://dl.winehq.org/wine-builds/fedora/38/ Directory (none) 2024-05-04 12:55:00 38 (stop)
https://dl.winehq.org/wine-builds/fedora/39/ Directory (none) 2024-07-29 03:48:00 39
https://dl.winehq.org/wine-builds/fedora/40/ Directory (none) 2024-07-29 03:49:00 40
```
