// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::{
    data_types::{Bootstrapper, FontDir, derive_root, write_builder_icons_json},
    error::{Error, Result},
    font::{RecolorOptions, recolor_font},
};

#[derive(Debug)]
pub struct Summary {
    pub processed: usize,
    pub failed: usize,
}

pub fn process_directory(
    dir: &Path,
    opts: &RecolorOptions,
    bootstrapper: Bootstrapper,
    mod_name: Option<&str>,
) -> Result<Summary> {
    if !dir.is_dir() {
        return Err(Error::NotADirectory(dir.to_path_buf()));
    }

    let fonts: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ttf")))
        .collect();

    let results: Vec<Result<PathBuf>> =
        fonts.par_iter().map(|p| recolor_font(p, opts)).collect();

    let font_dir = FontDir::get(bootstrapper, mod_name);
    if font_dir.is_none() {
        eprintln!("warning: no install location for {bootstrapper} on this platform; fonts left in place");
    }

    let mut failed = 0;
    for r in &results {
        let outcome = match (r, &font_dir) {
            (Ok(out), Some(fd)) => fd.copy_font(out).map(|_| ()),
            (Ok(_), None) => Ok(()),
            (Err(e), _) => Err(Error::font(
                match e {
                    Error::Font { path, .. } | Error::Io { path, .. } => path.clone(),
                    _ => dir.to_path_buf(),
                },
                "failed",
            )),
        };
        if let (Err(original), _) = (r, &outcome) {
            eprintln!("error: {original}");
            failed += 1;
        } else if let Err(e) = outcome {
            eprintln!("error: {e}");
            failed += 1;
        }
    }

    let root = match &font_dir {
        Some(fd) => fd.root().map(Path::to_path_buf),
        None => derive_root(dir),
    };
    if let Some(root) = root {
        if let Err(e) = write_builder_icons_json(&root) {
            eprintln!("error: {e}");
            failed += 1;
        }
    }

    println!("Processed {} files ({failed} failed)", results.len());
    Ok(Summary { processed: results.len(), failed })
}
