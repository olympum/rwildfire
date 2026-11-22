//! Strict comparison of rwildfire parity trace documents.
//!
//! A trace is a versioned JSON document carrying one upstream revision, one
//! suite identity, one seed, and a set of named numeric vectors. The
//! comparator validates both documents, requires matching metadata and case
//! dimensions, and reports per-case, per-component numerical differences.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The only trace protocol this comparator accepts.
pub const SCHEMA_VERSION: u32 = 1;

/// The length of a full, lowercase Git commit SHA.
const COMMIT_LEN: usize = 40;

/// A trace document in the version 1 parity protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    /// Protocol version; must be SCHEMA_VERSION.
    pub schema_version: u32,
    /// Full pinned upstream Git commit: 40 lowercase hexadecimal characters.
    pub upstream_commit: String,
    /// Suite identity, for example 'primitives-v1'.
    pub suite: String,
    /// Seed shared by every case.
    pub seed: u64,
    /// Nonempty cases with unique IDs.
    pub cases: Vec<Case>,
}

/// One named numeric vector in a trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Unique, nonempty case identifier.
    pub id: String,
    /// Nonempty finite numeric vector.
    pub values: Vec<f64>,
}

/// Absolute and relative numerical tolerances.
///
/// The default is exact comparison. Both components must be finite and
/// nonnegative; Tolerance::new enforces that, and compare validates values
/// constructed in other ways.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Tolerance {
    /// Absolute tolerance.
    pub abs: f64,
    /// Relative tolerance.
    pub rel: f64,
}

impl Tolerance {
    /// Exact comparison (both tolerances zero).
    pub const EXACT: Self = Self { abs: 0.0, rel: 0.0 };

    /// Create tolerances after rejecting negative or nonfinite values.
    pub fn new(abs: f64, rel: f64) -> Result<Self, Error> {
        validate_tolerance("absolute", abs)?;
        validate_tolerance("relative", rel)?;
        Ok(Self { abs, rel })
    }

    fn validate(&self) -> Result<(), Error> {
        validate_tolerance("absolute", self.abs)?;
        validate_tolerance("relative", self.rel)?;
        Ok(())
    }
}

impl Default for Tolerance {
    fn default() -> Self {
        Self::EXACT
    }
}

impl Trace {
    /// Parse and validate a trace from a UTF-8 JSON string.
    pub fn from_json_str(input: &str) -> Result<Self, Error> {
        let trace: Self = serde_json::from_str(input).map_err(Error::MalformedJson)?;
        trace.validate()?;
        Ok(trace)
    }

    /// Parse and validate a trace from UTF-8 JSON bytes.
    pub fn from_json_slice(input: &[u8]) -> Result<Self, Error> {
        let trace: Self = serde_json::from_slice(input).map_err(Error::MalformedJson)?;
        trace.validate()?;
        Ok(trace)
    }

