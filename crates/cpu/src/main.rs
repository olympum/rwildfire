use rwildfire_cpu::{render, scene::parse};
use std::{
    env,
    fs::{self, OpenOptions},
    io::BufWriter,
    process::ExitCode,
};

const USAGE: &str = "Usage: rwildfire-cpu INPUT.flame OUTPUT.png [SAMPLES_PER_PIXEL]\nSingle-thread CPU preview. Writes a new PNG at twice the scene size. Default: 100 samples/pixel.";

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.iter().any(|v| v == "--help" || v == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let quality = match args.get(2) {
        None => Some(100),
        Some(v) => v.to_str().and_then(|v| v.parse().ok()).filter(|v| *v > 0),
    };
    if let (2..=3, Some(quality)) = (args.len(), quality) {
        match run(&args[0], &args[1], quality) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("rwildfire-cpu: {err}");
                ExitCode::FAILURE
            }
        }
    } else {
        eprintln!("{USAGE}");
        ExitCode::from(2)
    }
}

fn run(
    input: &std::ffi::OsStr,
    output: &std::ffi::OsStr,
    quality: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let scene = parse(&fs::read_to_string(input)?)?;
    let width = scene.width.checked_mul(2).ok_or("image width overflow")?;
    let height = scene.height.checked_mul(2).ok_or("image height overflow")?;
    let pixels = render(&scene, width, height, quality)?;
    let mut png = png::Encoder::new(
        BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?,
        ),
        width as u32,
        height as u32,
    );
    png.set_color(png::ColorType::Rgb);
    png.set_depth(png::BitDepth::Eight);
    let mut writer = png.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    Ok(())
}
