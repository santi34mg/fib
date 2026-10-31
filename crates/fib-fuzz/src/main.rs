use anyhow::{Context, Result, bail};
mod engine;
use fib_frontend::{
    CompilationUnit,
    frontend::{analyzer::Analyzer, parser::Parser, tokenizer::Tokenizer},
    plan::{BuildPlan, FileId, SourceFile},
    standard_library::StandardLibrary,
};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn worker(path: &Path) -> Result<()> {
    let source = fs::read_to_string(path)?;
    Tokenizer::new(&source).for_each(drop);
    let ast = match Parser::with_source(Tokenizer::new(&source), Path::new("input.fib"), &source)
        .parse()
    {
        Ok(ast) => ast,
        Err(error) => {
            println!("parse rejected: {error:#}");
            return Ok(());
        }
    };
    println!("parse accepted");
    let plan = BuildPlan {
        standard_library: StandardLibrary::disabled(),
        native: Default::default(),
        packages: vec![],
        files: vec![SourceFile {
            id: FileId(0),
            name: "main".to_string().into(),
            path: "input.fib".into(),
        }],
        entry: FileId(0),
        output: "unused".into(),
    };
    match Analyzer::new(
        plan,
        vec![CompilationUnit {
            source_file_id: FileId(0),
            ast,
        }],
    )?
    .analyze()
    {
        Ok(_) => println!("analysis accepted"),
        Err(error) => println!("analysis rejected: {error:#}"),
    }
    Ok(())
}

fn evaluate(exe: &Path, input: &Path, profile: &Path, timeout: Duration) -> Result<String> {
    let mut child = Command::new(exe)
        .arg("worker")
        .arg(input)
        .env("LLVM_PROFILE_FILE", profile)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    fn capture(mut reader: impl std::io::Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
        thread::spawn(move || {
            let mut saved = Vec::new();
            let mut buffer = [0; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let keep = count.min(16384usize.saturating_sub(saved.len()));
                saved.extend_from_slice(&buffer[..keep]);
            }
            saved
        })
    }
    let stdout = capture(child.stdout.take().context("worker stdout")?);
    let stderr = capture(child.stderr.take().context("worker stderr")?);
    let start = Instant::now();
    let outcome = loop {
        if let Some(status) = child.try_wait()? {
            break if status.success() {
                "ok".to_owned()
            } else {
                format!("crash:{status}")
            };
        }
        if start.elapsed() > timeout {
            child.kill()?;
            child.wait()?;
            break "timeout".into();
        }
        thread::sleep(Duration::from_millis(2));
    };
    let out = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader failed"))?;
    let err = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader failed"))?;
    fs::write(profile.with_extension("stdout"), out)?;
    fs::write(profile.with_extension("stderr"), &err)?;
    if outcome.starts_with("crash") {
        let text = String::from_utf8_lossy(&err);
        let signature = text
            .lines()
            .find(|line| line.contains("panicked at"))
            .and_then(|line| line.split_once("panicked at").map(|(_, location)| location))
            .unwrap_or("");
        return Ok(format!("{outcome}:{signature}"));
    }
    Ok(outcome)
}

fn coverage(exe: &Path, dir: &Path) -> Result<BTreeSet<String>> {
    fn tool(name: &str, variable: &str) -> std::path::PathBuf {
        if let Some(path) = std::env::var_os(variable) {
            return path.into();
        }
        if let Ok(output) = Command::new("rustc").args(["--print", "sysroot"]).output() {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if let Ok(entries) = fs::read_dir(Path::new(&root).join("lib/rustlib")) {
                for entry in entries.flatten() {
                    let path = entry.path().join("bin").join(name);
                    if path.is_file() {
                        return path;
                    }
                }
            }
        }
        name.into()
    }
    static PROFDATA: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    static COV: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let status =
        Command::new(PROFDATA.get_or_init(|| tool("llvm-profdata", "FIB_FUZZ_LLVM_PROFDATA")))
            .args(["merge", "-sparse"])
            .arg(dir.join("input.profraw"))
            .arg("-o")
            .arg(dir.join("input.profdata"))
            .output()?;
    if !status.status.success() {
        bail!(
            "coverage merge failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    let output = Command::new(COV.get_or_init(|| tool("llvm-cov", "FIB_FUZZ_LLVM_COV")))
        .arg("export")
        .args(["--skip-functions", "--skip-expansions"])
        .arg(exe)
        .arg(format!(
            "-instr-profile={}",
            dir.join("input.profdata").display()
        ))
        .output()?;
    if !output.status.success() {
        bail!(
            "coverage export failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    parse_coverage(&json)
}

fn parse_coverage(json: &serde_json::Value) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for data in json["data"].as_array().context("missing coverage data")? {
        for file in data["files"].as_array().context("missing coverage files")? {
            let name = file["filename"].as_str().unwrap_or("");
            if !name.contains("fib-frontend/src/") {
                continue;
            }
            for segment in file["segments"].as_array().context("missing segments")? {
                if segment[2].as_u64().unwrap_or(0) > 0 && segment[3] == true {
                    result.insert(format!("{name}:{}:{}", segment[0], segment[1]));
                }
            }
        }
    }
    if result.is_empty() {
        bail!("no frontend coverage; build with RUSTFLAGS=-C instrument-coverage");
    }
    Ok(result)
}

struct Rng(u64);
impl Rng {
    fn pick(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 as usize) % n
    }
}
fn main() -> Result<()> {
    engine::main()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_filters_harness_and_rejects_missing_data() {
        let json = serde_json::json!({"data":[{"files":[
            {"filename":"/repo/crates/fib-frontend/src/frontend/parser.rs", "segments":[[1,1,1,true],[2,1,0,true]]},
            {"filename":"/repo/crates/fib-fuzz/src/main.rs", "segments":[[1,1,99,true]]}
        ]}]});
        let covered = parse_coverage(&json).unwrap();
        assert_eq!(covered.len(), 1);
        assert!(covered.first().unwrap().ends_with("parser.rs:1:1"));
        assert!(parse_coverage(&serde_json::json!({})).is_err());
        assert!(parse_coverage(&serde_json::json!({"data":[{"files":[]}]})).is_err());
    }
    #[test]
    fn isolated_worker_outcomes_and_bounded_capture() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("fixture.rs");
        let exe = temp.path().join("fixture");
        fs::write(
            &source,
            r#"fn main() {
            let path = std::env::args().nth(2).unwrap();
            match std::fs::read_to_string(path).unwrap().as_str() {
                "panic" => panic!("fixture panic"),
                "abort" => std::process::exit(7),
                "hang" => loop { std::thread::sleep(std::time::Duration::from_secs(1)); },
                "verbose" => { for _ in 0..20000 { println!("diagnostic output"); } },
                _ => (),
            }
        }"#,
        )?;
        assert!(
            Command::new("rustc")
                .arg(&source)
                .arg("-o")
                .arg(&exe)
                .status()?
                .success()
        );
        let input = temp.path().join("input.fib");
        let profile = temp.path().join("input.profraw");
        for (text, expected) in [
            ("ok", "ok"),
            ("verbose", "ok"),
            ("panic", "crash:"),
            ("abort", "crash:"),
            ("hang", "timeout"),
        ] {
            fs::write(&input, text)?;
            let outcome = evaluate(&exe, &input, &profile, Duration::from_millis(500))?;
            assert!(outcome.starts_with(expected), "{text}: {outcome}");
            assert!(fs::metadata(profile.with_extension("stdout"))?.len() <= 16384);
        }
        Ok(())
    }
}
