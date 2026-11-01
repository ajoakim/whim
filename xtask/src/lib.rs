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

/// `--label` ends up in file names: allow only `[A-Za-z0-9._-]`, non-empty.
pub fn valid_label(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::valid_label;

    #[test]
    fn labels_are_filename_safe() {
        assert!(valid_label("windows-latest-glow"));
        assert!(valid_label("a.b_c-1"));
        assert!(!valid_label(""));
        assert!(!valid_label("../x"));
        assert!(!valid_label("a/b"));
        assert!(!valid_label(r"a\b"));
        assert!(!valid_label("a b"));
    }
}
