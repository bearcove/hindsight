use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, anyhow::Error>;

fn main() -> Result<()> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("serve") => serve(true),
        Some("dev") => serve(false),
        _ => print_help(),
    }
}

fn print_help() -> Result<()> {
    eprintln!(
        r#"
Tasks:
  serve      Build WASM (release), copy to server, and run with --seed
  dev        Build WASM (dev), copy to server, and run with --seed (faster builds)
"#
    );
    Ok(())
}

fn serve(release: bool) -> Result<()> {
    let project_root = project_root();

    println!("📦 Building WASM frontend...");
    build_wasm(&project_root, release)?;

    println!("📋 Copying WASM to server static directory...");
    copy_wasm(&project_root)?;

    println!("🏗️  Building server binary...");
    build_server(&project_root, release)?;

    println!("🚀 Starting Hindsight server with seed data...");
    run_server(&project_root, release)?;

    Ok(())
}

fn build_wasm(project_root: &Path, release: bool) -> Result<()> {
    let wasm_crate = project_root.join("crates/hindsight-wasm");
    let mut cmd = Command::new("wasm-pack");
    cmd.arg("build")
        .arg("--target")
        .arg("web")
        .current_dir(&wasm_crate);

    if release {
        cmd.arg("--release");
    } else {
        cmd.arg("--dev");
    }

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("wasm-pack build failed");
    }

    Ok(())
}

fn copy_wasm(project_root: &Path) -> Result<()> {
    let src = project_root.join("crates/hindsight-wasm/pkg");
    let dest = project_root.join("crates/hindsight-server/static/wasm");

    // Create destination directory
    std::fs::create_dir_all(&dest)?;

    // Copy all files from pkg to wasm directory
    for entry in std::fs::read_dir(&src)? {
        let entry = entry?;
        let dest_path = dest.join(entry.file_name());
        std::fs::copy(entry.path(), dest_path)?;
    }

    Ok(())
}

fn build_server(project_root: &PathBuf, release: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--bin")
        .arg("hindsight")
        .current_dir(project_root);

    if release {
        cmd.arg("--release");
    }

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("cargo build failed");
    }

    Ok(())
}

fn run_server(project_root: &PathBuf, release: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--bin")
        .arg("hindsight")
        .current_dir(project_root);

    if release {
        cmd.arg("--release");
    }

    cmd.arg("--").arg("serve").arg("--seed");

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("server failed to run");
    }

    Ok(())
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
