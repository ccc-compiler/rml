//! rmlgo — RML 專案管理工具（類似 cargo），協調 rmlc、rustc、cargo
//!
//! 建置： rustc -O rmlgo.rs -o rmlgo
//! 用法： rmlgo <new|init|build|run|check|clean|help> [args...]

#![allow(dead_code)]

use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return;
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "new" => {
            if args.is_empty() {
                eprintln!("用法：rmlgo new <專案名稱>");
                process::exit(1);
            }
            let name = &args[0];
            if Path::new(name).exists() {
                eprintln!("錯誤：目錄 {} 已存在", name);
                process::exit(1);
            }
            if let Err(e) = new_project(name) {
                eprintln!("建立專案失敗：{}", e);
                process::exit(1);
            }
            println!("已建立專案 {}", name);
        }
        "init" => {
            if let Err(e) = init_project() {
                eprintln!("初始化失敗：{}", e);
                process::exit(1);
            }
            println!("已在目前目錄初始化 RML 專案");
        }
        "build" => {
            if let Err(e) = build_project() {
                eprintln!("建置失敗：{}", e);
                process::exit(1);
            }
        }
        "run" => {
            if let Err(e) = build_project() {
                eprintln!("建置失敗：{}", e);
                process::exit(1);
            }
            if let Err(e) = run_project() {
                eprintln!("執行失敗：{}", e);
                process::exit(1);
            }
        }
        "check" => {
            if let Err(e) = check_project() {
                eprintln!("型別檢查失敗：{}", e);
                process::exit(1);
            }
        }
        "clean" => {
            if let Err(e) = clean_project() {
                eprintln!("清理失敗：{}", e);
                process::exit(1);
            }
            println!("已清理 target/ 與中間產物");
        }
        "help" | "-h" | "--help" => print_help(),
        _ => {
            eprintln!("未知指令：{}，請用 rmlgo help", cmd);
            process::exit(1);
        }
    }
}

fn print_help() {
    println!("rmlgo — RML 專案管理工具（v0.2）");
    println!("用法：rmlgo <指令> [參數]");
    println!();
    println!("指令：");
    println!("  new <name>  建立新專案");
    println!("  init        在目前目錄初始化專案");
    println!("  build       編譯專案（透過 rmlc 產生 Rust，再由 cargo 建置）");
    println!("  run         建置並執行");
    println!("  check       僅檢查（呼叫 rmlc 驗證型別與所有權）");
    println!("  clean       清理 target/ 及中間 .rs 檔案");
    println!("  help        顯示說明");
}

fn new_project(name: &str) -> std::io::Result<()> {
    let root = PathBuf::from(name);
    fs::create_dir_all(&root)?;
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("target"))?;

    // Rml.toml
    let mut rml_toml = File::create(root.join("Rml.toml"))?;
    writeln!(rml_toml, "[package]")?;
    writeln!(rml_toml, "name = \"{}\"", name)?;
    writeln!(rml_toml, "version = \"0.1.0\"")?;
    writeln!(rml_toml, "edition = \"2021\"")?;
    writeln!(rml_toml)?;

    // src/main.rml
    let mut main_rml = File::create(root.join("src/main.rml"))?;
    writeln!(main_rml, "(* RML v0.2 範例 *)")?;
    writeln!(main_rml, "let main () =")?;
    writeln!(main_rml, "  print_endline \"Hello from rmlgo!\"")?;
    writeln!(main_rml)?;

    // Cargo.toml（橋接：讓 cargo 編譯 rmlgo 產生的 Rust）
    let mut cargo_toml = File::create(root.join("Cargo.toml"))?;
    writeln!(cargo_toml, "[package]")?;
    writeln!(cargo_toml, "name = \"{}\"", name)?;
    writeln!(cargo_toml, "version = \"0.1.0\"")?;
    writeln!(cargo_toml, "edition = \"2021\"")?;
    writeln!(cargo_toml)?;
    writeln!(cargo_toml, "[dependencies]")?;
    writeln!(cargo_toml)?;
    writeln!(cargo_toml, "[lib]")?;
    writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
    writeln!(cargo_toml)?;
    writeln!(cargo_toml, "[[bin]]")?;
    writeln!(cargo_toml, "name = \"{}\"", name)?;
    writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
    writeln!(cargo_toml)?;

    // .gitignore（可選）
    let mut gitignore = File::create(root.join(".gitignore"))?;
    writeln!(gitignore, "/target")?;
    writeln!(gitignore, "**/*.rs.bk")?;
    writeln!(gitignore, "*.pdb")?;
    writeln!(gitignore)?;

    Ok(())
}

