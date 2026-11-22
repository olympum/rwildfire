use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as FmtWrite;
use std::io::{self, Write as IoWrite};
use std::process::ExitCode;

use rwildfire_parity::{compare, Report, Tolerance, Trace};

const HELP: &str = "\
rwildfire-parity - compare rwildfire parity trace JSON

Usage:
  rwildfire-parity compare REFERENCE CANDIDATE [--abs-tol N] [--rel-tol N] [--json]
  rwildfire-parity -h | --help
  rwildfire-parity -V | --version

Arguments:
  REFERENCE      reference trace JSON file
  CANDIDATE      candidate trace JSON file

Options:
  --abs-tol N    absolute tolerance (finite, nonnegative; default 0)
  --rel-tol N    relative tolerance (finite, nonnegative; default 0)
  --json         write a stable JSON report to stdout
  -h, --help     print this help
  -V, --version  print the version

Exit codes:
  0  valid inputs match, or help/version requested
  1  valid inputs differ
  2  invalid input, invocation, or I/O failure

Comparison is exact by default. With tolerances a component matches when
|expected - actual| <= max(abs_tol, rel_tol * max(|expected|, |actual|)).
";

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    // Help and version are resolved before invocation validation so that
    // -h/--help still works alongside missing, unknown, or malformed args.
    if args
        .iter()
        .any(|arg| is_arg(arg, "--help") || is_arg(arg, "-h"))
    {
        return finish(write_stdout(HELP));
    }
    if args
        .iter()
        .any(|arg| is_arg(arg, "--version") || is_arg(arg, "-V"))
    {
        let version = format!("rwildfire-parity {}\n", env!("CARGO_PKG_VERSION"));
        return finish(write_stdout(&version));
    }

    match run(&args) {
        Ok((report, json)) => {
            let rendered = if json {
                json_report(&report)
            } else {
                Ok(human_report(&report))
            };
            match rendered.and_then(|text| write_stdout(&text)) {
                Ok(()) if report.passed() => ExitCode::SUCCESS,
                Ok(()) => ExitCode::from(1),
                Err(error) => {
                    eprintln!("rwildfire-parity: {error}");
                    ExitCode::from(2)
                }
            }
        }
        Err(error) => {
            eprintln!("rwildfire-parity: {error}");
            ExitCode::from(2)
        }
    }
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rwildfire-parity: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[OsString]) -> Result<(Report, bool), String> {
    let command = args
        .first()
        .ok_or_else(|| "missing command; expected 'compare'".to_owned())?;
    if command.to_str() != Some("compare") {
        return Err(format!("unknown command {command:?}; expected 'compare'"));
    }

    let mut paths = Vec::new();
    let mut absolute_tolerance = 0.0;
    let mut relative_tolerance = 0.0;
    let mut json = false;
    let mut index = 1;

    while index < args.len() {
        let argument = &args[index];
        let text = argument.to_str();

        if text == Some("--json") {
            json = true;
            index += 1;
        } else if text == Some("--abs-tol") {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "--abs-tol requires a value".to_owned())?;
            absolute_tolerance = parse_tolerance(value, "absolute")?;
            index += 2;
        } else if let Some(value) = text.and_then(|value| value.strip_prefix("--abs-tol=")) {
            absolute_tolerance = parse_tolerance_str(value, "absolute")?;
            index += 1;
        } else if text == Some("--rel-tol") {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "--rel-tol requires a value".to_owned())?;
            relative_tolerance = parse_tolerance(value, "relative")?;
            index += 2;
        } else if let Some(value) = text.and_then(|value| value.strip_prefix("--rel-tol=")) {
            relative_tolerance = parse_tolerance_str(value, "relative")?;
            index += 1;
        } else if matches!(text, Some(value) if value.starts_with('-')) {
            return Err(format!("unknown option {argument:?}"));
        } else {
            paths.push(argument.clone());
            index += 1;
        }
    }

    if paths.len() != 2 {
        return Err(format!(
            "expected REFERENCE and CANDIDATE paths, got {}",
            paths.len()
        ));
    }

    let tolerance = Tolerance::new(absolute_tolerance, relative_tolerance)
        .map_err(|error| error.to_string())?;
    let reference = Trace::from_path(&paths[0]).map_err(|error| error.to_string())?;
    let candidate = Trace::from_path(&paths[1]).map_err(|error| error.to_string())?;
    let report = compare(&reference, &candidate, tolerance).map_err(|error| error.to_string())?;
    Ok((report, json))
}

fn parse_tolerance(value: &OsStr, name: &'static str) -> Result<f64, String> {
    let text = value
        .to_str()
        .ok_or_else(|| format!("{name} tolerance must be valid UTF-8"))?;
    parse_tolerance_str(text, name)
}

fn parse_tolerance_str(text: &str, name: &'static str) -> Result<f64, String> {
    text.parse::<f64>()
        .map_err(|_| format!("invalid {name} tolerance {text:?}"))
}

fn human_report(report: &Report) -> String {
    let mut output = String::new();
    let summary = &report.summary;
    if summary.passed {
        let _ = writeln!(
            output,
            "PASS suite={:?} seed={} cases={} components={} abs_tol={} rel_tol={}",
            report.suite,
            report.seed,
            summary.cases,
            summary.components,
            report.tolerance.abs,
            report.tolerance.rel
        );
    } else {
        let _ = writeln!(
            output,
            "FAIL suite={:?} seed={} cases={} components={} mismatched_cases={} mismatched_components={} abs_tol={} rel_tol={}",
            report.suite,
            report.seed,
            summary.cases,
            summary.components,
            summary.mismatched_cases,
            summary.mismatched_components,
            report.tolerance.abs,
            report.tolerance.rel
        );
        for case in &report.mismatches {
            for component in &case.components {
                let _ = writeln!(
                    output,
                    "case {:?} component {}: expected {:?}, actual {:?}",
                    case.id, component.index, component.expected, component.actual
                );
            }
        }
    }
    output
}

fn json_report(report: &Report) -> Result<String, String> {
    let mut output = serde_json::to_string(report)
        .map_err(|error| format!("cannot serialize report: {error}"))?;
    output.push('\n');
    Ok(output)
}

fn write_stdout(text: &str) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("cannot write stdout: {error}"))
}

fn is_arg(argument: &OsStr, expected: &str) -> bool {
    argument == OsStr::new(expected)
}
