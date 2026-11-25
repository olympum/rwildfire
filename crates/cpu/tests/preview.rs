mod support;
use rwildfire_cpu::{render, scene::parse, RenderRequest, PREVIEW_SEED};

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

#[test]
fn resolved_still_reports_padding_and_starts_a_fresh_job_per_render() {
    let mut scene = parse(&support::flame(true)).unwrap();
    scene.filter_radius = 1.0;
    let request = RenderRequest::new(80, 60, 16, PREVIEW_SEED);
    let still = request.resolve(&scene).unwrap();
    let settings = still.settings();
    assert_eq!(settings.request, request);
    assert_eq!(
        (
            settings.raster_width,
            settings.raster_height,
            settings.border
        ),
        (84, 64, 2)
    );
    assert_eq!(settings.sample_budget, 86_016);
    assert_eq!(settings.filter_radius, 1.0);
    let first = still.render().unwrap();
    let other = RenderRequest::new(80, 60, 16, 42)
        .resolve(&scene)
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(first.len(), 80 * 60 * 3);
    assert_ne!(first, other, "the render job must honor the supplied seed");
    assert_eq!(first, still.render().unwrap());
}

#[test]
fn resolution_rejects_oversized_jobs_before_rendering() {
    let scene = parse(&support::flame(false)).unwrap();
    for request in [
        RenderRequest::new(0, 60, 16, PREVIEW_SEED),
        RenderRequest::new(80, 60, 0, PREVIEW_SEED),
        RenderRequest::new(932, 662, 500, PREVIEW_SEED),
        RenderRequest::new(80, 60, usize::MAX, PREVIEW_SEED),
    ] {
        assert!(request.resolve(&scene).is_err(), "accepted {request:?}");
    }
}
