# Parsers of tsumugu

This is a list of parsers that tsumugu supports:

- apache_f2: [Apache2's autoindex](https://httpd.apache.org/docs/2.4/mod/mod_autoindex.html) with HTMLTable FancyIndexed list (`F=2`).
- directory_lister: [Directory Lister](https://www.directorylister.com/).
- docker: A specialized parser for <https://download.docker.com/>.
- lighttpd: [lighttpd's mod_dirlisting](https://redmine.lighttpd.net/projects/lighttpd/wiki/Docs_ModDirlisting).
- nginx: [Nginx's autoindex](https://nginx.org/en/docs/http/ngx_http_autoindex_module.html).
- caddy: [Caddy's file_server](https://caddyserver.com/docs/caddyfile/directives/file_server).

## Debugging

You could use `tsumugu list` to help you debug the parser (and behavior of exclusion/inclusion).

Example:

```console
$ cargo run -- list --parser lighttpd --exclude edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma --upstream-base / https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/tsumugu list --parser lighttpd --exclude edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma --upstream-base / 'https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/'`
Relative: edk2/git/MdeModulePkg/Universal/RegularExpressionDxe
Exclusion: Ok
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma/ Directory (none) 2023-09-07 20:21:46 oniguruma (stop)
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/OnigurumaUefiPort.c File 2.9 K 2023-09-07 20:21:19 OnigurumaUefiPort.c
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/OnigurumaUefiPort.h File 3 K 2023-09-07 20:21:19 OnigurumaUefiPort.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/RegularExpressionDxe.c File 13.9 K 2023-09-07 20:21:19 RegularExpressionDxe.c
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/RegularExpressionDxe.h File 5.8 K 2023-09-07 20:21:19 RegularExpressionDxe.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/RegularExpressionDxe.inf File 3.5 K 2023-09-07 20:21:19 RegularExpressionDxe.inf
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/config.h File 0.2 K 2023-09-07 20:21:19 config.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/stdarg.h File 0.2 K 2023-09-07 20:21:19 stdarg.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/stddef.h File 0.2 K 2023-09-07 20:21:19 stddef.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/stdio.h File 0.2 K 2023-09-07 20:21:19 stdio.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/stdlib.h File 0.2 K 2023-09-07 20:21:19 stdlib.h
https://sources.buildroot.net/edk2/git/MdeModulePkg/Universal/RegularExpressionDxe/string.h File 0.2 K 2023-09-07 20:21:19 string.h
```
