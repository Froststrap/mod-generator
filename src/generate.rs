// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

//! The full mod generation pipeline: download, extract, recolor, write manifests.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    hash::Hasher,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use walkdir::WalkDir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    color::Gradient,
    data_types::write_builder_icons_json,
    error::{Error, IoContext, Result},
    font::RecolorOptions,
    process::recolor_directory,
    texture::{Fill, recolor_images, recolor_sprite_sheets},
};

pub const DEFAULT_DOMAIN: &str = "roblox.com";

const PACKAGE_CDN: &str = "https://setup.rbxcdn.com";
const EMBEDDED_MAPPINGS: &str = include_str!("../assets/mappings.json");
const FONT_CACHE_LIMIT: usize = 16;
const DEFAULT_MAX_COLORS: u8 = 64;

const BUILDER_ICONS: &str = "ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons";
const FONT_DIR: &str = "ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font";
const FOUNDATION_IMAGES: &str =
    "ExtraContent/LuaPackages/Packages/_Index/FoundationImages/FoundationImages";
const SPRITE_SHEETS: &str =
    "ExtraContent/LuaPackages/Packages/_Index/FoundationImages/FoundationImages/SpriteSheets";
const IMAGE_SET_DATA: &str = "ExtraContent/LuaPackages/Packages/_Index/FoundationImages/FoundationImages/Generated/GetImageSetData.lua";

const CURSOR_FILES: [&str; 3] = [
    "content/textures/Cursors/KeyboardMouse/IBeamCursor.png",
    "content/textures/Cursors/KeyboardMouse/ArrowCursor.png",
    "content/textures/Cursors/KeyboardMouse/ArrowFarCursor.png",
];
const SHIFTLOCK_FILES: [&str; 1] = ["content/textures/MouseLockedCursor.png"];
const EMOTE_WHEEL_FILES: [&str; 6] = [
    "content/textures/ui/Emotes/Large/SelectedGradient.png",
    "content/textures/ui/Emotes/Large/SelectedGradient@2x.png",
    "content/textures/ui/Emotes/Large/SelectedGradient@3x.png",
    "content/textures/ui/Emotes/Large/SelectedLine.png",
    "content/textures/ui/Emotes/Large/SelectedLine@2x.png",
    "content/textures/ui/Emotes/Large/SelectedLine@3x.png",
];

/// Roblox packages the generator needs, and where each one is extracted to.
const PACKAGES: [(&str, &str); 3] = [
    ("extracontent-luapackages", "ExtraContent/LuaPackages"),
    ("extracontent-textures", "ExtraContent/textures"),
    ("content-textures2", "content/textures"),
];

/// Top-level entries the generator owns inside the output directory.
const OUTPUT_ENTRIES: [&str; 3] = ["ExtraContent", "content", "info.json"];

pub type Mappings = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Stage {
    Downloading = 1,
    Extracting = 2,
    Recoloring = 3,
    Finishing = 4,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateConfig {
    /// Directory the mod is written to.
    pub output: PathBuf,
    /// Directory for downloaded packages and cached fonts.
    pub cache_dir: PathBuf,
    #[serde(default = "default_domain")]
    pub domain: String,
    /// `RRGGBB[,RRGGBB...]` or `offset:RRGGBB[,offset:RRGGBB...]`.
    pub color: String,
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub bands: Option<u16>,
    #[serde(default)]
    pub image_map: HashMap<String, PathBuf>,
    #[serde(default)]
    pub skip_glyphs: Vec<String>,
    #[serde(default)]
    pub cursors: bool,
    #[serde(default)]
    pub shiftlock: bool,
    #[serde(default)]
    pub emote_wheel: bool,
    #[serde(default)]
    pub custom_logo: Option<PathBuf>,
    #[serde(default)]
    pub custom_spinner: Option<PathBuf>,
    /// Texture mappings to recolor; the embedded set is used when absent.
    #[serde(default)]
    pub mappings: Option<Mappings>,
    /// Recorded as `FroststrapVersion` in `info.json`.
    #[serde(default)]
    pub generator_version: Option<String>,
}

fn default_domain() -> String {
    DEFAULT_DOMAIN.to_string()
}

#[derive(Debug, Clone)]
pub struct Generated {
    pub roblox_version: String,
    pub version_hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientVersion {
    version: String,
    client_version_upload: String,
}

pub fn embedded_mappings() -> Mappings {
    serde_json::from_str(EMBEDDED_MAPPINGS).expect("embedded mappings are valid JSON")
}

/// Platform cache directory for the standalone CLI.
pub fn default_cache_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        env::home_dir()?.join("Library/Caches")
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::home_dir().map(|h| h.join(".cache")))?
    };
    Some(base.join("mod-generator"))
}

