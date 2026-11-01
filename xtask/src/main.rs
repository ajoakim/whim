use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <subcommand>
  sparse --size 20GiB [--force] <path>   create a sparse file
  check-size [--bin PATH] [--max-mb F] [--label S]   stripped-size gate (MB = MiB = 1,048,576 bytes)
  measure [--bin PATH] [--file PATH] [--runs N] [--idle-secs S] [--timeout-secs T]
          [--max-startup-ms F] [--max-rss-mb F] [--max-idle-cpu F] [--label S] [--extra-arg ARG]...";

fn main() -> ExitCode {
    let mut parser = lexopt::Parser::from_env();
    let sub = match parser.next() {
        Ok(Some(lexopt::Arg::Value(v))) => v.to_string_lossy().into_owned(),
        Ok(Some(lexopt::Arg::Long("help") | lexopt::Arg::Short('h'))) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match sub.as_str() {
        "sparse" => xtask::sparse::run(parser),
        "check-size" => xtask::size::run(parser),
        "measure" => xtask::measure::run(parser),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
