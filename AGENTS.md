# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

`ved` is a command-line search-and-replace tool written in Rust. It searches
for one or more patterns in a file (or recursively across a directory /
glob of files) and replaces them, streaming through files without loading
them fully into memory so it can handle very large files efficiently.

Per `README.md`:
- Handles very large files without excessive memory use — implemented
- Works on file hierarchies — implemented
- Regex matching — not yet implemented
- Block/column-aligned matching — not yet implemented

## Toolchain

- Language: Rust, `edition = "2021"`.
- Required toolchain: **nightly** (see `rust-toolchain.toml`). The code uses
  the unstable `#![feature(test)]` attribute in `src/main.rs`, so a nightly
  compiler is mandatory — stable will fail to build.
- Build/test with `cargo`, e.g. `cargo build`, `cargo test`, `cargo run -- --search <s> --replace <r> --path <p>`.
- `cargo` was not found preinstalled in this container at the time of writing;
  if it's missing, install Rust via `rustup` (and ensure the nightly channel
  from `rust-toolchain.toml` is available) before attempting to build/test.

## Code Layout

```
src/
  main.rs                CLI entry point (clap-based Args), wires args to replacer::replace_glob
  replacer/
    mod.rs               Core replace_glob / replace_path / replace_stream logic, threaded per-file
    bufsearcher.rs        Streaming buffered search engine (SEARCH_MAX / COLUMN_MAX limits)
    diff.rs               Diff type representing a single match/replacement
    diffheap.rs            Heap/ordering structure for pending diffs
    error.rs              Error enum (thiserror) and Result alias for the crate
  teereader/
    mod.rs                 Splits one Reader into two independent Readers (tee), AI-generated (see comment)
```

Key behaviors to keep in mind when editing:
- `replace_path` recurses into directories and, for files, writes the result
  to a temporary file first, then renames it over the original (atomic-ish
  replace) rather than editing in place.
- `replace_glob` fans work out across OS threads (`std::thread::scope`), one
  per matched glob path — be careful with shared state/mutability when
  changing this code.
- `bufsearcher.rs` implements manual buffered scanning with hard limits
  (`SEARCH_MAX = 4096 * 1024` bytes between match start/end, `COLUMN_MAX =
  1000` for block-pattern column alignment) — respect these constants' intent
  if refactoring the search loop.
- `teereader/mod.rs` is explicitly marked as AI-generated; review its
  concurrency (Mutex-guarded shared buffer) carefully before modifying.

## Testing Conventions

- Tests are colocated with implementation using `#[cfg(test)] mod tests { ... }`
  blocks (see `src/main.rs` for the pattern: build a `tempfile::TempDir`,
  write files, run the code, assert file contents).
- Run the full suite with `cargo test`. Prefer targeted runs
  (`cargo test <name>`) while iterating, then run the full suite before
  finishing.
- This repo has been mutation-tested (`mutants.out`, `mutants.out.old`
  directories from `cargo-mutants`); avoid deleting these unless asked, and
  keep new code's tests strong enough to catch mutants if you touch tested
  logic.

## Housekeeping Notes

- `flamegraph.svg`, `perf.data`, `perf.data.old`, `perf.fish`, and the
  `.old` mutants directory are profiling/tooling artifacts, not part of the
  library — leave them alone unless the task specifically concerns
  performance profiling.
- Only `/target`, `/mutants.out`, and `/mutants.out.old` are gitignored;
  other large artifacts in the repo root (e.g. `perf.data*`) are currently
  tracked/untracked stray files — check `git status` before assuming a file
  is safe to modify or delete.