fn join_rel(root: &Path, rel: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    path.extend(rel.split('/'));
    path
}

fn is_safe_component(c: &str) -> bool {
    !c.is_empty() && c != "." && c != ".." && !c.contains(['/', '\\', ':'])
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into()
}

fn client_version(agent: &ureq::Agent, domain: &str) -> Result<ClientVersion> {
    if !domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err(Error::Config(format!("invalid domain '{domain}'")));
    }
    let url = format!("https://clientsettingscdn.{domain}/v2/client-version/WindowsStudio64");
    let fail = |reason: String| Error::Download {
        url: url.clone(),
        reason,
    };
    let body = agent
        .get(&url)
        .call()
        .map_err(|e| fail(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| fail(e.to_string()))?;
    serde_json::from_str(&body).map_err(|e| fail(e.to_string()))
}

fn download_package(
    agent: &ureq::Agent,
    cache_dir: &Path,
    hash: &str,
    kind: &str,
) -> Result<PathBuf> {
    let path = cache_dir.join(format!("{kind}-{hash}.zip"));
    if fs::metadata(&path).is_ok_and(|m| m.len() > 0) {
        return Ok(path);
    }

    let url = format!("{PACKAGE_CDN}/version-{hash}-{kind}.zip");
    let fail = |reason: String| Error::Download {
        url: url.clone(),
        reason,
    };
    let mut response = agent.get(&url).call().map_err(|e| fail(e.to_string()))?;

    let part = path.with_extension("zip.part");
    let mut file = fs::File::create(&part).at(&part)?;
    io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(|e| fail(e.to_string()))?;
    drop(file);
    fs::rename(&part, &path).at(&path)?;

    log::info!("Downloaded {kind}");
    Ok(path)
}

fn prune_packages(cache_dir: &Path, hash: &str) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".zip") && !name.contains(hash) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Files and directory prefixes (lowercase, `/`-separated) to pull out of the packages.
struct Wanted {
    files: HashSet<String>,
    prefixes: Vec<String>,
}

impl Wanted {
    fn contains(&self, rel: &str) -> bool {
        let rel = rel.to_lowercase();
        self.files.contains(&rel) || self.prefixes.iter().any(|p| rel.starts_with(p))
    }
}

fn wanted_files(cfg: &GenerateConfig, mappings: &Mappings) -> Result<(Wanted, Vec<String>)> {
    let mut images: Vec<String> = Vec::with_capacity(mappings.len());
    for (name, parts) in mappings {
        if parts.is_empty() || !parts.iter().all(|p| is_safe_component(p)) {
            return Err(Error::Config(format!("invalid mapping path for '{name}'")));
        }
        images.push(parts.join("/"));
    }

    let optional = [
        (cfg.cursors, &CURSOR_FILES[..]),
        (cfg.shiftlock, &SHIFTLOCK_FILES[..]),
        (cfg.emote_wheel, &EMOTE_WHEEL_FILES[..]),
    ];
    for (enabled, files) in optional {
        if enabled {
            images.extend(files.iter().map(|f| f.to_string()));
        }
    }

    let mut files: HashSet<String> = images.iter().map(|f| f.to_lowercase()).collect();
    files.insert(IMAGE_SET_DATA.to_lowercase());
    images.sort_unstable();
    images.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let prefixes = [FONT_DIR, SPRITE_SHEETS]
        .iter()
        .map(|p| format!("{}/", p.to_lowercase()))
        .collect();
    Ok((Wanted { files, prefixes }, images))
}

fn extract_package(zip_path: &Path, prefix: &str, output: &Path, wanted: &Wanted) -> Result<usize> {
    let archive_err = |e: zip::result::ZipError| Error::Archive {
        path: zip_path.to_path_buf(),
        reason: e.to_string(),
    };
    let file = fs::File::open(zip_path).at(zip_path)?;
    let mut archive = ZipArchive::new(io::BufReader::new(file)).map_err(archive_err)?;

    let mut extracted = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(archive_err)?;
        let name = entry.name().map_err(archive_err)?.replace('\\', "/");
        let name = name.trim_start_matches('/');
        if name.is_empty() || name.ends_with('/') || !name.split('/').all(is_safe_component) {
            continue;
        }

        let rel = format!("{prefix}/{name}");
        if !wanted.contains(&rel) {
            continue;
        }

        let dest = join_rel(output, &rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).at(parent)?;
        }
        let mut out = fs::File::create(&dest).at(&dest)?;
        io::copy(&mut entry, &mut out).at(&dest)?;
        extracted += 1;
    }
    Ok(extracted)
}