fn init_project() -> std::io::Result<()> {
    if Path::new("src").exists() && (Path::new("src/main.rml").exists() || has_rml_files("src")?) {
        // 已有 rml 檔案，僅確保必要檔案存在
    } else if !Path::new("src").exists() {
        fs::create_dir_all("src")?;
        let mut main_rml = File::create("src/main.rml")?;
        writeln!(main_rml, "let main () =")?;
        writeln!(main_rml, "  print_endline \"Hello from rmlgo!\"")?;
        writeln!(main_rml)?;
    }
    fs::create_dir_all("target")?;

    if !Path::new("Rml.toml").exists() {
        let name = current_dir_name();
        let mut rml_toml = File::create("Rml.toml")?;
        writeln!(rml_toml, "[package]")?;
        writeln!(rml_toml, "name = \"{}\"", name)?;
        writeln!(rml_toml, "version = \"0.1.0\"")?;
        writeln!(rml_toml, "edition = \"2021\"")?;
        writeln!(rml_toml)?;
    }

    if !Path::new("Cargo.toml").exists() {
        let name = current_dir_name();
        let mut cargo_toml = File::create("Cargo.toml")?;
        writeln!(cargo_toml, "[package]")?;
        writeln!(cargo_toml, "name = \"{}\"", name)?;
        writeln!(cargo_toml, "version = \"0.1.0\"")?;
        writeln!(cargo_toml, "edition = \"2021\"")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[dependencies]")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[lib]")?;
        writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[[bin]]")?;
        writeln!(cargo_toml, "name = \"{}\"", name)?;
        writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
        writeln!(cargo_toml)?;
    }
    Ok(())
}

fn current_dir_name() -> String {
    env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "rmlproj".to_string())
}

fn has_rml_files(dir: &str) -> std::io::Result<bool> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if has_rml_files(path.to_str().unwrap_or(""))? {
                return Ok(true);
            }
        } else if let Some(ext) = path.extension() {
            if ext == "rml" {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn find_rmlc() -> String {
    // 優先使用本地 ./rmlc（與 repo 同目錄呼叫時最常用）
    if Path::new("./rmlc").exists() {
        return "./rmlc".to_string();
    }
    if Path::new("rmlc").exists() {
        return "rmlc".to_string();
    }
    // 試圖從執行路徑推測：rmlgo 與 rmlc 可能同一目錄
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("rmlc");
            if cand.exists() {
                return cand.to_string_lossy().to_string();
            }
        }
    }
    "rmlc".to_string()
}

fn find_entry_rml() -> std::io::Result<PathBuf> {
    // 常見入口：src/main.rml
    let candidates = vec!["src/main.rml", "main.rml"];
    for c in &candidates {
        if Path::new(c).exists() {
            return Ok(PathBuf::from(c));
        }
    }
    // 搜尋第一個 .rml 檔案
    fn scan(dir: &str, found: &mut Option<PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                scan(path.to_str().unwrap_or(""), found)?;
                if found.is_some() {
                    return Ok(());
                }
            } else if let Some(ext) = path.extension() {
                if ext == "rml" {
                    *found = Some(path);
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    let mut found: Option<PathBuf> = None;
    scan(".", &mut found)?;
    found.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "找不到任何 .rml 檔案（請將主檔案放在 src/main.rml 或專案根目錄）"))
}

