//! Cross-OS sparse file creation and on-disk size check.
use std::fs::OpenOptions;
use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Maximum acceptable on-disk size for a "sparse" file.
pub const MAX_ON_DISK: u64 = 100_000_000;

pub fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, suf) = s.split_at(split);
    let n: u64 = num.parse().map_err(|_| format!("invalid size: {s}"))?;
    let mult: u64 = match suf.trim() {
        "" => 1,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        "TiB" => 1 << 40,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        other => return Err(format!("unknown size suffix: {other}")),
    };
    let v = n
        .checked_mul(mult)
        .ok_or_else(|| "size overflow".to_string())?;
    if v == 0 {
        return Err("size must be greater than zero".into());
    }
    Ok(v)
}

#[cfg(windows)]
fn set_sparse(f: &std::fs::File, path: &Path) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    let mut ret = 0u32;
    // SAFETY: valid open handle; no input/output buffers are used by FSCTL_SET_SPARSE.
    let ok = unsafe {
        DeviceIoControl(
            f.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut ret,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let e = Error::last_os_error();
        return Err(Error::new(
            e.kind(),
            format!(
                "FSCTL_SET_SPARSE failed for {} ({e}); the volume must be NTFS/ReFS (exFAT/FAT32 unsupported)",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_sparse(_f: &std::fs::File, _path: &Path) -> std::io::Result<()> {
    Ok(())
}

const MIDDLE: &[u8] = b"WHIM-MIDDLE
";
const END: &[u8] = b"WHIM-END
";
/// The end marker starts this many bytes before EOF.
const END_BACK: u64 = 16;

fn head_bytes() -> String {
    let mut head = String::from(
        "WHIM-START
",
    );
    for i in 0..40 {
        head.push_str(&format!(
            "line {i:02}: whim sparse test file
"
        ));
    }
    head
}

/// Marker offsets `(mid_off, end_off)` for a file of `len` bytes, or an error when the
/// markers would not fit or would overlap each other.
fn marker_layout(len: u64, head_len: u64) -> Result<(u64, u64), String> {
    let mid = len / 2;
    let end = len.saturating_sub(END_BACK);
    if len < END_BACK || mid < head_len || mid + MIDDLE.len() as u64 > end {
        return Err(format!(
            "size {len} is too small for the marker layout (head {head_len} B, middle, end)"
        ));
    }
    if end + END.len() as u64 > len {
        return Err("end marker does not fit".into());
    }
    Ok((mid, end))
}

/// Write `data` at `off`; errors (never silently skips) if it would run past `len`.
fn write_at(f: &mut std::fs::File, len: u64, off: u64, data: &[u8]) -> std::io::Result<()> {
    if off.checked_add(data.len() as u64).is_none_or(|e| e > len) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("marker at {off} does not fit in a {len}-byte file"),
        ));
    }
    f.seek(SeekFrom::Start(off))?;
    f.write_all(data)
}

pub fn create(path: &Path, len: u64, force: bool) -> std::io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true);
    if force {
        opts.create(true).truncate(true);
    } else {
        // Atomic exclusive create: fails if the path exists (also if it is a symlink).
        opts.create_new(true);
    }
    let mut f = opts.open(path).map_err(|e| {
        if e.kind() == ErrorKind::AlreadyExists {
            Error::new(
                ErrorKind::AlreadyExists,
                format!("{} already exists (use --force)", path.display()),
            )
        } else {
            e
        }
    })?;
    // Must precede set_len/writes, or NTFS zero-fills the gap.
    set_sparse(&f, path)?;
    f.set_len(len)?;
    let head = head_bytes();
    let (mid, end) = marker_layout(len, head.len() as u64)
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
    write_at(&mut f, len, 0, head.as_bytes())?;
    write_at(&mut f, len, mid, MIDDLE)?;
    write_at(&mut f, len, end, END)?;
    f.flush()?;
    Ok(())
}

#[cfg(windows)]
pub fn on_disk_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high = 0u32;
    // SAFETY: wide is NUL-terminated; high is a valid out pointer.
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    if low == u32::MAX && unsafe { GetLastError() } != 0 {
        return Err(Error::last_os_error());
    }
    Ok(((high as u64) << 32) | low as u64)
}

