//! Whim build/measurement tasks.
pub mod measure;
pub mod size;
pub mod sparse;

use std::path::{Path, PathBuf};

/// Workspace root (parent of the xtask crate).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

/// Default release binary path.
pub fn default_bin() -> PathBuf {
    workspace_root()
        .join("target")
        .join("release")
        .join(format!("whim{}", std::env::consts::EXE_SUFFIX))
}

/// Output directory for measurement JSON (created on demand).
pub fn measure_dir() -> PathBuf {
    let d = workspace_root().join("target").join("measure");
    let _ = std::fs::create_dir_all(&d);
    d
}
