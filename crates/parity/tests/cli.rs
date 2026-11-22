use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const COMMIT: &str = "2a26806eb7487ea60eb3b1a554c8a73120d54343";
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rwildfire-parity-cli-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create temp dir");
        Self { path }
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn case(id: &str, values: &str) -> String {
    format!(r#"{{"id":"{id}","values":[{values}]}}"#)
}

fn trace(suite: &str, seed: u64, cases: &[String]) -> String {
    trace_with_commit(COMMIT, suite, seed, cases)
}

fn trace_with_commit(commit: &str, suite: &str, seed: u64, cases: &[String]) -> String {
    format!(
        r#"{{"schema_version":1,"upstream_commit":"{commit}","suite":"{suite}","seed":{seed},"cases":[{}]}}"#,
        cases.join(",")
    )
}

fn one_case(suite: &str, seed: u64, values: &str) -> String {
    trace(suite, seed, &[case("x", values)])
}

fn append_top_level_field(mut json: String, field: &str) -> String {
    json.pop();
    json.push_str(field);
    json.push('}');
    json
}

fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_rwildfire-parity"))
        .args(args)
        .output()
        .expect("run rwildfire-parity")
}

fn run_compare(reference: &Path, candidate: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rwildfire-parity"))
        .arg("compare")
        .arg(reference)
        .arg(candidate)
        .args(extra)
        .output()
        .expect("run rwildfire-parity")
}

fn text(output: &Output) -> (String, String) {
    (
        String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8"),
        String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8"),
    )
}

fn assert_exit(output: &Output, expected: i32) {
    let (stdout, stderr) = text(output);
    assert_eq!(
        output.status.code(),
        Some(expected),
        "expected exit {expected}; stdout={stdout:?} stderr={stderr:?}"
    );
}

fn report(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be a JSON report")
}

#[test]
fn help_and_version_win_before_argument_validation() {
    for args in [
        vec!["-h"],
        vec!["--help"],
        vec!["compare", "--help"],
        vec!["compare", "--bogus", "a", "b", "--help"],
        vec!["frobnicate", "-h", "--bad"],
    ] {
        let output = run(args);
        assert_exit(&output, 0);
        let (stdout, stderr) = text(&output);
        assert!(stdout.contains("Usage:"), "stdout: {stdout:?}");
        assert!(stderr.is_empty(), "stderr: {stderr:?}");
    }

    for args in [vec!["-V"], vec!["--version"]] {
        let output = run(args);
        assert_exit(&output, 0);
        let (stdout, stderr) = text(&output);
        assert!(
            stdout.starts_with("rwildfire-parity "),
            "stdout: {stdout:?}"
        );
        assert!(stderr.is_empty(), "stderr: {stderr:?}");
    }
}

#[test]
fn invocation_and_tolerance_errors_exit_2_without_stdout() {
    let invocation_cases: &[&[&str]] = &[
        &[],
        &["compare"],
        &["frobnicate"],
        &["compare", "a"],
        &["compare", "a", "b", "c"],
        &["compare", "a", "b", "--unknown"],
    ];

    for &args in invocation_cases {
        let output = run(args);
        assert_exit(&output, 2);
        let (stdout, stderr) = text(&output);
        assert!(stdout.is_empty(), "args={args:?} stdout={stdout:?}");
        assert!(
            stderr.contains("rwildfire-parity:"),
            "args={args:?} stderr={stderr:?}"
        );
    }

    // Valid traces make the tolerance table fail for the tolerance reason.
    // If an option were ignored, the comparison would pass instead of exiting 2.
    let dir = TempDir::new();
    let reference = dir.write(
        "reference.json",
        &trace("primitives-v1", 7, &[case("x", "1.0")]),
    );
    let candidate = dir.write(
        "candidate.json",
        &trace("primitives-v1", 7, &[case("x", "1.0")]),
    );
    assert_exit(&run_compare(&reference, &candidate, &[]), 0);

    let tolerance_cases: &[(&[&str], &str)] = &[
        (&["--abs-tol", "abc"], "invalid absolute tolerance"),
        (&["--abs-tol=abc"], "invalid absolute tolerance"),
        (
            &["--abs-tol", "-1"],
            "absolute tolerance must be nonnegative",
        ),
        (
            &["--rel-tol", "-0.5"],
            "relative tolerance must be nonnegative",
        ),
        (&["--rel-tol", "NaN"], "relative tolerance must be finite"),
        (&["--abs-tol", "inf"], "absolute tolerance must be finite"),
        (&["--rel-tol", "inf"], "relative tolerance must be finite"),
        (&["--abs-tol"], "--abs-tol requires a value"),
    ];

    for &(extra, expected) in tolerance_cases {
        let output = run_compare(&reference, &candidate, extra);
        assert_exit(&output, 2);
        let (stdout, stderr) = text(&output);
        assert!(stdout.is_empty(), "args={extra:?} stdout={stdout:?}");
        assert!(
            stderr.contains(expected),
            "args={extra:?} expected {expected:?} in stderr {stderr:?}"
        );
    }
}

