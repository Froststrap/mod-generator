// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use mod_generator::{
    color::Gradient,
    data_types::Bootstrapper,
    error::{Error, IoContext, Result},
    font::{RecolorOptions, parse_image_pair},
    generate::{DEFAULT_DOMAIN, GenerateConfig, Progress, Stage, default_cache_dir, generate, zip_directory},
    process::process_directory,
};

struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        match record.level() {
            log::Level::Info if io::stdout().is_terminal() => {
                println!("\r\x1b[2K{}", record.args())
            }
            log::Level::Info => println!("{}", record.args()),
            level => eprintln!("{}: {}", level.as_str().to_lowercase(), record.args()),
        }
    }

    fn flush(&self) {}
}

static LOGGER: ConsoleLogger = ConsoleLogger;

#[derive(Parser, Debug)]
#[command(name = "mod_generator")]
#[command(version = "3.0")]
#[command(about = "Generates mods", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate a complete mod: download Roblox's assets and recolor textures, sprite sheets and fonts
    Generate(GenerateArgs),
    /// Recolor the BuilderIcons fonts in a directory
    Fonts(FontArgs),
}

#[derive(Args, Debug)]
struct GenerateArgs {
    /// Directory to write the mod to
    #[arg(long, default_value = "mod")]
    output: PathBuf,
    /// Also zip the generated mod to this path
    #[arg(long)]
    zip: Option<PathBuf>,
    /// Hex color, a comma-separated list, or offset:hex stops
    #[arg(long)]
    color: String,
    #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
    angle: f32,
    #[arg(long)]
    bands: Option<u16>,
    #[arg(long)]
    cursors: bool,
    #[arg(long)]
    shiftlock: bool,
    #[arg(long)]
    emote_wheel: bool,
    /// Image to replace the Roblox logo sprite with
    #[arg(long)]
    logo: Option<PathBuf>,
    /// Image to replace the loading spinner sprite with
    #[arg(long)]
    spinner: Option<PathBuf>,
    #[arg(long, value_delimiter = ',', value_parser = parse_image_pair)]
    image_map: Vec<(String, PathBuf)>,
    #[arg(long, value_delimiter = ',')]
    skip_glyphs: Vec<String>,
    /// JSON file of texture mappings to use instead of the built-in set
    #[arg(long)]
    mappings: Option<PathBuf>,
    /// Directory for downloaded packages and cached fonts
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    #[arg(long, default_value = DEFAULT_DOMAIN)]
    domain: String,
}

#[derive(Args, Debug)]
struct FontArgs {
    #[arg(long)]
    path: PathBuf,
    #[arg(long)]
    color: Gradient,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    angle: i32,
    #[arg(long)]
    bands: Option<u16>,
    #[arg(long, value_enum, ignore_case = true, default_value_t = Bootstrapper::default())]
    bootstrapper: Bootstrapper,
    #[arg(long)]
    mod_name: Option<String>,
    #[arg(long, value_delimiter = ',', value_parser = parse_image_pair)]
    image_map: Vec<(String, PathBuf)>,
    #[arg(long, value_delimiter = ',')]
    skip_glyphs: Vec<String>,
    #[arg(long)]
    skip_color_matching: bool,
    #[arg(long, default_value_t = 64)]
    max_colors: u8,
}

fn run_generate(args: GenerateArgs) -> Result<bool> {
    let mappings = match &args.mappings {
        Some(path) => {
            let text = fs::read_to_string(path).at(path)?;
            Some(serde_json::from_str(&text).map_err(|e| Error::Config(format!("{}: {e}", path.display())))?)
        }
        None => None,
    };
    let cache_dir = args
        .cache_dir
        .or_else(default_cache_dir)
        .ok_or_else(|| Error::Config("couldn't find a cache directory, pass --cache-dir".into()))?;

    let config = GenerateConfig {
        output: args.output,
        cache_dir,
        domain: args.domain,
        color: args.color,
        angle: args.angle,
        bands: args.bands,
        image_map: args.image_map.into_iter().collect(),
        skip_glyphs: args.skip_glyphs,
        cursors: args.cursors,
        shiftlock: args.shiftlock,
        emote_wheel: args.emote_wheel,
        custom_logo: args.logo,
        custom_spinner: args.spinner,
        mappings,
        generator_version: None,
    };

    let current = std::sync::Mutex::new(None);
    let generated = generate(&config, &|p: Progress| {
        let name = match p.stage {
            Stage::Downloading => "Downloading Roblox assets",
            Stage::Extracting => "Extracting",
            Stage::Recoloring => "Recoloring",
            Stage::Finishing => "Writing manifests",
        };
        let mut current = current.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *current != Some(p.stage);
        *current = Some(p.stage);
        if !io::stdout().is_terminal() {
            if changed {
                println!("{name}...");
            }
            return;
        }
        let detail = match (p.stage, p.total) {
            (_, 0) => String::new(),
            (Stage::Downloading, total) => format!(
                " {:.1}/{:.1} MB",
                p.done as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            ),
            (_, total) => format!(" {}/{total}", p.done),
        };
        print!("\r\x1b[2K{name}...{detail}");
        let _ = io::stdout().flush();
    })?;
    if io::stdout().is_terminal() {
        println!();
    }
    println!("Generated mod for Roblox {} in {}", generated.roblox_version, config.output.display());

    if let Some(zip) = &args.zip {
        zip_directory(&config.output, zip)?;
        println!("Zipped to {}", zip.display());
    }
    Ok(true)
}

fn run_fonts(args: FontArgs) -> Result<bool> {
    if !args.image_map.is_empty() {
        let names: Vec<&str> = args.image_map.iter().map(|(g, _)| g.as_str()).collect();
        println!("Loaded image map for glyphs: {}", names.join(", "));
    }
    if !args.skip_glyphs.is_empty() {
        println!("Will skip coloring these glyphs: {}", args.skip_glyphs.join(", "));
    }

    let opts = RecolorOptions {
        gradient: args.color,
        angle: args.angle,
        bands: args.bands,
        image_map: args.image_map.into_iter().collect::<HashMap<_, _>>(),
        skip_glyphs: args.skip_glyphs.into_iter().collect::<HashSet<_>>(),
        skip_color_matching: args.skip_color_matching,
        max_colors: args.max_colors,
    };

    let summary = process_directory(&args.path, &opts, args.bootstrapper, args.mod_name.as_deref())?;
    println!("Processed {} files ({} failed)", summary.processed, summary.failed);
    Ok(summary.failed == 0)
}

fn run() -> Result<bool> {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    match Cli::parse().command {
        Command::Generate(args) => run_generate(args),
        Command::Fonts(args) => run_fonts(args),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
