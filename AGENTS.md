# AGENTS.md

This is RML v0.1: a single-file compiler `rmlc.rs` that compiles ML-like RML to Rust. Keep guidance minimal and repo-specific.

## Build

- `rustc -O rmlc.rs -o rmlc` (repo already has `rmlc` binary; rebuild if you change `rmlc.rs`)
- Don't need Cargo or external crates.

## Run compiler

- `./rmlc <prog.rml>` → writes `<prog>.rs` (default)
- `./rmlc <prog.rml> -o out.rs`
- `./rmlc <prog.rml> --run` → compiles to Rust, then `rustc -O` + runs binary
- `./rmlc <prog.rml> --stdout` → print generated Rust to stdout

## Examples & quick check

- Examples: `fib.rml`, `lists.rml`, `tree.rml`, `ownership.rml`, `err_use_after_move.rml`
- Run all: `./run.sh` (runs each with `--run`, shows tail). Note `err_use_after_move.rml` is expected to fail with use-after-move.
- Generated artifacts in `examples/`: `.rs` files and compiled binaries (`fib`, `ownership`, `tree`).

## Language/compiler quirks that matter

- Pipeline: lex → parse → HM type inference → ownership/move check → Rust codegen (all in `rmlc.rs`)
- Top-level only: `let rec` supported at top level; local `let rec` is not supported.
- No partial application/currying.
- ADT fields must not contain functions or references (functions/references are values only).
- Tail-recursive top-level functions become Rust `loop` (e.g. `sum_to`).
- Ownership: move semantics with use-after-move detection; `&` borrow, `&mut` mutable borrow, `!` deref, `:=` assign, `<-` var update, `let mut`. `clone` builtin exists. References cannot move out non-Copy values.
- Strings are `String` (moved by default). `string_length`/builtins that take string by ref are treated as borrowing in codegen where noted.
- Runtime wrapper: generated `main` spawns a thread with large stack (`1<<30`) to avoid deep recursion/long list drop stack overflow.
- Builtins: `print_*`, `string_of_*`, `string_length`, `concat` (via `^`), `float_of_int/int_of_float`, `sqrt`, `abs`, `min`, `max`, `clone` — thin inline wrappers in generated code.
- Predefined: `list` (`Nil | Cons of 'a * 'a list`), `option` (`None | Some of 'a`).
- Rust keywords escaped by appending `_` in generated names; type names normalized.

## Editing compiler

- Single file `rmlc.rs` (Chinese comments). If you modify it, rebuild: `rustc -O rmlc.rs -o rmlc`.
- Preserve existing behavior for examples if possible; run `./run.sh` after changes.
- Error messages go to stderr with filename:line when line>0.

## Repo structure

- `rmlc.rs` — source (v0.1 by Claude)
- `rmlc` — compiled binary
- `examples/*.rml` — source examples
- `examples/*.rs` + binaries — generated artifacts (committed)
- `run.sh` — runs all examples with `--run`
- `README.md` — minimal
- `_doc/v0.1.md` — detailed writeup for this version

