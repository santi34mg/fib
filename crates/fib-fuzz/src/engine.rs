use super::*;
use clap::{Parser as CliParser, Subcommand, ValueEnum};
use fib_frontend::frontend::tokens::TokenKind;
use std::{cmp::Reverse, collections::BTreeMap, path::PathBuf};

#[derive(CliParser)]
#[command(about = "Coverage-guided genetic fuzzing of the Fib frontend")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Run(Options),
    Replay {
        file: PathBuf,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    Minimize {
        file: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = Mode::Chars)]
        mode: Mode,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    #[command(hide = true)]
    Worker {
        file: PathBuf,
    },
}
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Mode {
    Chars,
    Tokens,
    Both,
}
#[derive(clap::Args)]
struct Options {
    #[arg(long, value_enum, default_value_t = Mode::Both)]
    mode: Mode,
    #[arg(long, default_value_t = 10000)]
    evaluations: usize,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 64)]
    population: usize,
    #[arg(long, default_value_t = 16384)]
    max_bytes: usize,
    #[arg(long, default_value_t = 2048)]
    max_tokens: usize,
    #[arg(long, default_value_t = 2000)]
    timeout_ms: u64,
    #[arg(long)]
    corpus: Option<PathBuf>,
    #[arg(long, default_value = "out/fuzz")]
    output: PathBuf,
}

