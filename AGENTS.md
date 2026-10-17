# AGENTS.md

This is RML v0.1 + v0.2: a single-file compiler `rmlc.rs` that compiles ML-like RML to Rust, plus `rmlgo.rs` (cargo-like project manager). Keep guidance minimal and repo-specific.

## Build

- rmlc: `rustc -O rmlc.rs -o rmlc` (repo has `rmlc` binary; rebuild if you change `rmlc.rs`)
- rmlgo: `rustc -O rmlgo.rs -o rmlgo` (repo has `rmlgo` binary; rebuild if you change `rmlgo.rs`)
- Don't need Cargo or external crates for the tools themselves.

## Run compiler (rmlc)

- `./rmlc <prog.rml>` → writes `<prog>.rs` (default)
- `./rmlc <prog.rml> -o out.rs`
- `./rmlc <prog.rml> --run` → compiles to Rust, then `rustc -O` + runs binary
- `./rmlc <prog.rml> --stdout` → print generated Rust to stdout

## rmlgo (project manager, v0.2)

- `./rmlgo new <name>` → create project with `src/main.rml`, `Rml.toml`, `Cargo.toml`, `.gitignore`
- `./rmlgo init` → initialize in current dir
- `./rmlgo build` → `rmlc src/main.rml --stdout` → `target/gen.rs`, then `cargo build --release`
- `./rmlgo run` → build + run release binary
- `./rmlgo check` → type/ownership check via `rmlc` (no full build)
- `./rmlgo clean` → clean `target/` and root `.rs` files matching `.rml` names
- `./rmlgo help` → usage

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

## rmlgo quirks (v0.2)

- Single-file `rmlgo.rs`; calls `rmlc` (prefers local `./rmlc`), `cargo`, `rustc` as needed. Does not modify `rmlc` behavior.
- `build` writes `rmlc --stdout` to `target/gen.rs` and runs `cargo build --release`. Project uses bridge `Cargo.toml` pointing to `target/gen.rs` for both lib/bin.
- Auto-detects entry: `src/main.rml` first, else project root `.rml`, else first found `.rml`. Auto-creates minimal `Cargo.toml`/`Rml.toml` if missing.
- `run` looks up package name from `Cargo.toml` or dir name to find binary in `target/release/`.

## Editing compiler/tools

- `rmlc.rs` (Chinese comments): if modified, rebuild with `rustc -O rmlc.rs -o rmlc`. Preserve existing behavior for examples; run `./run.sh` after changes.
- `rmlgo.rs`: if modified, rebuild with `rustc -O rmlgo.rs -o rmlgo`.
- Error messages go to stderr with filename:line when line>0.

## Repo structure

- `rmlc.rs`, `rmlc` — compiler (v0.1)
- `rmlgo.rs`, `rmlgo` — project manager (v0.2)
- `examples/*.rml` — source examples
- `examples/*.rs` + binaries — generated artifacts (committed)
- `run.sh` — runs all examples with `--run`
- `README.md` — minimal
- `_doc/v0.1.md`, `_doc/v0.2.md` — detailed writeups
