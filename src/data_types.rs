//! Implementing data types used across the codebase

use std::{
    fmt::{
        Formatter,
        Display
    },
    env, fs, io,
    path::{Path, PathBuf},
};

const ROOT_SUBPATH: [&str; 6] = [
    "ExtraContent", "LuaPackages", "Packages", "_Index", "BuilderIcons", "BuilderIcons",
];

const BUILDER_ICONS_JSON: &str = r#"{
    "name": "Builder Icons",
    "loadStrategy": "sameFamilyOnly",
    "faces": [
        {
            "name": "Regular",
            "weight": 400,
            "style": "normal",
            "assetId": "rbxasset://LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font/BuilderIcons-Regular.otf"
        },
        {
            "name": "Bold",
            "weight": 700,
            "style": "normal",
            "assetId": "rbxasset://LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font/BuilderIcons-Filled.otf"
        }
    ]
}"#;

#[derive(Clone, Debug, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Bootstrapper {
    Bloxstrap,
    Fishstrap,
    Froststrap,
    Luczystrap,
    Lunastrap,
    Sober,
}

impl Default for Bootstrapper {
    fn default() -> Self {
        if cfg!(target_os = "linux") { Self::Sober } else { Self::Froststrap }
    }
}

impl Display for Bootstrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        use Bootstrapper::*;
        let str = match self {
            Bloxstrap => "Bloxstrap",
            Fishstrap => "Fishstrap",
            Froststrap => "Froststrap",
            Luczystrap => "Luczystrap",
            Lunastrap => "Lunastrap",
            Sober => "Sober",
        };
        write!(
            f,
            "{str}"
        )
    }
}

#[derive(Debug)]
pub struct FontDir(PathBuf);

impl From<FontDir> for PathBuf {
    fn from(dir: FontDir) -> Self {
        dir.0
    }
}

impl FontDir {
    fn from_base(base: PathBuf) -> Self {
        let mut p = base;
        p.extend(ROOT_SUBPATH);
        p.push("Font");
        Self(p)
    }

    #[cfg(target_os = "linux")]
    pub fn get(bootstrapper: Bootstrapper, _mod_name: Option<&str>) -> Option<Self> {
        if bootstrapper != Bootstrapper::Sober {
            return None;
        }
        let base = env::home_dir()?
            .join(".var/app/org.vinegarhq.Sober/data/sober/asset_overlay");
        Some(Self::from_base(base))
    }

    #[cfg(target_os = "windows")]
    pub fn get(bootstrapper: Bootstrapper, mod_name: Option<&str>) -> Option<Self> {
        if bootstrapper == Bootstrapper::Sober {
            return None;
        }
        let mut base = PathBuf::from(env::var_os("LOCALAPPDATA")?)
            .join(bootstrapper.to_string())
            .join("Modifications");
        if let (Bootstrapper::Froststrap, Some(name)) = (bootstrapper, mod_name) {
            base.push(name);
        }
        Some(Self::from_base(base))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub fn get(_: Bootstrapper, _: Option<&str>) -> Option<Self> {
        None
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn root(&self) -> Option<&Path> {
        self.0.parent()
    }

    pub fn copy_font(&self, font: &Path) -> io::Result<PathBuf> {
        let name = font.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "font path has no file name")
        })?;
        fs::create_dir_all(&self.0)?;
        let dest = self.0.join(name);
        fs::copy(font, &dest)?;
        Ok(dest)
    }

    pub fn write_builder_icons_json(&self) -> io::Result<()> {
        let root = self.root().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "font dir has no parent")
        })?;
        fs::write(root.join("BuilderIcons.json"), BUILDER_ICONS_JSON)
    }
}