fn clear_output(output: &Path) -> Result<()> {
    for entry in OUTPUT_ENTRIES {
        let path = output.join(entry);
        if path.is_dir() {
            fs::remove_dir_all(&path).at(&path)?;
        } else if path.exists() {
            fs::remove_file(&path).at(&path)?;
        }
    }
    fs::create_dir_all(output).at(output)
}

fn first_failure<T>(results: Vec<Result<T>>, what: &str) -> Result<()> {
    let total = results.len();
    let errors: Vec<Error> = results.into_iter().filter_map(|r| r.err()).collect();
    for e in &errors {
        log::error!("{e}");
    }
    match errors.len() {
        0 => Ok(()),
        n => Err(Error::Failed(format!(
            "failed to recolor {n} of {total} {what} ({})",
            errors[0]
        ))),
    }
}

fn recolor_textures(cfg: &GenerateConfig, fill: &Fill, images: &[String]) -> Result<()> {
    let paths: Vec<PathBuf> = images
        .iter()
        .map(|rel| join_rel(&cfg.output, rel))
        .filter(|p| p.is_file())
        .collect();
    first_failure(recolor_images(&paths, fill), "images")?;
    log::info!("Recolored {} images", paths.len());

    let lua = join_rel(&cfg.output, IMAGE_SET_DATA);
    if lua.is_file() {
        let results = recolor_sprite_sheets(
            &lua,
            fill,
            cfg.custom_logo.as_deref(),
            cfg.custom_spinner.as_deref(),
        )?;
        first_failure(results, "sprite sheets")?;
    }
    Ok(())
}

/// FNV-1a, so cache keys stay stable across Rust versions.
struct Fnv(u64);

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

fn font_cache_key(hash: &str, opts: &RecolorOptions) -> String {
    let mut h = Fnv(0xcbf29ce484222325);
    h.write(env!("CARGO_PKG_VERSION").as_bytes());
    for stop in &opts.gradient.0 {
        h.write(&stop.offset.to_le_bytes());
        h.write(&stop.rgb);
    }
    h.write(&opts.angle.to_le_bytes());
    h.write(&opts.n_bands().to_le_bytes());

    let mut skip: Vec<&String> = opts.skip_glyphs.iter().collect();
    skip.sort();
    for glyph in skip {
        h.write(glyph.as_bytes());
        h.write(&[0]);
    }

    let mut map: Vec<_> = opts.image_map.iter().collect();
    map.sort();
    for (glyph, path) in map {
        h.write(glyph.as_bytes());
        h.write(&[0]);
        if let Ok(bytes) = fs::read(path) {
            h.write(&bytes);
        }
    }
    format!("{hash}-{:016x}", h.finish())
}

fn otf_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("otf")))
                .collect()
        })
        .unwrap_or_default()
}

fn prune_font_cache(fonts_cache: &Path, hash: &str) {
    let Ok(entries) = fs::read_dir(fonts_cache) else {
        return;
    };
    let mut current = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&format!("{hash}-")) {
            let _ = fs::remove_dir_all(entry.path());
        } else if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            current.push((modified, entry.path()));
        }
    }
    current.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in current.into_iter().skip(FONT_CACHE_LIMIT) {
        let _ = fs::remove_dir_all(path);
    }
}

fn recolor_fonts(cfg: &GenerateConfig, opts: &RecolorOptions, hash: &str) -> Result<()> {
    let font_dir = join_rel(&cfg.output, FONT_DIR);
    if !font_dir.is_dir() {
        return Ok(());
    }

    let fonts_cache = cfg.cache_dir.join("fonts");
    let cached = fonts_cache.join(font_cache_key(hash, opts));
    let cached_fonts = otf_files(&cached);
    if !cached_fonts.is_empty() {
        for font in &cached_fonts {
            let dest = font_dir.join(font.file_name().unwrap_or_default());
            fs::copy(font, &dest).at(&dest)?;
        }
        log::info!("Reused {} cached fonts", cached_fonts.len());
        return Ok(());
    }

    let results = recolor_directory(&font_dir, opts)?;
    let fonts: Vec<PathBuf> = results
        .iter()
        .filter_map(|r| r.as_ref().ok().cloned())
        .collect();
    first_failure(results, "fonts")?;
    log::info!("Recolored {} fonts", fonts.len());

    let staging = cached.with_extension("tmp");
    let store = || -> io::Result<()> {
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging)?;
        for font in &fonts {
            fs::copy(font, staging.join(font.file_name().unwrap_or_default()))?;
        }
        fs::rename(&staging, &cached)
    };
    if let Err(e) = store() {
        log::warn!("Couldn't cache fonts: {e}");
        let _ = fs::remove_dir_all(&staging);
    }
    prune_font_cache(&fonts_cache, hash);
    Ok(())
}

