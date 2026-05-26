use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &str, args: &[&str], dir: Option<&str>) -> Result<(), String> {
    println!("==> {} {}", program, args.join(" "));
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("falha ao iniciar {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} saiu com status {status}"))
    }
}

fn copy_artifacts() -> Result<(), String> {
    let dist = Path::new("dist");
    let release = Path::new("release/portable");
    if !dist.exists() {
        return Ok(());
    }
    fs::create_dir_all(release).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(dist).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let is_portable = name.contains("portable")
            || name.ends_with(".AppImage")
            || name.ends_with(".zip");
        if path.is_file() && is_portable {
            fs::copy(&path, release.join(name)).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run_all() {
        eprintln!("commit build falhou: {error}");
        std::process::exit(1);
    }
}

fn run_all() -> Result<(), String> {
    let skip_release = env::var("GDOWNLOADER_SKIP_RELEASE_BUILD").is_ok();

    run("npm", &["run", "lint"], None)?;
    run("npm", &["run", "typecheck"], None)?;
    run("npm", &["test", "--", "--run"], None)?;
    run("cargo", &["test", "--manifest-path", "backend/Cargo.toml"], None)?;

    if skip_release {
        println!("GDOWNLOADER_SKIP_RELEASE_BUILD definido; build portable pulado.");
        return Ok(());
    }

    run("npm", &["run", "build:portable"], None)?;
    copy_artifacts()?;
    Ok(())
}