#[test]
fn missing_files_exit_2() {
    let dir = TempDir::new();
    let reference = dir.path.join("missing-reference.json");
    let candidate = dir.path.join("missing-candidate.json");

    let output = run_compare(&reference, &candidate, &[]);

    assert_exit(&output, 2);
    let (stdout, stderr) = text(&output);
    assert!(stdout.is_empty());
    assert!(stderr.contains("cannot read"), "stderr: {stderr:?}");
}

#[test]
fn human_output_reports_pass_and_mismatch() {
    let dir = TempDir::new();
    let reference = dir.write(
        "reference.json",
        &trace(
            "primitives-v1",
            7,
            &[case("b", "1.0,2.0"), case("a", "0.0,-0.0")],
        ),
    );
    let candidate_pass = dir.write(
        "candidate-pass.json",
        &trace(
            "primitives-v1",
            7,
            &[case("a", "-0.0,0.0"), case("b", "1.0,2.0")],
        ),
    );
    let candidate_mismatch = dir.write(
        "candidate-mismatch.json",
        &trace(
            "primitives-v1",
            7,
            &[case("a", "-0.0,0.0"), case("b", "1.0,2.5")],
        ),
    );

    let output = run_compare(&reference, &candidate_pass, &[]);
    assert_exit(&output, 0);
    let (stdout, stderr) = text(&output);
    assert!(stdout.starts_with("PASS "), "stdout: {stdout:?}");
    assert!(stderr.is_empty(), "stderr: {stderr:?}");

    let output = run_compare(&reference, &candidate_mismatch, &[]);
    assert_exit(&output, 1);
    let (stdout, stderr) = text(&output);
    assert!(stdout.starts_with("FAIL "), "stdout: {stdout:?}");
    assert!(
        stdout.contains("case \"b\" component 1: expected 2.0, actual 2.5"),
        "stdout: {stdout:?}"
    );
    assert!(stderr.is_empty(), "stderr: {stderr:?}");
}

#[test]
fn json_report_shape_and_exit_codes() {
    let dir = TempDir::new();
    let reference = dir.write(
        "reference.json",
        &trace(
            "primitives-v1",
            7,
            &[case("b", "1.0,2.0"), case("a", "0.0,-0.0")],
        ),
    );
    let candidate_pass = dir.write(
        "candidate-pass.json",
        &trace(
            "primitives-v1",
            7,
            &[case("a", "-0.0,0.0"), case("b", "1.0,2.0")],
        ),
    );
    let candidate_mismatch = dir.write(
        "candidate-mismatch.json",
        &trace(
            "primitives-v1",
            7,
            &[case("a", "-0.0,0.0"), case("b", "1.0,2.5")],
        ),
    );

    let output = run_compare(&reference, &candidate_mismatch, &["--json"]);
    assert_exit(&output, 1);
    let value = report(&output);
    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(value["suite"].as_str(), Some("primitives-v1"));
    assert_eq!(value["seed"].as_u64(), Some(7));
    assert_eq!(value["tolerance"]["abs"].as_f64(), Some(0.0));
    assert_eq!(value["tolerance"]["rel"].as_f64(), Some(0.0));
    assert_eq!(value["summary"]["cases"].as_u64(), Some(2));
    assert_eq!(value["summary"]["components"].as_u64(), Some(4));
    assert_eq!(value["summary"]["mismatched_cases"].as_u64(), Some(1));
    assert_eq!(value["summary"]["mismatched_components"].as_u64(), Some(1));
    assert_eq!(value["summary"]["passed"].as_bool(), Some(false));
    assert_eq!(value["mismatches"][0]["id"].as_str(), Some("b"));
    assert_eq!(
        value["mismatches"][0]["components"][0]["index"].as_u64(),
        Some(1)
    );
    assert_eq!(
        value["mismatches"][0]["components"][0]["expected"].as_f64(),
        Some(2.0)
    );
    assert_eq!(
        value["mismatches"][0]["components"][0]["actual"].as_f64(),
        Some(2.5)
    );
    let (stdout, stderr) = text(&output);
    assert_eq!(stdout.lines().count(), 1, "stdout: {stdout:?}");
    assert!(stderr.is_empty(), "stderr: {stderr:?}");

    let output = run_compare(&reference, &candidate_pass, &["--json"]);
    assert_exit(&output, 0);
    let value = report(&output);
    assert_eq!(value["summary"]["passed"].as_bool(), Some(true));
    assert_eq!(
        value["mismatches"].as_array().map(|items| items.len()),
        Some(0)
    );
}