// Token spans are character columns, not byte offsets. Preserve original spelling,
// including escaped literals and multi-character operators.
fn genes(source: &str, tokens: bool) -> Option<Vec<String>> {
    if !tokens {
        return Some(source.chars().map(|c| c.to_string()).collect());
    }
    let mut positions = BTreeMap::new();
    let (mut line, mut column) = (1, 1);
    for (offset, c) in source.char_indices() {
        positions.insert((line, column), offset);
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    positions.insert((line, column), source.len());
    let mut result = Vec::new();
    for token in Tokenizer::new(source) {
        match token.kind {
            TokenKind::Error(_) | TokenKind::Unknown(_) => return None,
            TokenKind::Comment => continue,
            _ => {}
        }
        let start = *positions.get(&(token.line, token.column))?;
        let end = *positions.get(&(token.end_line, token.end_column + 1))?;
        result.push(source.get(start..end)?.to_owned());
    }
    Some(result)
}
fn render(genes: &[String], tokens: bool) -> String {
    genes.join(if tokens { " " } else { "" })
}
fn bounded(mut items: Vec<String>, tokens: bool, options: &Options) -> String {
    if tokens {
        items = genes(&render(&items, true), true).unwrap_or_default();
        items.truncate(options.max_tokens);
    }
    let mut size = items.iter().map(String::len).sum::<usize>()
        + if tokens {
            items.len().saturating_sub(1)
        } else {
            0
        };
    while size > options.max_bytes {
        if let Some(last) = items.pop() {
            size = size.saturating_sub(last.len() + usize::from(tokens && !items.is_empty()));
        } else {
            break;
        }
    }
    render(&items, tokens)
}
fn random_gene(tokens: bool, rng: &mut Rng) -> String {
    const DICT: &[&str] = &[
        "fn", "main", "(", ")", "{", "}", "[", "]", "return", "0", "1", ";", "int32", "const",
        "comp", "x", ":=", ":", "+", "while", "true", "\"text\"", "struct", "enum", "if", "else",
        "*", "=", ",", "as", "void",
    ];
    if tokens {
        match rng.pick(5) {
            0 => format!("name{}", rng.pick(1000)),
            1 => rng.pick(65536).to_string(),
            _ => DICT[rng.pick(DICT.len())].into(),
        }
    } else {
        loop {
            if let Some(c) = char::from_u32(if rng.pick(5) == 0 {
                rng.pick(0x110000) as u32
            } else {
                rng.pick(128) as u32
            }) {
                return c.to_string();
            }
        }
    }
}
fn offspring(a: &str, b: &str, tokens: bool, rng: &mut Rng, options: &Options) -> String {
    let mut items = genes(a, tokens).unwrap_or_default();
    if rng.pick(100) < 20 {
        let other = genes(b, tokens).unwrap_or_default();
        items.truncate(rng.pick(items.len() + 1));
        items.extend_from_slice(&other[rng.pick(other.len() + 1)..]);
    }
    for _ in 0..1 + rng.pick(4) {
        let at = rng.pick(items.len() + 1);
        match rng.pick(4) {
            0 if at < items.len() => {
                items.remove(at);
            }
            1 if at < items.len() => items[at] = random_gene(tokens, rng),
            2 if at < items.len() => {
                let end = at + 1 + rng.pick(items.len() - at);
                let copy = items[at..end].to_vec();
                items.splice(at..at, copy);
            }
            _ => items.insert(at, random_gene(tokens, rng)),
        }
    }
    bounded(items, tokens, options)
}
#[derive(Clone)]
struct Candidate {
    source: String,
    coverage: BTreeSet<String>,
    novelty: usize,
}
fn rank(population: &mut [Candidate]) {
    let mut frequencies = BTreeMap::new();
    for candidate in population.iter() {
        for region in &candidate.coverage {
            *frequencies.entry(region.clone()).or_insert(0usize) += 1;
        }
    }
    population.sort_by_cached_key(|candidate| {
        (
            Reverse(candidate.novelty),
            Reverse(
                candidate
                    .coverage
                    .iter()
                    .map(|r| 1_000_000 / frequencies[r])
                    .sum::<usize>(),
            ),
            Reverse(candidate.coverage.len()),
            candidate.source.len(),
        )
    });
}
fn select(population: &[Candidate], rng: &mut Rng) -> usize {
    (0..3).map(|_| rng.pick(population.len())).min().unwrap()
}
fn fingerprint(path: &Path) -> Result<String> {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in fs::read(path)? {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}
fn run(options: Options) -> Result<()> {
    if options.seed == 0
        || options.population < 4
        || options.max_bytes == 0
        || options.max_tokens == 0
        || options.timeout_ms == 0
    {
        bail!("seed and limits must be positive; population must be at least four");
    }
    // A fresh directory prevents accidental corpus overwrites, including after failure.
    fs::create_dir_all(options.output.parent().unwrap_or(Path::new(".")))?;
    fs::create_dir(&options.output).context("output must be a new directory")?;
    let dir = &options.output;
    let exe = std::env::current_exe()?;
    let timeout = Duration::from_millis(options.timeout_ms);
    let mut seeds = vec![
        String::new(),
        "fn main() int32 { return 0; }".into(),
        "fn f(x: int32) int32 { if (x == 0) { return 1; } else { return x + 1; }; }".into(),
    ];
    if let Some(corpus) = &options.corpus {
        let mut paths = fs::read_dir(corpus)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            if path.extension().is_some_and(|x| x == "fib") {
                let source = fs::read_to_string(&path)?;
                if source.len() <= options.max_bytes {
                    seeds.push(source);
                }
            }
        }
    }
    let mut rng = Rng(options.seed);
    let mut populations: [Vec<Candidate>; 2] = [vec![], vec![]];
    let mut seen = [BTreeSet::new(), BTreeSet::new()];
    let mut pending = [
        seeds.clone(),
        seeds
            .into_iter()
            .filter(|s| genes(s, true).is_some())
            .collect(),
    ];
    let mut next: [Vec<Candidate>; 2] = [vec![], vec![]];
    let mut generations = [0usize; 2];
    let mut findings = 0;
    let started = Instant::now();
    fs::write(
        dir.join("run.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"arguments":std::env::args().collect::<Vec<_>>(),"seed":options.seed,"worker":exe,"fingerprint":fingerprint(&exe)?}),
        )?,
    )?;
    // An actual profile round trip checks instrumentation and tool compatibility.
    fs::write(dir.join("input.fib"), "fn main() int32 { return 0; }")?;
    if evaluate(
        &exe,
        &dir.join("input.fib"),
        &dir.join("input.profraw"),
        timeout,
    )? != "ok"
    {
        bail!("coverage preflight worker failed");
    }
    coverage(&exe, dir).context(
        "coverage preflight: build with -C instrument-coverage and use matching LLVM tools",
    )?;
    for i in 0..options.evaluations {
        let index =
            usize::from(options.mode == Mode::Tokens || (options.mode == Mode::Both && i % 2 == 1));
        let tokens = index == 1;
        let source = if let Some(seed) = pending[index].pop() {
            bounded(
                genes(&seed, tokens).context("invalid seed")?,
                tokens,
                &options,
            )
        } else if populations[index].is_empty() || rng.pick(100) < 10 {
            let length = 1 + rng.pick(if tokens { 64 } else { 256 });
            bounded(
                (0..length).map(|_| random_gene(tokens, &mut rng)).collect(),
                tokens,
                &options,
            )
        } else {
            let a = select(&populations[index], &mut rng);
            let b = select(&populations[index], &mut rng);
            offspring(
                &populations[index][a].source,
                &populations[index][b].source,
                tokens,
                &mut rng,
                &options,
            )
        };
        fs::write(dir.join("input.fib"), &source)?;
        let profile = dir.join("input.profraw");
        if profile.exists() {
            fs::remove_file(&profile)?;
        }
        let outcome = evaluate(&exe, &dir.join("input.fib"), &profile, timeout)?;
        let mut candidate = Candidate {
            source: source.clone(),
            coverage: BTreeSet::new(),
            novelty: 0,
        };
        if outcome != "ok" {
            findings += 1;
            let kind = if outcome == "timeout" {
                "timeout"
            } else {
                "crash"
            };
            fs::write(dir.join(format!("{kind}-{i}.fib")), &source)?;
            fs::copy(
                profile.with_extension("stderr"),
                dir.join(format!("{kind}-{i}.stderr")),
            )?;
            fs::copy(
                profile.with_extension("stdout"),
                dir.join(format!("{kind}-{i}.stdout")),
            )?;
            fs::write(
                dir.join(format!("{kind}-{i}.json")),
                serde_json::to_vec(
                    &serde_json::json!({"outcome":outcome,"evaluation":i,"mode":if tokens {"tokens"} else {"chars"},"seed":options.seed}),
                )?,
            )?;
        } else {
            candidate.coverage = coverage(&exe, dir)?;
            candidate.novelty = candidate.coverage.difference(&seen[index]).count();
            if candidate.novelty > 0 {
                seen[index].extend(candidate.coverage.iter().cloned());
                fs::write(dir.join(format!("corpus-{index}-{i}.fib")), &source)?;
                if options.mode == Mode::Both && genes(&source, !tokens).is_some() {
                    pending[1 - index].insert(0, source);
                }
            }
        }
        next[index].push(candidate);
        if next[index].len() >= options.population - populations[index].len().min(4) {
            let mut generation = std::mem::take(&mut next[index]);
            generation.extend(populations[index].iter().take(4).cloned());
            rank(&mut generation);
            generation.truncate(options.population);
            populations[index] = generation;
            generations[index] += 1;
        }
        if i % 100 == 0 || i + 1 == options.evaluations {
            println!(
                "evaluations={} generations={:?} coverage={}/{} findings={} rate={:.1}/s",
                i + 1,
                generations,
                seen[0].len(),
                seen[1].len(),
                findings,
                (i + 1) as f64 / started.elapsed().as_secs_f64()
            );
            fs::write(
                dir.join("summary.json"),
                serde_json::to_vec(
                    &serde_json::json!({"evaluations":i+1,"generations":generations,"coverage":[seen[0].len(),seen[1].len()],"findings":findings}),
                )?,
            )?;
        }
    }
    Ok(())
}
fn minimize(file: &Path, output: &Path, tokens: bool, timeout: Duration) -> Result<()> {
    let exe = std::env::current_exe()?;
    minimize_with_worker(&exe, file, output, tokens, timeout)
}
fn minimize_with_worker(
    exe: &Path,
    file: &Path,
    output: &Path,
    tokens: bool,
    timeout: Duration,
) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("input.fib");
    let profile = temp.path().join("input.profraw");
    let source = fs::read_to_string(file)?;
    let mut items = genes(&source, tokens).context("input is not tokenizable")?;
    fs::write(&input, &source)?;
    let expected = evaluate(exe, &input, &profile, timeout)?;
    if expected == "ok" {
        bail!("input does not crash or time out");
    }
    if evaluate(exe, &input, &profile, timeout)? != expected {
        bail!("finding is not reproducible");
    }
    fs::write(&input, render(&items, tokens))?;
    if evaluate(exe, &input, &profile, timeout)? != expected {
        bail!("token rendering changes this finding; minimize with --mode chars");
    }
    let mut width = items.len().div_ceil(2).max(1);
    loop {
        let mut at = 0;
        while at < items.len() {
            let mut trial = items.clone();
            trial.drain(at..(at + width).min(trial.len()));
            fs::write(&input, render(&trial, tokens))?;
            if evaluate(exe, &input, &profile, timeout)? == expected
                && evaluate(exe, &input, &profile, timeout)? == expected
            {
                items = trial;
            } else {
                at += width;
            }
        }
        if width == 1 {
            break;
        }
        width = width.div_ceil(2);
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(render(&items, tokens).as_bytes())
        })?;
    Ok(())
}
pub(super) fn main() -> Result<()> {
    match Cli::parse().command {
        Action::Worker { file } => worker(&file),
        Action::Run(options) => run(options),
        Action::Replay { file, timeout_ms } => {
            let temp = tempfile::tempdir()?;
            let outcome = evaluate(
                &std::env::current_exe()?,
                &file,
                &temp.path().join("replay.profraw"),
                Duration::from_millis(timeout_ms),
            )?;
            println!("{outcome}");
            print!("{}", fs::read_to_string(temp.path().join("replay.stdout"))?);
            eprint!("{}", fs::read_to_string(temp.path().join("replay.stderr"))?);
            Ok(())
        }
        Action::Minimize {
            file,
            output,
            mode,
            timeout_ms,
        } => {
            if mode == Mode::Both {
                bail!("choose chars or tokens for minimization");
            }
            minimize(
                &file,
                &output,
                mode == Mode::Tokens,
                Duration::from_millis(timeout_ms),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimization_preserves_failure_and_refuses_overwrite() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let fixture = temp.path().join("fixture.rs");
        let exe = temp.path().join("fixture");
        fs::write(
            &fixture,
            r#"fn main() { let path = std::env::args().nth(2).unwrap(); if std::fs::read_to_string(path).unwrap().contains("BUG") { std::process::exit(7); } }"#,
        )?;
        assert!(
            Command::new("rustc")
                .arg(&fixture)
                .arg("-o")
                .arg(&exe)
                .status()?
                .success()
        );
        let input = temp.path().join("input.fib");
        let output = temp.path().join("small.fib");
        fs::write(&input, "prefix BUG suffix")?;
        minimize_with_worker(&exe, &input, &output, false, Duration::from_secs(2))?;
        assert_eq!(fs::read_to_string(&output)?, "BUG");
        assert!(
            minimize_with_worker(&exe, &input, &output, false, Duration::from_secs(2)).is_err()
        );
        Ok(())
    }
    #[test]
    fn token_spelling_round_trip() {
        let source = "fn f() { x := \"a b\\n\"; /* comment */ λ := 42; }";
        let items = genes(source, true).unwrap();
        assert!(items.contains(&"\"a b\\n\"".into()));
        assert_eq!(genes(&render(&items, true), true).unwrap(), items);
        assert!(genes("\"unterminated", true).is_none());
    }
    #[test]
    fn deterministic_bounded_mutation() {
        let options = Options {
            mode: Mode::Both,
            evaluations: 10,
            seed: 1,
            population: 4,
            max_bytes: 80,
            max_tokens: 10,
            timeout_ms: 2000,
            corpus: None,
            output: "unused".into(),
        };
        for tokens in [false, true] {
            let (mut a, mut b) = (Rng(42), Rng(42));
            for _ in 0..500 {
                let x = offspring("fn f ( ) { }", "λ := 1 ;", tokens, &mut a, &options);
                assert_eq!(
                    x,
                    offspring("fn f ( ) { }", "λ := 1 ;", tokens, &mut b, &options)
                );
                assert!(x.len() <= 80);
                if tokens {
                    assert!(genes(&x, true).unwrap().len() <= 10);
                }
            }
        }
    }
    #[test]
    fn fitness_prefers_novelty_then_rarity() {
        let make = |source: &str, regions: &[&str], novelty| Candidate {
            source: source.into(),
            coverage: regions.iter().map(|s| s.to_string()).collect(),
            novelty,
        };
        let mut population = vec![
            make("common", &["a"], 0),
            make("rare", &["b"], 0),
            make("new", &["a"], 1),
        ];
        rank(&mut population);
        assert_eq!(population[0].source, "new");
        assert_eq!(population[1].source, "rare");
    }
}