fn colors_used(gradient: &Gradient, angle: f32) -> Value {
    let hex = |rgb: [u8; 3]| format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    match gradient.0.as_slice() {
        [only] => json!({ "SolidColor": hex(only.rgb) }),
        stops => json!({
            "GradientStops": stops
                .iter()
                .map(|s| json!({ "Offset": s.offset, "Color": hex(s.rgb) }))
                .collect::<Vec<_>>(),
            "Angle": angle,
        }),
    }
}

fn finish(
    cfg: &GenerateConfig,
    gradient: &Gradient,
    version: &ClientVersion,
    hash: &str,
) -> Result<()> {
    write_builder_icons_json(&join_rel(&cfg.output, BUILDER_ICONS))?;

    let lua = join_rel(&cfg.output, IMAGE_SET_DATA);
    let _ = fs::remove_file(&lua);
    if let Some(generated) = lua.parent() {
        let _ = fs::remove_dir(generated);
    }
    let _ = fs::remove_dir(join_rel(&cfg.output, FOUNDATION_IMAGES).join("SpriteSheets"));

    let info = json!({
        "FroststrapVersion": cfg.generator_version.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
        "RobloxVersion": version.version,
        "RobloxVersionHash": hash,
        "ColorsUsed": colors_used(gradient, cfg.angle),
    });
    let path = cfg.output.join("info.json");
    let text = serde_json::to_string_pretty(&info).map_err(|e| Error::Config(e.to_string()))?;
    fs::write(&path, text).at(path)
}

/// Generates a complete mod into `cfg.output`.
pub fn generate(cfg: &GenerateConfig, progress: &(dyn Fn(Stage) + Sync)) -> Result<Generated> {
    let gradient: Gradient = cfg.color.parse()?;
    let fill = Fill::new(&gradient, cfg.angle);
    let font_opts = RecolorOptions {
        gradient: gradient.clone(),
        angle: cfg.angle.round() as i32,
        bands: cfg.bands.filter(|b| *b > 0),
        image_map: cfg.image_map.clone(),
        skip_glyphs: cfg.skip_glyphs.iter().cloned().collect(),
        skip_color_matching: false,
        max_colors: DEFAULT_MAX_COLORS,
    };
    let mappings = match &cfg.mappings {
        Some(m) if !m.is_empty() => m.clone(),
        _ => embedded_mappings(),
    };
    let (wanted, images) = wanted_files(cfg, &mappings)?;

    progress(Stage::Downloading);
    fs::create_dir_all(&cfg.cache_dir).at(&cfg.cache_dir)?;
    let agent = agent();
    let version = client_version(&agent, &cfg.domain)?;
    let hash = version
        .client_version_upload
        .trim_start_matches("version-")
        .to_string();
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::Config(format!(
            "unexpected client version '{}'",
            version.client_version_upload
        )));
    }
    let zips: Vec<PathBuf> = PACKAGES
        .par_iter()
        .map(|(kind, _)| download_package(&agent, &cfg.cache_dir, &hash, kind))
        .collect::<Result<_>>()?;
    prune_packages(&cfg.cache_dir, &hash);

    progress(Stage::Extracting);
    clear_output(&cfg.output)?;
    let extracted: usize = zips
        .par_iter()
        .zip(PACKAGES.par_iter())
        .map(|(zip, (_, prefix))| extract_package(zip, prefix, &cfg.output, &wanted))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum();
    log::info!("Extracted {extracted} files for Roblox {}", version.version);

    progress(Stage::Recoloring);
    let (textures, fonts) = rayon::join(
        || recolor_textures(cfg, &fill, &images),
        || recolor_fonts(cfg, &font_opts, &hash),
    );
    textures?;
    fonts?;

    progress(Stage::Finishing);
    finish(cfg, &gradient, &version, &hash)?;

    Ok(Generated {
        roblox_version: version.version,
        version_hash: hash,
    })
}

/// Zips the contents of `source` into `destination`.
pub fn zip_directory(source: &Path, destination: &Path) -> Result<()> {
    let archive_err = |e: zip::result::ZipError| Error::Archive {
        path: destination.to_path_buf(),
        reason: e.to_string(),
    };
    let part = destination.with_extension("zip.part");
    let file = fs::File::create(&part).at(&part)?;
    let mut writer = ZipWriter::new(io::BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(source)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(source).unwrap_or(entry.path());
        let name = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        writer.start_file(name, options).map_err(archive_err)?;
        let mut data = Vec::new();
        fs::File::open(entry.path())
            .and_then(|mut f| f.read_to_end(&mut data))
            .at(entry.path())?;
        io::Write::write_all(&mut writer, &data).at(destination)?;
    }

    writer.finish().map_err(archive_err)?;
    fs::rename(&part, destination).at(destination)
}
