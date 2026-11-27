use rwildfire_cpu::{scene::parse, Antialias, RenderRequest};
use std::{
    error::Error,
    fs,
    io::{BufWriter, Write},
    path::Path,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [flame, prefix, width, height, quality, seed, amount, radius] = args.as_slice() else {
        return Err("expected FLAME PREFIX WIDTH HEIGHT QUALITY SEED AMOUNT RADIUS".into());
    };
    let scene = parse(&fs::read_to_string(flame)?)?;
    let mut request = RenderRequest::new(
        width.parse()?,
        height.parse()?,
        quality.parse()?,
        seed.parse()?,
    );
    request.antialias = Antialias {
        amount: amount.parse()?,
        radius: radius.parse()?,
    };
    let job = request.resolve(&scene)?;
    let settings = job.settings();
    let mut output = BufWriter::new(fs::File::create(
        Path::new(prefix).with_extension("raster"),
    )?);
    for dimension in [settings.raster_width, settings.raster_height] {
        output.write_all(&u32::try_from(dimension)?.to_be_bytes())?;
    }
    let capture = |bins: &[rwildfire_cpu::Bin]| {
        let write = || -> std::io::Result<()> {
            for bin in bins {
                output.write_all(&bin.count.to_be_bytes())?;
                for channel in bin.rgb {
                    output.write_all(&channel.to_be_bytes())?;
                }
            }
            output.flush()
        };
        let mut write = write;
        write().map_err(|err| err.to_string())
    };
    let (extension, pixels) = if scene.bg_transparency {
        ("rgba", job.render_rgba_with_raster(capture)?)
    } else {
        ("rgb", job.render_with_raster(capture)?)
    };
    fs::write(Path::new(prefix).with_extension(extension), pixels)?;
    Ok(())
}
