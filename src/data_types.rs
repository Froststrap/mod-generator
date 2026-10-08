//! Implementing data types used across the codebase

use std::{
    env,
    path::PathBuf,
    fmt::{
        Formatter,
        Display
    },
};

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

impl Into<PathBuf> for FontDir {
    fn into(self) -> PathBuf {
        self.0
    }
}

impl FontDir {
    // #[cfg(target_os = "linux")]
    pub fn get(
        bootstrapper: Bootstrapper,
        _mod_name: Option<String>, 
    ) -> Option<Self> {
        // TODO: Right now just mirroring old code
        // this in Rust is pretty stupid
        if !(bootstrapper == Bootstrapper::Sober) {
            return None;
        }

        Some(Self(
            (env::home_dir()?)
                .join(".var")
                .join("app")
                .join("org.vinegarhq.Sober")
                .join("data")
                .join("asset_overlay")
                .join("ExtraContent")
                .join("LuaPackages")
                .join("Packages")
                .join("_Index")
                .join("BuilderIcons")
                .join("BuilderIcons")
                .join("Fonts")
        ))
    }
    #[cfg(target_os = "windows")]
    pub fn get(
        bootstrapper: Bootstrapper,
        mod_name: Option<String>, 
    ) -> Option<Self> {
        let Some(lad) = env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
            return None;
        };

        let base_path = if bootstrapper == Bootstrapper::Froststrap && mod_name.is_some() {
            lad.join(bootstrapper.to_string()).join("Modifications").join(mod_name.unwrap())
        } else {
            lad.join(bootstrapper.to_string()).join("Modifications")
        };

        Some(Self(
            base_path
                .join("ExtraContent")
                .join("LuaPackages")
                .join("Packages")
                .join("_Index")
                .join("BuilderIcons")
                .join("BuilderIcons")
                .join("Font")
        ))
    }
}
