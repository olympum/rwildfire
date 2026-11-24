mod support;
use rwildfire_cpu::{render, scene::parse};

#[test]
fn unsupported_variation_is_not_silently_discarded() {
    let modified = support::flame(false).replacen("linear3D=", "swirl3D=", 1);
    let Err(error) = parse(&modified) else {
        panic!("unsupported variation was accepted");
    };
    assert!(error.contains("swirl3D"), "{error}");
}

#[test]
fn spherical_variation_changes_a_deterministic_preview() {
    let run = |spherical| render(&parse(&support::flame(spherical)).unwrap(), 80, 60, 16).unwrap();
    let linear = run(false);
    let spherical = run(true);
    for image in [&linear, &spherical] {
        assert_eq!(image.len(), 80 * 60 * 3);
        assert!(
            image
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|p| p.iter().any(|&c| c > 24))
                .count()
                > 80
        );
    }
    assert_eq!(spherical, run(true));
    assert_ne!(linear, spherical);
}
