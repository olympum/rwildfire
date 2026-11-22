use rwildfire_parity::{compare, Case, Error, Tolerance, Trace, SCHEMA_VERSION};

const COMMIT: &str = "2a26806eb7487ea60eb3b1a554c8a73120d54343";

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

fn parse(input: &str) -> Trace {
    Trace::from_json_str(input).expect("fixture must be valid")
}

fn struct_trace(id: &str, values: Vec<f64>) -> Trace {
    Trace {
        schema_version: SCHEMA_VERSION,
        upstream_commit: COMMIT.to_owned(),
        suite: "primitives-v1".to_owned(),
        seed: 7,
        cases: vec![Case {
            id: id.to_owned(),
            values,
        }],
    }
}

fn assert_invalid(input: &str, expected: impl Fn(&Error) -> bool, label: &str) {
    match Trace::from_json_str(input) {
        Ok(_) => panic!("{label}: expected the trace to be rejected"),
        Err(error) if expected(&error) => {}
        Err(error) => panic!("{label}: unexpected error: {error}"),
    }
}

fn append_top_level_field(mut json: String, field: &str) -> String {
    json.pop();
    json.push_str(field);
    json.push('}');
    json
}

#[test]
fn exact_default_matches_by_id_ignoring_order_and_signed_zero() {
    let reference = trace(
        "primitives-v1",
        7,
        &[case("b", "1.0,2.0"), case("a", "0.0,-0.0")],
    );
    let candidate = trace(
        "primitives-v1",
        7,
        &[case("a", "-0.0,0.0"), case("b", "1.0,2.0")],
    );

    let report = compare(&parse(&reference), &parse(&candidate), Tolerance::default()).unwrap();

    assert!(report.passed());
    assert_eq!(report.summary.cases, 2);
    assert_eq!(report.summary.components, 4);
    assert_eq!(report.summary.mismatched_cases, 0);
    assert_eq!(report.summary.mismatched_components, 0);
    assert!(report.mismatches.is_empty());
}

