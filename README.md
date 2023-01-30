# WhatTheStack

A command line tool for parsing LLVM stack information, based on `cargo-call-stack` and `stack-sizes`.

## Status

[![ci](https://github.com/ryankurte/whatthestack/actions/workflows/ci.yml/badge.svg)](https://github.com/ryankurte/whatthestack/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/whatthestack.svg)](https://crates.io/crates/whatthestack)
[![Docs.rs](https://docs.rs/whatthestack/badge.svg)](https://docs.rs/whatthestack)


## Usage

Enable `-Zemit-stack-sizes` by adding this to `RUSTFLAGS` in `.cargo/config.toml`

```toml
[build]
rustflags = [
    "-Z", "emit-stack-sizes",
]
```

Call `wts` with your compiled (`ELF` format) binary to retrieve a list of functions and stack sizes. Note that `lto = "full"` may result in LLVM inlining some methods (or your entire application), this may be influenced via the tactical addition of `#[inline(never)]` attributes to force the creation of separate stack frames.

```
> wts --help

WhatTheStack (wts), a tool for analysing stack use via LLVM `-Zemit-stack-sizes` information

Usage: wts [OPTIONS] <FILE>

Arguments:
  <FILE>
          ELF or object file for parsing

Options:
      --mode <MODE>
          ELF or object file mode
          
          [default: elf]

          Possible values:
          - elf:    Load ELF file
          - object: Load Object File

      --sort <SORT>
          Sort by function or stack size
          
          [default: stack]

          Possible values:
          - text:  Sort by function size
          - stack: Sort by stack size

      --min-size <MIN_SIZE>
          Minimum size for filtering
          
          [default: 16]

  -n, --lines <LINES>
          Number of lines to show
          
          [default: 10]

      --map-source
          Resolve addresses to source locations

      --long-names
          Disable function name shortening

      --log-level <LOG_LEVEL>
          Log level
          
          [default: info]

  -h, --help
          Print help (see a summary with '-h')
```