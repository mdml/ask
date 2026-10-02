# Libc direct-dependency review

Review date: 2026-10-02

Status: evidence record for the direct `libc` dependency added by the terminal raw-mode fix. The owner approved `libc = "=0.2.189"` as a direct dependency, pinned to the version already in `Cargo.lock`, with no other new dependency and no feature changes, before implementation.

## Purpose

`console` 0.16.6 puts the terminal in raw mode for each key it reads and restores the saved attributes afterwards. Between reads, while a menu redraws or the hidden key prompt loops, the terminal was back in canonical echo mode, so a key arriving in that window was echoed and line-edited by the terminal driver: a character of a pasted API key could appear on screen, and a Backspace typed quickly in the model filter could be lost. `src/terminal/attributes.rs` now saves the terminal attributes of standard input when a menu or the hidden prompt starts, holds the same raw mode `console` uses (output flags unchanged) until it returns, and restores the saved attributes on every exit path. After Ctrl-C it restores them before raising `SIGINT`. Those calls (`tcgetattr`, `tcsetattr`, `cfmakeraw`, and `raise`) need `libc`. The module is the crate's only `unsafe` code; `Cargo.toml` denies `unsafe_code` everywhere else.

## Observed facts

The implementation worker made the following observations locally and offline on 2026-10-02 on Linux with the pinned Rust 1.97.1 toolchain.

- `Cargo.lock` already contained `libc` 0.2.189 from the crates.io registry, with checksum `3eaf3ede3fee6db1a4c2ee091bf8a8b4dccdc6d17f656fb07896ee72867612f2`, as a transitive dependency of `console`, `tokio`, `mio`, `getrandom`, and other locked packages since commit `69f1c7e` (2026-09-04). The only lockfile change is the added `libc` edge in `ask`'s own dependency list; no package was added, removed, or changed version.
- The SHA-256 of the locally cached `libc-0.2.189.crate` equals the lockfile checksum.
- `cargo tree --offline --locked --target all -e features --prefix none --format '{p} {f}'`, run on the base revision and on the change with the repository path removed and the output sorted and deduplicated, is identical. The direct dependency enables `libc`'s default `std` feature, which was already enabled; no package's resolved features changed. `cargo tree --offline --locked --prefix none --format '{p}'` likewise lists the same packages before and after.
- `cargo metadata` reports license `MIT OR Apache-2.0`, a `lib` target, and a build script (already compiled for every build before this change); no procedural macro.
- `cargo deny --locked check` passed after the change.

## Quarantine

The version was not updated, so no new publication enters the closure. The managing agent confirms the version's publication time against crates.io before merging, as for any dependency change.

## Limits

- Attributes are held on standard input, the terminal `console` reads keys from when standard input is a terminal, which is the only case in which `ask` shows a menu or the hidden prompt.
- A process stopped by `SIGTSTP` while a menu waits may be resumed by a shell that restored canonical mode; the first key after resumption is then line-buffered until the next read reapplies raw mode. On every exit path the saved attributes are restored.
- A `SIGINT`, `SIGTERM`, or `SIGHUP` delivered by another process rather than typed as Ctrl-C uses the default disposition and does not restore the attributes; keyboard Ctrl-C arrives as a byte in raw mode and does.