#[cfg(unix)]
pub fn on_disk_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.blocks() * 512)
}

const HELP: &str = "cargo xtask sparse --size 20GiB [--force] <path>\n  Suffixes: KiB/MiB/GiB/TiB (1024^n), KB/MB/GB (1000^n). Refuses to overwrite without --force.";

fn parse_args(args: &mut lexopt::Parser) -> Result<Option<(u64, bool, PathBuf)>, String> {
    use lexopt::prelude::*;
    let (mut size, mut force, mut path) = (None, false, None);
    while let Some(a) = args.next().map_err(|e| e.to_string())? {
        match a {
            Long("size") => {
                let v = args.value().map_err(|e| e.to_string())?;
                size = Some(parse_size(&v.string().map_err(|e| e.to_string())?)?);
            }
            Long("force") => force = true,
            Long("help") | Short('h') => return Ok(None),
            Value(v) => {
                if path.is_some() {
                    return Err("only one <path> is allowed".into());
                }
                path = Some(PathBuf::from(v));
            }
            other => return Err(other.unexpected().to_string()),
        }
    }
    match (size, path) {
        (Some(s), Some(p)) => Ok(Some((s, force, p))),
        _ => Err("--size and <path> are required".into()),
    }
}

pub fn run(mut args: lexopt::Parser) -> ExitCode {
    let (len, force, path) = match parse_args(&mut args) {
        Ok(Some(v)) => v,
        Ok(None) => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n{HELP}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = create(&path, len, force) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    let apparent = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let disk = match on_disk_bytes(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not read on-disk size: {e}");
            return ExitCode::from(1);
        }
    };
    println!("apparent_bytes={apparent}");
    println!("on_disk_bytes={disk}");
    if disk > MAX_ON_DISK {
        eprintln!("error: filesystem did not create a sparse file");
        let _ = std::fs::remove_file(&path);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn parse_sizes() {
        assert_eq!(parse_size("20GiB"), Ok(21_474_836_480));
        assert_eq!(parse_size("4096"), Ok(4096));
        assert_eq!(parse_size("1MiB"), Ok(1_048_576));
        assert_eq!(parse_size("2GB"), Ok(2_000_000_000));
    }

    #[test]
    fn parse_rejects_bad() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("0").is_err());
    }

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "whim-sparse-test-{}-{tag}.sparse.bin",
            std::process::id()
        ))
    }

    #[test]
    fn creates_sparse_20gib_with_markers() {
        let p = tmp("big");
        let len = 20u64 << 30;
        create(&p, len, false).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), len);
        assert!(on_disk_bytes(&p).unwrap() < MAX_ON_DISK);
        let mut f = std::fs::File::open(&p).unwrap();
        for (off, m) in [
            (0u64, &b"WHIM-START"[..]),
            (len / 2, b"WHIM-MIDDLE"),
            (len - 16, b"WHIM-END"),
        ] {
            f.seek(SeekFrom::Start(off)).unwrap();
            let mut buf = vec![0u8; m.len()];
            f.read_exact(&mut buf).unwrap();
            assert_eq!(buf, m);
        }
        drop(f);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn marker_layout_rejects_small_and_overlap() {
        let head = head_bytes().len() as u64;
        assert!(marker_layout(10, head).is_err());
        assert!(marker_layout(head + 8, head).is_err());
        assert!(marker_layout(1 << 20, head).is_ok());
    }

    #[test]
    fn small_file_is_an_error_not_a_silent_skip() {
        let p = tmp("small");
        assert!(create(&p, 64, false).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn two_positionals_rejected() {
        let mut p = lexopt::Parser::from_args(["--size", "1MiB", "a", "b"]);
        assert!(parse_args(&mut p).is_err());
    }

    #[test]
    fn refuses_existing_without_force() {
        let p = tmp("exist");
        std::fs::write(&p, b"keep").unwrap();
        assert!(create(&p, 1 << 20, false).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"keep");
        std::fs::remove_file(&p).unwrap();
    }
}
