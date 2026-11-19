//! `bundle-macos`: assemble an unsigned `Whim.app` bundle (D-21, PERF-04).
//!
//! Uses only `std::fs` so it runs and is tested on every OS. Ad-hoc signing and
//! `ditto` zipping happen in CI on macOS.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const BUNDLE_ID: &str = "io.github.ajoakim.whim";
pub const BUNDLE_NAME: &str = "Whim";
pub const EXECUTABLE: &str = "whim";
pub const MIN_MACOS: &str = "11.0";

const HELP: &str = "cargo xtask bundle-macos [--bin PATH] [--out DIR] [--version V]\n  Assembles an unsigned Whim.app bundle in DIR (default target/bundle).";

/// Minimal XML text escaping for plist `<string>` values.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Renders `Contents/Info.plist` for the given package version.
pub fn info_plist(version: &str) -> String {
    let v = xml_escape(version);
    let id = xml_escape(BUNDLE_ID);
    let name = xml_escape(BUNDLE_NAME);
    let exe = xml_escape(EXECUTABLE);
    let min = xml_escape(MIN_MACOS);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>{exe}</string>
	<key>CFBundleIdentifier</key>
	<string>{id}</string>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{v}</string>
	<key>CFBundleVersion</key>
	<string>{v}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>LSMinimumSystemVersion</key>
	<string>{min}</string>
</dict>
</plist>
"#
    )
}

/// Extracts `version = "..."` from the `[package]` section of a `Cargo.toml`.
pub fn package_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for raw in cargo_toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((k, rest)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != "version" {
            continue;
        }
        let val = rest.split('#').next().unwrap_or("").trim();
        if let Some(stripped) = val.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Some(stripped.to_owned());
        }
    }
    None
}

/// Builds `<out_dir>/Whim.app`, replacing any existing bundle there.
pub fn build_bundle(bin: &Path, out_dir: &Path, version: &str) -> std::io::Result<PathBuf> {
    let app_dir = out_dir.join(format!("{BUNDLE_NAME}.app"));
    if app_dir.exists() {
        std::fs::remove_dir_all(&app_dir)?;
    }
    let contents = app_dir.join("Contents");
    let macos_dir = contents.join("MacOS");
    let resources_dir = contents.join("Resources");
    std::fs::create_dir_all(&macos_dir)?;
    std::fs::create_dir_all(&resources_dir)?;

    let dst_bin = macos_dir.join(EXECUTABLE);
    std::fs::copy(bin, &dst_bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst_bin, std::fs::Permissions::from_mode(0o755))?;
    }

    std::fs::write(contents.join("Info.plist"), info_plist(version))?;
    std::fs::write(contents.join("PkgInfo"), b"APPL????")?;

    Ok(app_dir)
}

type Parsed = (Option<PathBuf>, Option<PathBuf>, Option<String>);

fn parse(args: &mut lexopt::Parser) -> Result<Option<Parsed>, lexopt::Error> {
    use lexopt::prelude::*;
    let mut bin = None;
    let mut out = None;
    let mut version = None;
    while let Some(a) = args.next()? {
        match a {
            Long("bin") => bin = Some(args.value()?.into()),
            Long("out") => out = Some(args.value()?.into()),
            Long("version") => version = Some(args.value()?.string()?),
            Long("help") | Short('h') => return Ok(None),
            other => return Err(other.unexpected()),
        }
    }
    Ok(Some((bin, out, version)))
}

