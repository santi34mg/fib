use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf, process};

use fiberc;

#[derive(Parser)]
#[command(name = "fiber", about = "Fiber project CLI")]
struct Cli {
	#[command(subcommand)]
	command: Option<Commands>,

	#[arg(short, long, default_value_t = false)]
	debug: bool,
}

#[derive(Subcommand)]
enum Commands {
	Compile { file: Option<PathBuf> },
	Init { dir: PathBuf },
	Deps { urls: Vec<String>, dest: Option<PathBuf> },
}

fn main() {
	let cli = Cli::parse();
	match cli.command {
		Some(Commands::Init { dir }) => init_command(&dir),
		Some(Commands::Compile { file }) => compile_command(file, cli.debug),
		Some(Commands::Deps { urls, dest }) => deps_command(urls, dest),
		None => {
			eprintln!("Specify a command: compile, init, deps");
			process::exit(1);
		}
	}
}

fn compile_command(file: Option<PathBuf>, debug: bool) {
	let file = match file {
		Some(f) => f,
		None => {
			// Try to read fiber.toml for main_module_path, otherwise default to main.fib
			match fs::read_to_string("fiber.toml") {
				Ok(s) => {
					#[derive(serde::Deserialize)]
					struct Cfg {
						main_module_path: String,
					}
					if let Ok(cfg) = toml::from_str::<Cfg>(&s) {
						PathBuf::from(cfg.main_module_path)
					} else {
						PathBuf::from("main.fib")
					}
				}
				Err(_) => PathBuf::from("main.fib"),
			}
		}
	};

	match fiberc::compile_project(&file, debug) {
		Ok(()) => println!("Compilation succeeded."),
		Err(e) => {
			eprintln!("Compilation failed: {}", e);
			process::exit(1);
		}
	}
}

fn init_command(dir: &PathBuf) {
	fs::create_dir_all(&dir).unwrap_or_else(|e| {
		eprintln!("Error creating project directory: {e}");
		process::exit(1);
	});
	let config = "main_module_path = \"main.fib\"\nis_debug = true";
	fs::write(dir.join("fiber.toml"), config).unwrap();
	fs::write(dir.join("main.fib"), "// Fiber entry point\n").unwrap();
	println!("Initialized new Fiber project at {}", dir.display());
}

fn deps_command(urls: Vec<String>, dest: Option<PathBuf>) {
	let dest = dest.unwrap_or_else(|| PathBuf::from("deps"));
	fs::create_dir_all(&dest).unwrap_or_else(|e| {
		eprintln!("Failed to create deps dir: {}", e);
		process::exit(1);
	});

	for url in urls {
		match ureq::get(&url).call() {
			Ok(response) => {
				let fname = url.split('/').last().unwrap_or("file.bin");
				let path = dest.join(fname);
				let mut out = fs::File::create(&path).unwrap_or_else(|e| {
					eprintln!("Failed to create {}: {}", path.display(), e);
					process::exit(1);
				});
				let mut reader = response.into_reader();
				if let Err(e) = std::io::copy(&mut reader, &mut out) {
					eprintln!("Failed to write {}: {}", path.display(), e);
					process::exit(1);
				}
				println!("Downloaded {} -> {}", url, path.display());
			}
			Err(e) => {
				eprintln!("Failed to fetch {}: {}", url, e);
				process::exit(1);
			}
		}
	}
}
