# RML

RML 是一個 ML-like 的函數式程式語言，能將 RML 程式編譯成 Rust 程式碼並執行。

## 版本

- **v0.1**：`rmlc` — 單檔編譯器（lex → parse → HM 型別推論 → 所有權檢查 → Rust codegen），不依賴外部 crate
- **v0.2**：`rmlgo` — 類似 cargo 的專案管理工具，協調 `rmlc`、`cargo`、`rustc`

## 快速開始

### 直接編譯單一檔案（v0.1）

1. 建置編譯器（已預先編譯好 `rmlc`）
   ```bash
   rustc -O rmlc.rs -o rmlc
   ```

2. 編譯並執行 RML 程式
   ```bash
   ./rmlc examples/fib.rml --run
   ```

3. 僅產生 Rust 程式碼
   ```bash
   ./rmlc examples/fib.rml
   # 會產生 examples/fib.rs
   ```

4. 從標準輸出取得產生的 Rust
   ```bash
   ./rmlc examples/fib.rml --stdout > out.rs
   ```

### 使用專案管理工具（v0.2）

1. 建置 rmlgo（已預先編譯好 `rmlgo`）
   ```bash
   rustc -O rmlgo.rs -o rmlgo
   ```

2. 建立新專案
   ```bash
   ./rmlgo new myproj
   cd myproj
   ./rmlgo build
   ./rmlgo run
   ```

3. 或在現有目錄初始化
   ```bash
   mkdir -p src && echo 'let main () = print_endline "hi"' > src/main.rml
   ./rmlgo init
   ./rmlgo run
   ```

## 指令說明

### rmlc（編譯器）
- `./rmlc <file.rml>`：產生 `<file>.rs`
- `./rmlc <file.rml> -o <out.rs>`：指定輸出檔案
- `./rmlc <file.rml> --run`：編譯為 Rust、再用 `rustc -O` 編譯並執行
- `./rmlc <file.rml> --stdout`：輸出 Rust 程式碼至 stdout

### rmlgo（專案管理）
- `./rmlgo new <name>`：建立專案（`src/main.rml`、`Rml.toml`、`Cargo.toml`、`.gitignore`）
- `./rmlgo init`：在目前目錄初始化
- `./rmlgo build`：`rmlc src/main.rml --stdout` → `target/gen.rs` → `cargo build --release`
- `./rmlgo run`：建置並執行 release 版
- `./rmlgo check`：僅型別與所有權檢查（呼叫 rmlc）
- `./rmlgo clean`：清理 `target/` 與同名中間 `.rs` 檔案
- `./rmlgo help`：顯示說明

## 範例

執行所有範例：
```bash
./run.sh
```

範例檔案：`examples/fib.rml`、`examples/lists.rml`、`examples/tree.rml`、`examples/ownership.rml`、`examples/err_use_after_move.rml`（預期會報告 use-after-move 錯誤）。

## 語言特色（v0.1）

- ML-like 語法：`let`、`let rec`（頂層）、`fun`、`match with`
- HM 型別推論、泛型、代數資料型別（ADT）
- 所有權／移動語義：move、`&` 借用、`&mut` 可變借用、`!` 解參考、`let mut`、`<-`/`:=`
- 使用後移動偵測
- 字串為 `String`（預設移動）；內建 `clone`
- 頂層尾遞迴自動轉換為 `loop`
- 內建型別與函式：`int`/`float`/`bool`/`unit`/`string`、`list`/`option`、`print_*`、`string_of_*` 等
- 產生的程式碼以大堆疊執行緒執行，避免深遞迴堆疊溢位

## 專案檔案

- `rmlc.rs`、`rmlc`：編譯器（v0.1）
- `rmlgo.rs`、`rmlgo`：專案管理工具（v0.2）
- `examples/`：範例程式與產生結果
- `run.sh`：批次執行所有範例
- `_doc/v0.1.md`、`_doc/v0.2.md`：詳細設計文件
- `AGENTS.md`：給 AI Agent 的作業指引

## 詳細文件

更多實作細節請參考 `_doc/v0.1.md` 與 `_doc/v0.2.md`。