    /// Read, parse, and validate a trace from a file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_slice(&bytes)
    }

    /// Validate every version 1 protocol invariant.
    ///
    /// compare calls this for both documents, so traces built directly from
    /// the public fields cannot bypass validation.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchemaVersion(self.schema_version));
        }
        validate_commit(&self.upstream_commit)?;
        if self.suite.is_empty() {
            return Err(Error::EmptySuite);
        }
        if self.cases.is_empty() {
            return Err(Error::EmptyCases);
        }

        let mut ids = HashSet::with_capacity(self.cases.len());
        for (index, case) in self.cases.iter().enumerate() {
            if case.id.is_empty() {
                return Err(Error::EmptyCaseId { index });
            }
            if !ids.insert(case.id.as_str()) {
                return Err(Error::DuplicateCaseId(case.id.clone()));
            }
            if case.values.is_empty() {
                return Err(Error::EmptyValues {
                    case: case.id.clone(),
                });
            }
            for (value_index, value) in case.values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(Error::NonFiniteValue {
                        case: case.id.clone(),
                        index: value_index,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Compare two valid traces and return a stable mismatch report.
///
/// Metadata (upstream commit, suite, seed) must agree, and the case sets and
/// vector dimensions must match exactly. Those are input errors, not
/// numerical mismatches. Cases are matched by ID, so array order is
/// irrelevant. Numerical differences are returned in Report; a report with
/// summary.passed == true is a pass.
pub fn compare(
    reference: &Trace,
    candidate: &Trace,
    tolerance: Tolerance,
) -> Result<Report, Error> {
    reference.validate()?;
    candidate.validate()?;
    tolerance.validate()?;

    ensure_same(
        "upstream_commit",
        &reference.upstream_commit,
        &candidate.upstream_commit,
    )?;
    ensure_same("suite", &reference.suite, &candidate.suite)?;
    ensure_same(
        "seed",
        &reference.seed.to_string(),
        &candidate.seed.to_string(),
    )?;

    let reference_ids: HashSet<&str> = reference
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect();
    let candidate_ids: HashSet<&str> = candidate
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect();

    let mut missing_in_candidate: Vec<String> = reference
        .cases
        .iter()
        .filter(|case| !candidate_ids.contains(case.id.as_str()))
        .map(|case| case.id.clone())
        .collect();
    let mut extra_in_candidate: Vec<String> = candidate
        .cases
        .iter()
        .filter(|case| !reference_ids.contains(case.id.as_str()))
        .map(|case| case.id.clone())
        .collect();

    if !missing_in_candidate.is_empty() || !extra_in_candidate.is_empty() {
        missing_in_candidate.sort();
        extra_in_candidate.sort();
        return Err(Error::CaseSetMismatch {
            missing_in_candidate,
            extra_in_candidate,
        });
    }

    let mut candidate_by_id: HashMap<&str, &Case> = HashMap::with_capacity(candidate.cases.len());
    for case in &candidate.cases {
        candidate_by_id.insert(case.id.as_str(), case);
    }

    for reference_case in &reference.cases {
        let candidate_case = candidate_by_id
            .get(reference_case.id.as_str())
            .expect("case sets were checked above");
        if reference_case.values.len() != candidate_case.values.len() {
            return Err(Error::DimensionMismatch {
                case: reference_case.id.clone(),
                reference: reference_case.values.len(),
                candidate: candidate_case.values.len(),
            });
        }
    }

    let mut components_compared = 0usize;
    let mut mismatches = Vec::new();
    for reference_case in &reference.cases {
        let candidate_case = candidate_by_id
            .get(reference_case.id.as_str())
            .expect("case sets were checked above");
        components_compared += reference_case.values.len();

        let mut components = Vec::new();
        for (index, (expected, actual)) in reference_case
            .values
            .iter()
            .zip(&candidate_case.values)
            .enumerate()
        {
            if !values_match(*expected, *actual, tolerance) {
                components.push(ComponentMismatch {
                    index,
                    expected: *expected,
                    actual: *actual,
                });
            }
        }
        if !components.is_empty() {
            mismatches.push(CaseMismatch {
                id: reference_case.id.clone(),
                components,
            });
        }
    }

    // Sorting by case ID makes the report independent of input array order.
    mismatches.sort_by(|left, right| left.id.cmp(&right.id));
    let mismatched_cases = mismatches.len();
    let mismatched_components = mismatches.iter().map(|case| case.components.len()).sum();
    let passed = mismatched_components == 0;

    Ok(Report {
        schema_version: SCHEMA_VERSION,
        upstream_commit: reference.upstream_commit.clone(),
        suite: reference.suite.clone(),
        seed: reference.seed,
        tolerance,
        summary: Summary {
            cases: reference.cases.len(),
            components: components_compared,
            mismatched_cases,
            mismatched_components,
            passed,
        },
        mismatches,
    })
}

/// Stable machine-readable result of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Report schema version; always SCHEMA_VERSION. This is the report
    /// protocol and is distinct from the trace protocol, which is also 1.
    pub schema_version: u32,
    /// Upstream commit shared by both documents.
    pub upstream_commit: String,
    /// Suite identity shared by both documents.
    pub suite: String,
    /// Seed shared by both documents.
    pub seed: u64,
    /// Tolerances used for this comparison.
    pub tolerance: Tolerance,
    /// Counts derived from this comparison.
    pub summary: Summary,
    /// Differing cases, sorted by case ID.
    pub mismatches: Vec<CaseMismatch>,
}

impl Report {
    /// Whether every compared component matched.
    pub fn passed(&self) -> bool {
        self.summary.passed
    }
}

/// Aggregate counts for a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Number of cases compared.
    pub cases: usize,
    /// Number of scalar components compared.
    pub components: usize,
    /// Number of cases with at least one mismatch.
    pub mismatched_cases: usize,
    /// Total number of differing scalar components.
    pub mismatched_components: usize,
    /// True exactly when mismatched_components is zero.
    pub passed: bool,
}

/// All differing components in one case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseMismatch {
    /// Case identifier.
    pub id: String,
    /// Differing components in ascending index order.
    pub components: Vec<ComponentMismatch>,
}

/// One differing scalar component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ComponentMismatch {
    /// Zero-based index inside the case vector.
    pub index: usize,
    /// Reference value.
    pub expected: f64,
    /// Candidate value.
    pub actual: f64,
}

fn ensure_same(field: &'static str, reference: &str, candidate: &str) -> Result<(), Error> {
    if reference == candidate {
        Ok(())
    } else {
        Err(Error::MetadataMismatch {
            field,
            reference: reference.to_owned(),
            candidate: candidate.to_owned(),
        })
    }
}

fn validate_commit(commit: &str) -> Result<(), Error> {
    let valid = commit.len() == COMMIT_LEN
        && commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidUpstreamCommit(commit.to_owned()))
    }
}

fn validate_tolerance(name: &'static str, value: f64) -> Result<(), Error> {
    if !value.is_finite() {
        return Err(Error::NonFiniteTolerance { name });
    }
    if value < 0.0 {
        return Err(Error::NegativeTolerance { name, value });
    }
    Ok(())
}

