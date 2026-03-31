use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{fs, path::PathBuf, process};
use tera::{Context, Tera};

use fibc::{self, driver::CompilationOptions};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const CONFIG_PATH: &str = "fiber.toml";
const SRC_ROOT: &str = "src";
const MAIN_ENTRY: &str = "main.fib";
const CONFIG_TEMPLATE_STR: &str = include_str!("../resources/fiber.toml.template");
const SOURCE_TEMPLATE_STR: &str = include_str!("../resources/main.fib.template");

#[derive(Parser)]
#[command(name = "fiber", about = "Fiber project CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Deserialize)]
struct Cfg {
    package: PackageConfig,
}

#[derive(Debug, Deserialize)]
struct PackageConfig {
    name: String,
    version: String,
    src_root: String,
    main_module: String,
    include_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DefaultConfigs {
    src_root: String,
    main_module: String,
    out_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    Compile {
        file: Option<PathBuf>,
        /// Additional include directories to search when resolving imports (repeatable: -I path)
        #[arg(short = 'I', long = "include", value_name = "PATH")]
        include: Vec<PathBuf>,
    },
    Init {
        dir: PathBuf,
    },
    Deps {
        urls: Vec<String>,
        dest: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let command_result = match cli.command {
        Some(Commands::Init { dir }) => init_command(dir),
        Some(Commands::Compile { file, include }) => compile_command(file, include),
        Some(Commands::Deps { urls, dest }) => deps_command(urls, dest),
        None => {
            eprintln!("Specify a command: compile, init, deps");
            process::exit(1);
        }
    };
    match command_result {
        Ok(_) => {}
        Err(e) => eprintln!("Command failed: \n{}", e),
    }
}

fn compile_command(
    file: Option<PathBuf>,
    cli_includes: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config: Cfg = toml::from_str::<Cfg>(&fs::read_to_string(CONFIG_PATH)?)?;
    let file = match file {
        Some(f) => f,
        None => PathBuf::from(format!(
            "{}/{}",
            config.package.src_root, config.package.main_module
        )),
    };

    // Merge include paths from fiber.toml and CLI flags
    let mut include_paths: Vec<PathBuf> = config
        .package
        .include_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    include_paths.extend(cli_includes);

    let include_refs: Vec<&std::path::Path> = include_paths.iter().map(|p| p.as_path()).collect();

    println!(
        "Compiling project {} with fiber version {}",
        config.package.name, config.package.version
    );
    println!("Include paths: {:#?}", include_refs);

    let compilation_options = CompilationOptions {
        project_path: file,
        source: None,
        include_paths,
    };
    fibc::compile_project(compilation_options)
}

fn init_command(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join("src"))?;

    // get the directory name, if dir is '.' get the current directory name
    let abs_path = dir.canonicalize()?;
    let package_name = abs_path.to_str().unwrap().split('/').last().unwrap();

    let config_template = CONFIG_TEMPLATE_STR;
    let mut tera = Tera::default();
    let mut context = Context::new();

    tera.add_raw_template("default_config", config_template)?;

    context.insert("package_name", &package_name);
    context.insert("language_version", VERSION);

    let config = tera.render("default_config", &context)?;

    fs::write(dir.join(CONFIG_PATH), config)?;

    let source_template = SOURCE_TEMPLATE_STR;

    fs::write(dir.join(SRC_ROOT).join(MAIN_ENTRY), source_template)?;

    Ok(())
}

fn deps_command(
    urls: Vec<String>,
    dest: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}
