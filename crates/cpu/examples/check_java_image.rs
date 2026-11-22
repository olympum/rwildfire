use rwildfire_cpu::{render_with_raster, scene::parse};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::Path,
    process::ExitCode,
};

type Image = ([u32; 2], Vec<u8>);

fn write_png(path: &Path, [w, h]: [u32; 2], pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    writer.finish()?;
    Ok(())
}

fn read_png(path: &Path) -> Result<Image, Box<dyn Error>> {
    let mut reader = png::Decoder::new(BufReader::new(File::open(path)?)).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    if info.color_type != png::ColorType::Rgb || info.bit_depth != png::BitDepth::Eight {
        return Err("expected an 8-bit RGB PNG".into());
    }
    pixels.truncate(info.buffer_size());
    Ok(([info.width, info.height], pixels))
}

fn capture(
    flame: &Path,
    work: &Path,
    quality: u32,
    unfiltered: bool,
) -> Result<(), Box<dyn Error>> {
    let mut scene = parse(&fs::read_to_string(flame)?)?;
    if unfiltered {
        scene.filter_radius = 0.0;
    }
    let size = [(scene.width * 2) as u32, (scene.height * 2) as u32];
    let mut raster = BufWriter::new(File::create(work.join("raster.bin"))?);
    for value in [size[0], size[1], quality] {
        raster.write_all(&value.to_be_bytes())?;
    }
    let pixels = render_with_raster(
        &scene,
        size[0] as usize,
        size[1] as usize,
        quality as usize,
        |bins| {
            for bin in bins {
                raster
                    .write_all(&bin.count.to_be_bytes())
                    .map_err(|e| e.to_string())?;
                for sum in bin.rgb {
                    raster
                        .write_all(&sum.to_be_bytes())
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        },
    )?;
    raster.flush()?;
    write_png(&work.join("rust.png"), size, &pixels)
}

fn compare(work: &Path, mode: &str) -> Result<bool, Box<dyn Error>> {
    let mutate = mode == "mutate";
    let omit = mode == "omit";
    let (size, reference) =
        read_png(&work.join(if omit { "java-omitted.png" } else { "java.png" }))?;
    let (candidate_size, mut candidate) = read_png(&work.join("rust.png"))?;
    if size != candidate_size
        || reference.len() != candidate.len()
        || !reference.iter().any(|&v| v != 0)
    {
        return Err("expected matching dimensions and a nonblack reference".into());
    }
    if mutate {
        candidate[0] ^= 1;
    }
    let diff: Vec<_> = reference
        .iter()
        .zip(candidate)
        .map(|(&a, b)| a.abs_diff(b))
        .collect();
    let changed = diff
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|rgb| **rgb != [0; 3])
        .count();
    write_png(
        &work.join(if omit {
            "omitted-diff.png"
        } else if mutate {
            "mutant-diff.png"
        } else {
            "diff.png"
        }),
        size,
        &diff,
    )?;
    println!(
        "{}: {} of {} RGB pixels differ ({}×{})",
        if omit {
            "omitted filter"
        } else if mutate {
            "mutant"
        } else {
            "rust"
        },
        changed,
        reference.len() / 3,
        size[0],
        size[1]
    );
    if mutate && changed != 1 {
        return Err("negative control should differ at exactly one pixel".into());
    }
    Ok(changed == 0)
}

fn run() -> Result<bool, Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [mode, flame, work, quality] if mode == "filtered" || mode == "unfiltered" => {
            let quality = quality.to_str().ok_or("invalid quality")?.parse()?;
            capture(Path::new(flame), Path::new(work), quality, mode == "unfiltered")?;
            Ok(true)
        }
        [mode, work] if mode == "compare" || mode == "mutate" || mode == "omit" => {
            compare(Path::new(work), mode.to_str().unwrap())
        }
        _ => Err(
            "usage: check_java_image filtered|unfiltered FLAME WORK_DIR QUALITY | compare|mutate|omit WORK_DIR"
                .into(),
        ),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("check_java_image: {err}");
            ExitCode::from(2)
        }
    }
}