fn build_project() -> std::io::Result<()> {
    fs::create_dir_all("target")?;
    let entry = find_entry_rml()?;
    let rmlc = find_rmlc();
    println!("執行：{} {} --stdout > target/gen.rs", rmlc, entry.display());
    let output = Command::new(&rmlc)
        .arg(entry.to_string_lossy().as_ref())
        .arg("--stdout")
        .output()?;
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        process::exit(output.status.code().unwrap_or(1));
    }
    let mut gen = File::create("target/gen.rs")?;
    gen.write_all(&output.stdout)?;
    println!("已產生 target/gen.rs");

    // 確保 Cargo.toml 存在（若專案僅含 .rml）
    if !Path::new("Cargo.toml").exists() {
        let name = current_dir_name();
        let mut cargo_toml = File::create("Cargo.toml")?;
        writeln!(cargo_toml, "[package]")?;
        writeln!(cargo_toml, "name = \"{}\"", name)?;
        writeln!(cargo_toml, "version = \"0.1.0\"")?;
        writeln!(cargo_toml, "edition = \"2021\"")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[dependencies]")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[lib]")?;
        writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
        writeln!(cargo_toml)?;
        writeln!(cargo_toml, "[[bin]]")?;
        writeln!(cargo_toml, "name = \"{}\"", name)?;
        writeln!(cargo_toml, "path = \"target/gen.rs\"")?;
        writeln!(cargo_toml)?;
    }

    println!("執行：cargo build --release");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .status()?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    println!("建置完成（target/release/）");
    Ok(())
}

fn run_project() -> std::io::Result<()> {
    // 先嘗試從 Cargo.toml 取得套件名稱
    let name = read_pkg_name().unwrap_or_else(|_| current_dir_name());
    let bin_path = PathBuf::from(format!("target/release/{}", name));
    if !bin_path.exists() {
        // 試著找第一個執行檔
        if let Some(p) = find_exe("target/release")? {
            println!("執行：{}", p.display());
            let status = Command::new(p).status()?;
            if !status.success() {
                process::exit(status.code().unwrap_or(1));
            }
            return Ok(());
        }
        eprintln!("找不到執行檔（請先執行 rmlgo build）");
        process::exit(1);
    }
    println!("執行：{}", bin_path.display());
    let status = Command::new(&bin_path).status()?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn read_pkg_name() -> std::io::Result<String> {
    let mut s = String::new();
    File::open("Cargo.toml")?.read_to_string(&mut s)?;
    for line in s.lines() {
        let l = line.trim();
        if l.starts_with("name") && l.contains('=') {
            let v = l.split('=').nth(1).unwrap_or("").trim();
            let v = v.trim_matches('"').trim_matches('\'');
            return Ok(v.to_string());
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Cargo.toml 缺少 name"))
}

fn find_exe(dir: &str) -> std::io::Result<Option<PathBuf>> {
    if !Path::new(dir).exists() {
        return Ok(None);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            // 簡單判斷：非 .dSYM 等目錄檔案
            if let Some(name) = path.file_name().and_then(|x| x.to_str()) {
                if name.ends_with(".dSYM") || name.ends_with(".d") {
                    continue;
                }
            }
            // 檢查是否可執行（Unix）
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = fs::metadata(&path)?;
                if meta.permissions().mode() & 0o111 != 0 {
                    return Ok(Some(path));
                }
                // 有些情況下檔案就是執行檔
                return Ok(Some(path));
            }
            #[cfg(not(unix))]
            {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn check_project() -> std::io::Result<()> {
    let entry = find_entry_rml()?;
    let rmlc = find_rmlc();
    println!("執行：{} {}（僅型別／所有權檢查）", rmlc, entry.display());
    let status = Command::new(&rmlc)
        .arg(entry.to_string_lossy().as_ref())
        .status()?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    println!("檢查通過");
    Ok(())
}

fn clean_project() -> std::io::Result<()> {
    // 清理 target 目錄
    if Path::new("target").exists() {
        fs::remove_dir_all("target")?;
    }
    fs::create_dir_all("target")?;
    // 清理根目錄下由 rmlc 產生的 .rs（非 target/ 下的）
    for entry in fs::read_dir(".")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "rs" {
                    // 避免誤刪非本專案產生的檔案？簡單做：刪除與 .rml 同名的 .rs
                    if let Some(stem) = path.file_stem() {
                        let rml_cand = format!("{}.rml", stem.to_string_lossy());
                        if Path::new(&rml_cand).exists() {
                            fs::remove_file(&path)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
