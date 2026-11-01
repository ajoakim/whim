//! Read-only open + mmap. Nothing is read, populated or advised.

use std::path::{Path, PathBuf};

pub struct MappedFile {
    pub path: PathBuf,
    pub len: u64,
    map: Option<memmap2::Mmap>,
}

impl MappedFile {
    pub fn data(&self) -> &[u8] {
        self.map.as_deref().unwrap_or(&[])
    }

    pub fn file_name(&self) -> String {
        display_name(&self.path)
    }
}

#[derive(Debug)]
pub enum FileError {
    Open { name: String, reason: String },
    Map { name: String, reason: String },
}

impl FileError {
    pub fn message(&self) -> String {
        match self {
            FileError::Open { name, reason } => format!(
                "Couldn't open {name}: {reason}. Check the path and permissions, then try again."
            ),
            FileError::Map { name, reason } => format!(
                "Couldn't map {name}: {reason}. The file may be locked by another program."
            ),
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn open_mapped(path: &Path) -> Result<MappedFile, FileError> {
    let name = display_name(path);
    let open_err = |e: std::io::Error| FileError::Open { name: name.clone(), reason: e.to_string() };
    let file = std::fs::File::open(path).map_err(open_err)?;
    let len = file.metadata().map_err(open_err)?.len();
    let map = if len == 0 { None } else { Some(map_readonly(&file, &name)?) };
    Ok(MappedFile { path: path.to_path_buf(), len, map })
}

fn map_readonly(file: &std::fs::File, name: &str) -> Result<memmap2::Mmap, FileError> {
    // SAFETY: read-only mapping; external truncation is a documented Phase 2 risk.
    unsafe { memmap2::Mmap::map(file) }
        .map_err(|e| FileError::Map { name: name.to_string(), reason: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("whim-test-{}-{name}", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn missing_file_message() {
        let e = open_mapped(Path::new("definitely/missing/nope.txt")).err().unwrap();
        let m = e.message();
        assert!(m.starts_with("Couldn't open "));
        assert!(m.ends_with(". Check the path and permissions, then try again."));
    }

    #[test]
    fn empty_file() {
        let p = temp("empty", b"");
        let f = open_mapped(&p).unwrap();
        assert_eq!(f.len, 0);
        assert_eq!(f.data(), &[] as &[u8]);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn small_file() {
        let p = temp("small", b"abc");
        let f = open_mapped(&p).unwrap();
        assert_eq!(f.len, 3);
        assert_eq!(f.data(), b"abc");
        drop(f);
        let _ = std::fs::remove_file(p);
    }
}
