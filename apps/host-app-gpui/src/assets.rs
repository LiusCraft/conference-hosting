use std::borrow::Cow;
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
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        }
    }
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
