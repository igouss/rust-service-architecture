mod commands;
mod fault;
mod hop;
mod machine;
mod measure;
mod queries;
mod stats;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adapter_sqlite::Durability;
use fault::Fault;
use tokio::runtime::Runtime;

const COMMIT: Option<&str> = option_env!("BENCH_COMMIT");
const COMPILER: Option<&str> = option_env!("BENCH_RUSTC");

fn measure(directory: &Path, label: &str, durability: Durability, runtime: &Runtime) -> Result<(), Fault> {
    fs::create_dir_all(directory)?;
    println!();
    println!("== Durability::{durability:?}: synchronous={label} asked of the adapter, which exposes no read-back ==");
    commands::histories(directory, durability)?;
    commands::commands(directory, durability)?;
    queries::queries(directory, durability)?;
    hop::hop(directory, durability, runtime)
}

fn run() -> Result<(), Fault> {
    let started: Duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error: std::time::SystemTimeError| Fault::Unexpected(error.to_string()))?;
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/bench-data")
        .join(format!("{}-{}", started.as_secs(), std::process::id()));
    fs::create_dir_all(&root)?;
    let root: PathBuf = root.canonicalize()?;
    let mounted: String = machine::filesystem(&root)?;
    let runtime: Runtime = tokio::runtime::Builder::new_multi_thread().build()?;

    println!("state rows with facts as history on SQLite: commands, queries and the thread hop");
    println!("binary built from: {}", COMMIT.unwrap_or("BENCH_COMMIT was not set at build time"));
    println!("compiler: {}", COMPILER.unwrap_or("BENCH_RUSTC was not set at build time"));
    println!("started at unix time {}", started.as_secs());
    println!("processor: {}", machine::processor()?);
    println!("kernel: {}", machine::kernel()?);
    println!("sqlite: {} (bundled with rusqlite)", rusqlite::version());
    println!("tokio: multi-thread runtime, {} worker threads", runtime.metrics().num_workers());
    println!("data directory: {}", root.display());
    println!("filesystem (type, source, mount): {mounted}");
    println!("disk: {}", machine::disk(&mounted)?);
    println!("{}", machine::quiet_line("before")?);

    measure(&root.join("full"), "FULL", Durability::Full, &runtime)?;
    measure(&root.join("normal"), "NORMAL", Durability::Normal, &runtime)?;

    println!();
    println!("{}", machine::quiet_line("after")?);
    fs::remove_dir_all(&root)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(fault) => {
            eprintln!("bench: {fault}");
            ExitCode::FAILURE
        }
    }
}