pub fn run(mut args: lexopt::Parser) -> ExitCode {
    let (bin, out, version) = match parse(&mut args) {
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

    let bin = bin.unwrap_or_else(crate::default_bin);
    if !bin.is_file() {
        eprintln!("error: binary not found: {}", bin.display());
        return ExitCode::from(2);
    }
    let out = out.unwrap_or_else(|| crate::workspace_root().join("target").join("bundle"));
    let version = match version {
        Some(v) => v,
        None => {
            let toml_path = crate::workspace_root().join("Cargo.toml");
            let text = match std::fs::read_to_string(&toml_path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", toml_path.display());
                    return ExitCode::from(2);
                }
            };
            match package_version(&text) {
                Some(v) => v,
                None => {
                    eprintln!(
                        "error: [package].version not found in {}",
                        toml_path.display()
                    );
                    return ExitCode::from(2);
                }
            }
        }
    };

    match build_bundle(&bin, &out, &version) {
        Ok(app) => {
            println!("{}", app.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let p =
                std::env::temp_dir().join(format!("xtask-bundle-test-{}-{n}", std::process::id()));
            std::fs::create_dir(&p).expect("create unique test dir");
            Self(p)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn xml_escapes_special_chars() {
        assert_eq!(xml_escape("a&b<c>\"'"), "a&amp;b&lt;c&gt;&quot;&apos;");
    }

    #[test]
    fn info_plist_has_required_keys_and_balanced_tags() {
        let p = info_plist("0.1.0");
        assert!(p.contains(r#"<plist version="1.0">"#));
        assert_eq!(p.matches("<dict>").count(), 1);
        assert_eq!(p.matches("</dict>").count(), 1);
        assert_eq!(p.matches("<key>CFBundleExecutable</key>").count(), 1);
        assert_eq!(p.matches("<string>whim</string>").count(), 1);
        assert_eq!(p.matches("<key>CFBundleIdentifier</key>").count(), 1);
        assert_eq!(
            p.matches("<string>io.github.ajoakim.whim</string>").count(),
            1
        );
        assert_eq!(p.matches("<key>CFBundleName</key>").count(), 1);
        assert_eq!(p.matches("<key>CFBundleDisplayName</key>").count(), 1);
        assert_eq!(p.matches("<key>CFBundlePackageType</key>").count(), 1);
        assert_eq!(p.matches("<string>APPL</string>").count(), 1);
        assert_eq!(
            p.matches("<key>CFBundleShortVersionString</key>").count(),
            1
        );
        assert_eq!(p.matches("<key>CFBundleVersion</key>").count(), 1);
        assert_eq!(p.matches("<string>0.1.0</string>").count(), 2);
        assert!(p.contains("<key>NSHighResolutionCapable</key>\n\t<true/>"));
        assert!(p.contains("<key>LSMinimumSystemVersion</key>\n\t<string>11.0</string>"));
    }

    #[test]
    fn package_version_reads_only_package_section() {
        let sample = r#"
[workspace]
version = "9.9.9"

[package]
name = "whim"
version = "0.1.0" # comment

[dependencies]
version = "8.8.8"
"#;
        assert_eq!(package_version(sample).as_deref(), Some("0.1.0"));
        assert_eq!(package_version("[dependencies]\nversion = \"1.0.0\""), None);
    }

    #[test]
    fn build_bundle_creates_layout_and_copies_binary() {
        let tmp = TestDir::new();
        let fake_bin = tmp.0.join("whim");
        let bytes = b"\xCF\xFA\xED\xFEfake-mach-o";
        std::fs::write(&fake_bin, bytes).unwrap();
        let out = tmp.0.join("out");

        let app = build_bundle(&fake_bin, &out, "0.1.0").unwrap();
        assert_eq!(app, out.join("Whim.app"));
        let copied = app.join("Contents/MacOS/whim");
        assert_eq!(std::fs::read(&copied).unwrap(), bytes);
        assert_eq!(
            std::fs::read(app.join("Contents/PkgInfo")).unwrap(),
            b"APPL????"
        );
        assert!(app.join("Contents/Info.plist").is_file());
        assert!(app.join("Contents/Resources").is_dir());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&copied).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
    }

    #[test]
    fn build_bundle_replaces_existing_bundle() {
        let tmp = TestDir::new();
        let fake_bin = tmp.0.join("whim");
        std::fs::write(&fake_bin, b"v1").unwrap();
        let out = tmp.0.join("out");

        let app = build_bundle(&fake_bin, &out, "0.1.0").unwrap();
        let stale = app.join("Contents/Resources/stale.txt");
        std::fs::write(&stale, b"old").unwrap();
        assert!(stale.exists());

        std::fs::write(&fake_bin, b"v2").unwrap();
        build_bundle(&fake_bin, &out, "0.2.0").unwrap();
        assert!(!stale.exists());
        assert_eq!(
            std::fs::read(app.join("Contents/MacOS/whim")).unwrap(),
            b"v2"
        );
    }
}
