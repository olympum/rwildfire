mod support;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn flame_becomes_new_png_without_clobbering() {
    let dir = std::env::temp_dir().join(format!(
        "rwildfire-cpu-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let input = dir.join("original.flame");
    let output = dir.join("preview.png");
    fs::write(&input, support::flame(false)).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_rwildfire-cpu"))
            .arg(&input)
            .arg(&output)
            .arg("1")
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let png = fs::read(&output).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
        .read_info()
        .unwrap();
    assert_eq!((reader.info().width, reader.info().height), (80, 60));
    let mut rgb = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut rgb).unwrap();
    assert!(rgb.iter().any(|&v| v > 24));
    assert!(!run().status.success());
    assert_eq!(fs::read(&output).unwrap(), png);
    fs::remove_dir_all(&dir).unwrap();
}
