# Whim

A small, fast, cross-platform file editor for **files of any size**, in the spirit of Notepad++, with a
built-in Markdown view and a hex view.

The core promise: any file, from a 2 KB config to a 20 GB log, opens instantly and can be viewed and
edited **without the whole file ever being loaded into memory**.

> **Status: early development (pre-alpha).** Whim is currently a walking skeleton built to prove that
> the toolkit can meet the startup, memory and size targets. It opens a file from the command line and
> scrolls through it, but it cannot edit, save, search or open files from a dialog yet.
> See [Roadmap](#roadmap).

## Goals

- **Huge files are first-class.** Files are memory-mapped and only the visible part is read. Edits are
  kept in a piece table, so RAM grows with your edits, not with the file size.
- **Instant.** The window appears in about 200 ms, and the first page of a 20 GB file shows in under a second.
- **Light.** It uses tens of MB of RAM when idle and close to zero CPU. It ships as a single portable binary
  (target under 15 MB) with no installer or runtime.
- **Cross-platform.** Windows, Linux and macOS are all first-class.
- **Minimal UI.** Tabs, the editor and a status bar. Everything else lives in menus.

Planned features include tabs, syntax highlighting, find and replace (including regex) that works on huge files,
encoding and line-ending detection, a Markdown source/rendered toggle, a hex view with byte overwrite,
drag and drop, and "Open with" integration.

## Current state (Phase 1: toolkit and footprint gate)

The current build is a Rust + [egui/eframe](https://github.com/emilk/egui) skeleton that renders with
OpenGL (glow). It has:

- a virtualized monospace view that paints only the visible rows of a memory-mapped file, with a byte-offset
  scrollbar and gutter
- a menu bar and a status bar (offset, file size, renderer, startup time)
- IME composition support (tested with the Windows Japanese IME)
- crisp text at 100 to 200% display scaling

Measured on Windows 11 (Ryzen 9 5950X, Intel Arc B580) with a 20 GiB file open:

| Metric | Result | Target |
|---|---|---|
| Stripped binary | 5.0 MB | < 15 MB |
| Startup, warm median | ~266 ms | ~200 ms |
| Idle RAM, settled working set | ~58 MB | tens of MB |
| Idle CPU | ~0 % | ~0 % |

Opening the 20 GiB file adds no measurable RAM. Full numbers, method and per-OS CI results are in
[docs/measurements.md](docs/measurements.md).

## Building

Requires [rustup](https://rustup.rs). The toolchain version is pinned in `rust-toolchain.toml` and is
installed automatically.

```sh
cargo build --release
```

On Linux you need the usual X11/Wayland and OpenGL development packages for winit and glow.

## Running

```sh
# open a file (drag & drop and File > Open are coming in Phase 2)
target/release/whim path/to/file
```

## Development tools

The `xtask` crate holds the measurement harness:

```sh
cargo xtask sparse --size 20GiB target/big.sparse.bin   # create a sparse test file (takes no real disk space)
cargo xtask check-size --max-mb 10                      # stripped binary size gate
cargo xtask measure --runs 10                           # startup, idle RAM and CPU
cargo test --workspace                                  # unit tests
```

CI runs tests and the size/startup/RAM budgets on Windows, Linux and macOS for every push.

## Roadmap

1. **Toolkit & footprint gate**: prove egui/glow meets the budgets (in progress)
2. **Core storage & read-only viewers**: mmap-backed buffer, background line index, jump to line or offset,
   hex view, File > Open, drag & drop
3. **Editing, undo & hex overwrite**: piece table, selection, clipboard, bounded undo
4. **Safe save & recovery**: never-risk-the-original saves of huge files, crash recovery
5. **Encodings, tabs & file-change safety**
6. **Search, replace & tail mode**
7. **Syntax highlighting & Markdown view**
8. **OS integration, preferences & release**: CLI, single instance, file associations, session restore,
   themes, portable builds

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

The bundled JetBrains Mono font is licensed under the SIL Open Font License 1.1
([assets/fonts/OFL.txt](assets/fonts/OFL.txt)).

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in this work,
as defined in the Apache-2.0 license, is dual licensed as above, with no additional terms or conditions.
