// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{collections::{HashMap, HashSet}, path::PathBuf, process::ExitCode};

use clap::Parser;
use mod_generator::{
    color::Gradient,
    data_types::Bootstrapper,
    error::Result,
    font::{RecolorOptions, parse_image_pair},
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
struct AppArgs {
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

fn run() -> Result<bool> {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    let args = AppArgs::parse();

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