/// Compare one pair of finite f64 values.
///
/// Exact comparison treats signed zero as equal. With tolerances, the
/// accepted inequality is
/// |expected - actual| <= max(abs_tol, rel_tol * max(|expected|, |actual|)).
/// The implementation never turns arithmetic overflow into a pass: an
/// overflowing subtraction cannot fit a finite absolute tolerance, and the
/// relative branch compares a normalized difference instead of multiplying
/// two values that may overflow.
fn values_match(expected: f64, actual: f64, tolerance: Tolerance) -> bool {
    if expected == actual {
        return true;
    }

    let absolute_difference = (expected - actual).abs();
    if absolute_difference.is_finite() {
        if absolute_difference <= tolerance.abs {
            return true;
        }
        if tolerance.rel == 0.0 {
            return false;
        }
        let scale = expected.abs().max(actual.abs());
        return absolute_difference / scale <= tolerance.rel;
    }

    // The subtraction overflowed to infinity, so the true difference is
    // larger than any finite absolute tolerance. Normalize both operands by
    // their shared magnitude to compare against the relative tolerance
    // without recreating the overflow.
    if tolerance.rel == 0.0 {
        return false;
    }
    let scale = expected.abs().max(actual.abs());
    let normalized_difference = (expected / scale - actual / scale).abs();
    normalized_difference <= tolerance.rel
}

/// Errors that make a comparison an input error rather than evidence.
#[derive(Debug)]
pub enum Error {
    /// The JSON text is malformed or violates the strict field schema.
    MalformedJson(serde_json::Error),
    /// A trace file could not be read.
    Read {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The document does not use schema version 1.
    UnsupportedSchemaVersion(u32),
    /// The upstream commit is not a full lowercase SHA.
    InvalidUpstreamCommit(String),
    /// The suite identity is empty.
    EmptySuite,
    /// The trace has no cases.
    EmptyCases,
    /// A case has an empty identifier.
    EmptyCaseId {
        /// Position of the case in the array.
        index: usize,
    },
    /// Two cases share an identifier.
    DuplicateCaseId(String),
    /// A case has an empty values vector.
    EmptyValues {
        /// Case identifier.
        case: String,
    },
    /// A case value is NaN or infinite.
    NonFiniteValue {
        /// Case identifier.
        case: String,
        /// Position of the value in the vector.
        index: usize,
    },
    /// A tolerance is NaN or infinite.
    NonFiniteTolerance {
        /// Tolerance name: absolute or relative.
        name: &'static str,
    },
    /// A tolerance is negative.
    NegativeTolerance {
        /// Tolerance name: absolute or relative.
        name: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// Reference and candidate metadata disagree.
    MetadataMismatch {
        /// Metadata field name.
        field: &'static str,
        /// Reference value.
        reference: String,
        /// Candidate value.
        candidate: String,
    },
    /// Reference and candidate case sets differ.
    CaseSetMismatch {
        /// IDs present in the reference but absent from the candidate.
        missing_in_candidate: Vec<String>,
        /// IDs present in the candidate but absent from the reference.
        extra_in_candidate: Vec<String>,
    },
    /// One case has different vector dimensions in the two documents.
    DimensionMismatch {
        /// Case identifier.
        case: String,
        /// Reference dimension.
        reference: usize,
        /// Candidate dimension.
        candidate: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedJson(source) => write!(formatter, "invalid trace JSON: {source}"),
            Self::Read { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "unsupported schema_version {version}; expected {SCHEMA_VERSION}"
            ),
            Self::InvalidUpstreamCommit(commit) => write!(
                formatter,
                "upstream_commit must be 40 lowercase hexadecimal characters, got {commit:?}"
            ),
            Self::EmptySuite => formatter.write_str("suite must not be empty"),
            Self::EmptyCases => formatter.write_str("cases must not be empty"),
            Self::EmptyCaseId { index } => {
                write!(formatter, "case at index {index} has an empty id")
            }
            Self::DuplicateCaseId(id) => write!(formatter, "duplicate case id {id:?}"),
            Self::EmptyValues { case } => write!(formatter, "case {case:?} has no values"),
            Self::NonFiniteValue { case, index } => {
                write!(formatter, "case {case:?} value {index} is not finite")
            }
            Self::NonFiniteTolerance { name } => {
                write!(formatter, "{name} tolerance must be finite")
            }
            Self::NegativeTolerance { name, value } => write!(
                formatter,
                "{name} tolerance must be nonnegative, got {value:?}"
            ),
            Self::MetadataMismatch {
                field,
                reference,
                candidate,
            } => write!(
                formatter,
                "metadata mismatch for {field}: reference {reference:?}, candidate {candidate:?}"
            ),
            Self::CaseSetMismatch {
                missing_in_candidate,
                extra_in_candidate,
            } => write!(
                formatter,
                "case sets differ: missing from candidate {missing_in_candidate:?}, extra in candidate {extra_in_candidate:?}"
            ),
            Self::DimensionMismatch {
                case,
                reference,
                candidate,
            } => write!(
                formatter,
                "case {case:?} dimension mismatch: reference {reference}, candidate {candidate}"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MalformedJson(source) => Some(source),
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}
