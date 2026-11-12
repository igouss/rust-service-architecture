use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::fault::Fault;

const VOLATILE_FILESYSTEMS: [&str; 3] = ["tmpfs", "ramfs", "devtmpfs"];

fn text(output: Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim_end().to_string()
}

fn running(name: &str) -> Result<usize, Fault> {
    Ok(text(Command::new("pgrep").args(["-x", name]).output()?).lines().count())
}

pub fn quiet_line(moment: &str) -> Result<String, Fault> {
    let load: String = fs::read_to_string("/proc/loadavg")?;
    Ok(format!(
        "{moment}: rustc processes={} cargo processes={} loadavg={}",
        running("rustc")?,
        running("cargo")?,
        load.trim_end()
    ))
}

pub fn filesystem(directory: &Path) -> Result<String, Fault> {
    let report: String = text(Command::new("df").arg("--output=fstype,source,target").arg(directory).output()?);
    let line: &str = report
        .lines()
        .nth(1)
        .ok_or_else(|| Fault::Unexpected(format!("df printed no filesystem: {report}")))?;
    let kind: &str = line
        .split_whitespace()
        .next()
        .ok_or_else(|| Fault::Unexpected(format!("df printed no type: {line}")))?;
    if VOLATILE_FILESYSTEMS.contains(&kind) {
        return Err(Fault::VolatileFilesystem(line.to_string()));
    }
    Ok(line.to_string())
}

pub fn disk(filesystem: &str) -> Result<String, Fault> {
    let source: &str = filesystem
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| Fault::Unexpected(format!("df printed no source: {filesystem}")))?;
    let chain: String = text(Command::new("lsblk").args(["-s", "-n", "-l", "-o", "NAME,TYPE,MODEL,TRAN", source]).output()?);
    let devices: Vec<String> = chain
        .lines()
        .map(|line: &str| line.split_whitespace().collect::<Vec<&str>>().join(" "))
        .collect();
    Ok(devices.join(" <- "))
}

pub fn processor() -> Result<String, Fault> {
    let cpuinfo: String = fs::read_to_string("/proc/cpuinfo")?;
    let model: &str = cpuinfo
        .lines()
        .find(|line: &&str| line.starts_with("model name"))
        .unwrap_or("model name: unknown");
    Ok(model.split_once(':').map_or(model, |(_, name): (&str, &str)| name).trim().to_string())
}

pub fn kernel() -> Result<String, Fault> {
    Ok(fs::read_to_string("/proc/sys/kernel/osrelease")?.trim_end().to_string())
}
