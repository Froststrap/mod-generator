// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{error::Error as StdError, fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum Error {
    /// Bad color flag
    Color(String),
    /// Bad image-map flag
    ImageMap(String),
    /// Font couldn't be parsed or rebuilt
    Font { path: PathBuf, reason: String },
    Io { path: PathBuf, source: io::Error },
    NotADirectory(PathBuf),
    NoOutputDir,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn font(path: impl Into<PathBuf>, reason: impl fmt::Display) -> Self {
        Self::Font { path: path.into(), reason: reason.to_string() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Color(m) => write!(f, "invalid color specification: {m}"),
            Self::ImageMap(m) => write!(f, "invalid image-map entry: {m}"),
            Self::Font { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NotADirectory(p) => write!(f, "invalid directory: {}", p.display()),
            Self::NoOutputDir => f.write_str("could not determine a BuilderIcons directory"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io { path: path.into(), source })
    }
}
