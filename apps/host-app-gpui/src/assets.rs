use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use gpui::{AssetSource, Result, SharedString};

pub struct AppAssets {
    base: PathBuf,
}

impl AppAssets {
    pub fn new() -> Self {
        Self {
            base: resolve_assets_base(),
        }
    }
}

fn resolve_assets_base() -> PathBuf {
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("AI_MEETING_HOST_ASSETS_DIR") {
        candidates.push(PathBuf::from(path));
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("assets"));

            if exe_dir.file_name().is_some_and(|name| name == "MacOS") {
                if let Some(contents_dir) = exe_dir.parent() {
                    candidates.push(contents_dir.join("Resources").join("assets"));
                }
            }
        }
    }

    let manifest_assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    candidates.push(manifest_assets.clone());

    for candidate in candidates {
        if candidate.is_dir() {
            return candidate;
        }
    }

    manifest_assets
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match fs::read(self.base.join(path)) {
            Ok(data) => Ok(Some(Cow::Owned(data))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        match fs::read_dir(self.base.join(path)) {
            Ok(entries) => Ok(entries
                .filter_map(|entry| {
                    entry
                        .ok()
                        .and_then(|entry| entry.file_name().into_string().ok())
                        .map(SharedString::from)
                })
                .collect()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
}