#[test]
fn tolerance_flags_apply_and_boundaries_hold() {
    let dir = TempDir::new();
    let reference_one = dir.write("reference-one.json", &one_case("primitives-v1", 7, "1.0"));
    let candidate_one_five = dir.write(
        "candidate-one-five.json",
        &one_case("primitives-v1", 7, "1.5"),
    );
    let candidate_two = dir.write("candidate-two.json", &one_case("primitives-v1", 7, "2.0"));

    for (extra, expected) in [
        (vec!["--abs-tol", "0.5"], 0),
        (vec!["--abs-tol=0.5"], 0),
        (vec!["--abs-tol", "0.49"], 1),
    ] {
        let output = run_compare(&reference_one, &candidate_one_five, &extra);
        assert_exit(&output, expected);
    }

    for (extra, expected) in [
        (vec!["--rel-tol", "0.5"], 0),
        (vec!["--rel-tol=0.5"], 0),
        (vec!["--rel-tol", "0.49999999999999994"], 1),
    ] {
        let output = run_compare(&reference_one, &candidate_two, &extra);
        assert_exit(&output, expected);
    }
}

#[test]
fn invalid_trace_documents_exit_2_without_report() {
    let dir = TempDir::new();
    let reference = dir.write(
        "reference.json",
        &trace("primitives-v1", 7, &[case("x", "1.0")]),
    );
    let valid = trace("primitives-v1", 7, &[case("x", "1.0")]);
    let candidates = [
        ("malformed", "{".to_owned()),
        (
            "unknown top-level field",
            append_top_level_field(valid.clone(), ",\"extra\":1"),
        ),
        ("empty cases", trace("primitives-v1", 7, &[])),
        (
            "duplicate case id",
            trace("primitives-v1", 7, &[case("x", "1.0"), case("x", "2.0")]),
        ),
        ("empty values", trace("primitives-v1", 7, &[case("x", "")])),
        ("NaN value", trace("primitives-v1", 7, &[case("x", "NaN")])),
        (
            "positive infinity value",
            trace("primitives-v1", 7, &[case("x", "Infinity")]),
        ),
        (
            "negative infinity value",
            trace("primitives-v1", 7, &[case("x", "-Infinity")]),
        ),
        (
            "missing case",
            trace("primitives-v1", 7, &[case("y", "1.0")]),
        ),
        (
            "extra case",
            trace("primitives-v1", 7, &[case("x", "1.0"), case("y", "2.0")]),
        ),
        (
            "dimension mismatch",
            trace("primitives-v1", 7, &[case("x", "1.0,2.0")]),
        ),
        (
            "suite metadata mismatch",
            trace("other-suite", 7, &[case("x", "1.0")]),
        ),
        (
            "seed metadata mismatch",
            trace("primitives-v1", 8, &[case("x", "1.0")]),
        ),
        (
            "commit metadata mismatch",
            trace_with_commit(&"a".repeat(40), "primitives-v1", 7, &[case("x", "1.0")]),
        ),
        (
            "unsupported schema",
            valid.replacen("\"schema_version\":1", "\"schema_version\":2", 1),
        ),
    ];

    for (index, (label, json)) in candidates.into_iter().enumerate() {
        let candidate = dir.write(&format!("candidate-{index}.json"), &json);
        let output = run_compare(&reference, &candidate, &["--json"]);
        assert_exit(&output, 2);
        let (stdout, stderr) = text(&output);
        assert!(stdout.is_empty(), "{label}: stdout={stdout:?}");
        assert!(
            stderr.contains("rwildfire-parity:"),
            "{label}: stderr={stderr:?}"
        );
    }
}

#[test]
fn opposite_sign_max_json_report_is_valid_and_exits_1() {
    let dir = TempDir::new();
    let reference = dir.write(
        "max.json",
        &one_case("primitives-v1", 7, "1.7976931348623157e308"),
    );
    let candidate = dir.write(
        "min.json",
        &one_case("primitives-v1", 7, "-1.7976931348623157e308"),
    );

    let output = run_compare(&reference, &candidate, &["--json"]);
    assert_exit(&output, 1);
    let value = report(&output);
    assert_eq!(value["summary"]["passed"].as_bool(), Some(false));
    assert_eq!(
        value["mismatches"][0]["components"][0]["expected"].as_f64(),
        Some(f64::MAX)
    );
    assert_eq!(
        value["mismatches"][0]["components"][0]["actual"].as_f64(),
        Some(-f64::MAX)
    );

    for extra in [
        vec!["--abs-tol", "1.7976931348623157e308", "--json"],
        vec!["--rel-tol", "1", "--json"],
        vec!["--rel-tol", "1.5", "--json"],
    ] {
        let output = run_compare(&reference, &candidate, &extra);
        assert_exit(&output, 1);
        assert_eq!(report(&output)["summary"]["passed"].as_bool(), Some(false));
    }

    let output = run_compare(&reference, &candidate, &["--rel-tol", "2", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(report(&output)["summary"]["passed"].as_bool(), Some(true));
}
