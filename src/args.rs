//! Command-line parsing (lexopt, no clap).

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Args {
    pub path: Option<PathBuf>,
    pub bench: bool,
    pub bench_exit_after: Option<u64>,
    pub ppp: Option<f32>,
    pub help: bool,
    pub version: bool,
}

pub const USAGE: &str = "Usage: whim [PATH] [--bench] [--bench-exit-after SECS] [--ppp FACTOR]";

pub fn parse_args<I>(args: I) -> Result<Args, lexopt::Error>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    use lexopt::prelude::*;
    let mut out = Args::default();
    let mut parser = lexopt::Parser::from_args(args);
    while let Some(arg) = parser.next()? {
        match arg {
            Long("bench") => out.bench = true,
            Long("bench-exit-after") => out.bench_exit_after = Some(parser.value()?.parse()?),
            Long("ppp") => {
                let v: f32 = parser.value()?.parse()?;
                if !(0.5..=4.0).contains(&v) {
                    return Err(lexopt::Error::Custom("--ppp must be within 0.5..=4.0".into()));
                }
                out.ppp = Some(v);
            }
            Short('h') | Long("help") => out.help = true,
            Short('V') | Long("version") => out.version = true,
            Value(v) => {
                if out.path.is_some() {
                    return Err(lexopt::Error::UnexpectedArgument(v));
                }
                out.path = Some(PathBuf::from(v));
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_only() {
        let a = parse_args(["a.txt"]).unwrap();
        assert_eq!(a.path, Some(PathBuf::from("a.txt")));
        assert!(!a.bench);
    }

    #[test]
    fn bench_flags() {
        let a = parse_args(["--bench", "--bench-exit-after", "3", "big.bin"]).unwrap();
        assert!(a.bench);
        assert_eq!(a.bench_exit_after, Some(3));
        assert_eq!(a.path, Some(PathBuf::from("big.bin")));
    }

    #[test]
    fn ppp_valid_and_invalid() {
        assert_eq!(parse_args(["--ppp", "1.5"]).unwrap().ppp, Some(1.5));
        assert!(parse_args(["--ppp", "9"]).is_err());
    }

    #[test]
    fn unknown_flag_rejected() {
        assert!(parse_args(["--bogus"]).is_err());
    }

    #[test]
    fn two_paths_rejected() {
        assert!(parse_args(["a", "b"]).is_err());
    }

    #[test]
    fn help_version() {
        assert!(parse_args(["-h"]).unwrap().help);
        assert!(parse_args(["--version"]).unwrap().version);
    }
}