#[test]
fn exact_difference_reports_sorted_cases_and_component_indices() {
    let reference = trace(
        "primitives-v1",
        7,
        &[case("z", "4.0"), case("a", "1.0,2.0,3.0")],
    );
    let candidate = trace(
        "primitives-v1",
        7,
        &[case("a", "1.0,2.5,3.0"), case("z", "5.0")],
    );

    let report = compare(&parse(&reference), &parse(&candidate), Tolerance::EXACT).unwrap();

    assert!(!report.passed());
    assert_eq!(report.summary.cases, 2);
    assert_eq!(report.summary.components, 4);
    assert_eq!(report.summary.mismatched_cases, 2);
    assert_eq!(report.summary.mismatched_components, 2);
    assert_eq!(
        report
            .mismatches
            .iter()
            .map(|mismatch| mismatch.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
    let first = &report.mismatches[0];
    assert_eq!(first.components.len(), 1);
    assert_eq!(first.components[0].index, 1);
    assert_eq!(first.components[0].expected, 2.0);
    assert_eq!(first.components[0].actual, 2.5);
}

#[test]
fn absolute_tolerance_boundary_is_inclusive() {
    let reference = struct_trace("x", vec![1.0]);
    let candidate = struct_trace("x", vec![1.5]);
    let just_below = f64::from_bits(0.5f64.to_bits() - 1);

    let at_boundary = compare(&reference, &candidate, Tolerance::new(0.5, 0.0).unwrap()).unwrap();
    assert!(at_boundary.passed());

    let below_boundary = compare(
        &reference,
        &candidate,
        Tolerance::new(just_below, 0.0).unwrap(),
    )
    .unwrap();
    assert!(!below_boundary.passed());
    assert_eq!(below_boundary.summary.mismatched_components, 1);
}

#[test]
fn relative_tolerance_boundary_is_inclusive() {
    let reference = struct_trace("x", vec![1.0]);
    let candidate = struct_trace("x", vec![2.0]);
    let just_below = f64::from_bits(0.5f64.to_bits() - 1);

    let at_boundary = compare(&reference, &candidate, Tolerance::new(0.0, 0.5).unwrap()).unwrap();
    assert!(at_boundary.passed());

    let below_boundary = compare(
        &reference,
        &candidate,
        Tolerance::new(0.0, just_below).unwrap(),
    )
    .unwrap();
    assert!(!below_boundary.passed());

    let exact = compare(&reference, &candidate, Tolerance::EXACT).unwrap();
    assert!(!exact.passed());
}

#[test]
fn negative_and_nonfinite_tolerances_are_rejected() {
    assert!(matches!(
        Tolerance::new(-1.0, 0.0),
        Err(Error::NegativeTolerance {
            name: "absolute",
            ..
        })
    ));
    assert!(matches!(
        Tolerance::new(0.0, -0.5),
        Err(Error::NegativeTolerance {
            name: "relative",
            ..
        })
    ));
    assert!(matches!(
        Tolerance::new(f64::NAN, 0.0),
        Err(Error::NonFiniteTolerance { name: "absolute" })
    ));
    assert!(matches!(
        Tolerance::new(0.0, f64::INFINITY),
        Err(Error::NonFiniteTolerance { name: "relative" })
    ));
    assert!(matches!(
        Tolerance::new(f64::NEG_INFINITY, 0.0),
        Err(Error::NonFiniteTolerance { name: "absolute" })
    ));

    let trace = struct_trace("x", vec![1.0]);
    assert!(matches!(
        compare(
            &trace,
            &trace,
            Tolerance {
                abs: f64::NAN,
                rel: 0.0
            }
        ),
        Err(Error::NonFiniteTolerance { name: "absolute" })
    ));
    assert!(matches!(
        compare(
            &trace,
            &trace,
            Tolerance {
                abs: 0.0,
                rel: -1.0
            }
        ),
        Err(Error::NegativeTolerance {
            name: "relative",
            ..
        })
    ));
}

#[test]
fn opposite_sign_max_never_passes_through_overflow() {
    let reference = struct_trace("huge", vec![f64::MAX]);
    let candidate = struct_trace("huge", vec![-f64::MAX]);
    let just_below_two = f64::from_bits(2.0f64.to_bits() - 1);

    let rejected = [
        Tolerance::EXACT,
        Tolerance::new(f64::MAX, 0.0).unwrap(),
        Tolerance::new(0.0, 1.0).unwrap(),
        Tolerance::new(f64::MAX, 1.5).unwrap(),
        Tolerance::new(0.0, just_below_two).unwrap(),
    ];
    for tolerance in rejected {
        let report = compare(&reference, &candidate, tolerance).unwrap();
        assert!(
            !report.passed(),
            "tolerance {tolerance:?} must not manufacture a pass"
        );
        assert_eq!(report.summary.mismatched_components, 1);
    }

    let accepted = compare(&reference, &candidate, Tolerance::new(0.0, 2.0).unwrap()).unwrap();
    assert!(accepted.passed());
}

#[test]
fn decimal_json_preserves_f64_roundtrip() {
    // 1.009e-28 is a fixed serde_json regression vector: its default parser
    // (without float_roundtrip) returns 0x3a1ff9fb03194a70, while Rust's
    // correctly rounded parser returns the expected bits below.
    let input = trace(
        "primitives-v1",
        7,
        &[case("x", "1.7976931348623157e308,0.1,5e-324,1.009e-28")],
    );
    let trace = parse(&input);

    assert_eq!(trace.cases[0].values[0].to_bits(), f64::MAX.to_bits());
    assert_eq!(trace.cases[0].values[1].to_bits(), 0.1f64.to_bits());
    assert_eq!(
        trace.cases[0].values[2].to_bits(),
        f64::from_bits(1).to_bits()
    );
    assert_eq!(trace.cases[0].values[3].to_bits(), 0x3a1f_f9fb_0319_4a6f);
}

#[test]
fn zero_and_subnormal_boundaries_are_exact_or_tolerated() {
    let zero = struct_trace("x", vec![0.0]);
    let subnormal = struct_trace("x", vec![f64::from_bits(1)]);
    let signed_zero = struct_trace("x", vec![-0.0]);

    assert!(compare(&zero, &signed_zero, Tolerance::EXACT)
        .unwrap()
        .passed());
    assert!(!compare(&zero, &subnormal, Tolerance::EXACT)
        .unwrap()
        .passed());
    assert!(compare(
        &zero,
        &subnormal,
        Tolerance::new(f64::from_bits(1), 0.0).unwrap()
    )
    .unwrap()
    .passed());
}

#[test]
fn strict_json_schema_rejects_malformed_and_unknown_fields() {
    assert_invalid(
        "{",
        |error| matches!(error, Error::MalformedJson(_)),
        "truncated JSON",
    );

    let valid = trace("primitives-v1", 7, &[case("x", "1.0")]);
    let with_unknown_top_level = append_top_level_field(valid.clone(), ",\"extra\":1");
    assert_invalid(
        &with_unknown_top_level,
        |error| matches!(error, Error::MalformedJson(_)),
        "unknown top-level field",
    );

    let with_unknown_case_field = trace(
        "primitives-v1",
        7,
        &[r#"{"id":"x","values":[1.0],"extra":1}"#.to_owned()],
    );
    assert_invalid(
        &with_unknown_case_field,
        |error| matches!(error, Error::MalformedJson(_)),
        "unknown case field",
    );

    let missing_cases = format!(
        r#"{{"schema_version":1,"upstream_commit":"{COMMIT}","suite":"primitives-v1","seed":7}}"#
    );
    assert_invalid(
        &missing_cases,
        |error| matches!(error, Error::MalformedJson(_)),
        "missing cases field",
    );
    assert_invalid(
        &valid.replacen("\"schema_version\":1", "\"schema_version\":1.0", 1),
        |error| matches!(error, Error::MalformedJson(_)),
        "non-integer schema version",
    );
    assert_invalid(
        &valid.replacen("\"seed\":7", "\"seed\":-1", 1),
        |error| matches!(error, Error::MalformedJson(_)),
        "negative seed",
    );
}

#[test]
fn trace_metadata_and_identity_are_validated() {
    let base = trace("primitives-v1", 7, &[case("x", "1.0")]);
    let uppercase = base.replace(COMMIT, &COMMIT.to_uppercase());
    assert_invalid(
        &uppercase,
        |error| matches!(error, Error::InvalidUpstreamCommit(_)),
        "uppercase commit",
    );
    assert_invalid(
        &trace_with_commit("abc", "primitives-v1", 7, &[case("x", "1.0")]),
        |error| matches!(error, Error::InvalidUpstreamCommit(_)),
        "short commit",
    );
    assert_invalid(
        &trace_with_commit(&"g".repeat(40), "primitives-v1", 7, &[case("x", "1.0")]),
        |error| matches!(error, Error::InvalidUpstreamCommit(_)),
        "non-hex commit",
    );
    assert_invalid(
        &trace("", 7, &[case("x", "1.0")]),
        |error| matches!(error, Error::EmptySuite),
        "empty suite",
    );
    assert_invalid(
        &base.replacen("\"schema_version\":1", "\"schema_version\":2", 1),
        |error| matches!(error, Error::UnsupportedSchemaVersion(2)),
        "unsupported schema version",
    );
}

#[test]
fn case_structure_is_validated() {
    assert_invalid(
        &trace("primitives-v1", 7, &[]),
        |error| matches!(error, Error::EmptyCases),
        "empty cases",
    );
    assert_invalid(
        &trace("primitives-v1", 7, &[case("", "1.0")]),
        |error| matches!(error, Error::EmptyCaseId { index: 0 }),
        "empty case id",
    );
    assert_invalid(
        &trace("primitives-v1", 7, &[case("x", "1.0"), case("x", "2.0")]),
        |error| matches!(error, Error::DuplicateCaseId(id) if id == "x"),
        "duplicate case id",
    );
    assert_invalid(
        &trace("primitives-v1", 7, &[case("x", "")]),
        |error| matches!(error, Error::EmptyValues { case } if case == "x"),
        "empty values",
    );
}

#[test]
fn nonfinite_or_out_of_range_numbers_are_rejected() {
    for (label, values) in [
        ("NaN", "NaN"),
        ("Infinity", "Infinity"),
        ("negative Infinity", "-Infinity"),
        ("overflowing exponent", "1e400"),
    ] {
        assert_invalid(
            &trace("primitives-v1", 7, &[case("x", values)]),
            |error| {
                matches!(
                    error,
                    Error::MalformedJson(_) | Error::NonFiniteValue { .. }
                )
            },
            label,
        );
    }
}

#[test]
fn compare_validates_structurally_constructed_nonfinite_values() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let trace = struct_trace("x", vec![value]);
        assert!(matches!(
            compare(&trace, &trace, Tolerance::EXACT),
            Err(Error::NonFiniteValue { case, index: 0 }) if case == "x"
        ));
    }
}

#[test]
fn comparison_requires_matching_metadata() {
    let reference = parse(&trace("primitives-v1", 7, &[case("x", "1.0")]));
    let cases = &[case("x", "1.0")];
    let candidates = [
        (
            "upstream_commit",
            trace_with_commit(&"a".repeat(40), "primitives-v1", 7, cases),
        ),
        ("suite", trace("other-suite", 7, cases)),
        ("seed", trace("primitives-v1", 8, cases)),
    ];

    for (field, json) in candidates {
        match compare(&reference, &parse(&json), Tolerance::EXACT) {
            Err(Error::MetadataMismatch { field: actual, .. }) => assert_eq!(actual, field),
            other => panic!("expected {field} metadata mismatch, got {other:?}"),
        }
    }
}

#[test]
fn comparison_requires_exact_case_sets() {
    let cases_a = [case("a", "1.0")];
    let cases_ab = [case("a", "1.0"), case("b", "2.0")];
    let cases_b = [case("b", "2.0")];

    let reference_ab = parse(&trace("primitives-v1", 7, &cases_ab));
    match compare(
        &reference_ab,
        &parse(&trace("primitives-v1", 7, &cases_a)),
        Tolerance::EXACT,
    ) {
        Err(Error::CaseSetMismatch {
            missing_in_candidate,
            extra_in_candidate,
        }) => {
            assert_eq!(missing_in_candidate, vec!["b"]);
            assert!(extra_in_candidate.is_empty());
        }
        other => panic!("expected a missing case, got {other:?}"),
    }

    let reference_a = parse(&trace("primitives-v1", 7, &cases_a));
    match compare(
        &reference_a,
        &parse(&trace("primitives-v1", 7, &cases_ab)),
        Tolerance::EXACT,
    ) {
        Err(Error::CaseSetMismatch {
            missing_in_candidate,
            extra_in_candidate,
        }) => {
            assert!(missing_in_candidate.is_empty());
            assert_eq!(extra_in_candidate, vec!["b"]);
        }
        other => panic!("expected an extra case, got {other:?}"),
    }

    match compare(
        &reference_a,
        &parse(&trace("primitives-v1", 7, &cases_b)),
        Tolerance::EXACT,
    ) {
        Err(Error::CaseSetMismatch {
            missing_in_candidate,
            extra_in_candidate,
        }) => {
            assert_eq!(missing_in_candidate, vec!["a"]);
            assert_eq!(extra_in_candidate, vec!["b"]);
        }
        other => panic!("expected a replaced case, got {other:?}"),
    }
}

#[test]
fn comparison_requires_matching_dimensions() {
    let reference = parse(&trace("primitives-v1", 7, &[case("a", "1.0,2.0")]));
    let candidate = parse(&trace("primitives-v1", 7, &[case("a", "1.0")]));

    match compare(&reference, &candidate, Tolerance::EXACT) {
        Err(Error::DimensionMismatch {
            case,
            reference: reference_dimension,
            candidate: candidate_dimension,
        }) => {
            assert_eq!(case, "a");
            assert_eq!(reference_dimension, 2);
            assert_eq!(candidate_dimension, 1);
        }
        other => panic!("expected a dimension mismatch, got {other:?}"),
    }
}
