use std::env;
use std::fs::{self, File};
use std::path::Path;
use std::process;

use jpeg_core::{ColorSpace, EncoderConfig, RawImage, Subsampling};
use jpeg_io::ppm;

fn main()
{
    let args: Vec<String> = env::args().collect();

    if args.len() < 2
    {
        print_usage();
        process::exit(1);
    }

    let opts = match parse_args(&args[1..])
    {
        Ok(o) => o,
        Err(e) =>
        {
            eprintln!("Error: {}", e);
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = run(&opts)
    {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

struct Options
{
    input: String,
    output: String,
    quality: u8,
    subsampling: Subsampling,
    restart_interval: u16,
}

fn parse_args(args: &[String]) -> Result<Options, String>
{
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut quality: u8 = 85;
    let mut subsampling = Subsampling::S420;
    let mut restart_interval: u16 = 0;

    let mut i = 0;
    while i < args.len()
    {
        match args[i].as_str()
        {
            "-o" | "--output" =>
            {
                i += 1;
                output = Some(
                    args.get(i)
                        .ok_or("-o requires a filename")?
                        .clone(),
                );
            }
            "-q" | "--quality" =>
            {
                i += 1;
                let val: u8 = args
                    .get(i)
                    .ok_or("-q requires a value")?
                    .parse()
                    .map_err(|_| "quality must be a number 1-100")?;
                if val == 0 || val > 100
                {
                    return Err("quality must be 1-100".into());
                }
                quality = val;
            }
            "-s" | "--subsampling" =>
            {
                i += 1;
                subsampling = match args
                    .get(i)
                    .map(String::as_str)
                {
                    Some("444") => Subsampling::S444,
                    Some("422") => Subsampling::S422,
                    Some("420") => Subsampling::S420,
                    _ => return Err("subsampling must be 444, 422, or 420".into()),
                };
            }
            "-r" | "--restart" =>
            {
                i += 1;
                restart_interval = args
                    .get(i)
                    .ok_or("-r requires a value")?
                    .parse()
                    .map_err(|_| "restart interval must be a number 0-65535")?;
            }
            "-h" | "--help" =>
            {
                print_usage();
                process::exit(0);
            }
            other =>
            {
                if other.starts_with('-')
                {
                    return Err(format!("unknown option: {}", other));
                }
                if input.is_some()
                {
                    return Err("multiple input files not supported".into());
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }

    let input = input.ok_or("no input file specified")?;

    let output = output.unwrap_or_else(||
    {
        let p = Path::new(&input);
        p.with_extension("jpg")
            .to_string_lossy()
            .into_owned()
    });

    Ok(Options { input, output, quality, subsampling, restart_interval })
}

fn run(opts: &Options) -> Result<(), Box<dyn std::error::Error>>
{
    println!
    (
        "Reading {} ...",
        opts.input,
    );
    let file = File::open(&opts.input)?;
    let ppm_image = ppm::read_ppm(file)?;

    println!(
        "  {}x{} pixels, {} bytes",
        ppm_image.width,
        ppm_image.height,
        ppm_image.data.len(),
    );

    let config = EncoderConfig
    {
        quality: opts.quality,
        subsampling: opts.subsampling,
        restart_interval: opts.restart_interval,
        ..EncoderConfig::default()
    };

    let raw = RawImage
    {
        width: ppm_image.width,
        height: ppm_image.height,
        color_space: ColorSpace::Rgb,
        data: &ppm_image.data,
    };

    println!
    (
        "Encoding JPEG (quality={}, subsampling={:?}, restart={}) ...",
        opts.quality,
        opts.subsampling,
        opts.restart_interval,
    );
    let jpeg_data = jpeg_core::encode(&raw, &config)?;

    fs::write(&opts.output, &jpeg_data)?;
    println!
    (
        "Written {} ({} bytes, ratio {:.1}:1)",
        opts.output,
        jpeg_data.len(),
        ppm_image.data.len() as f64 / jpeg_data.len() as f64,
    );

    Ok(())
}

fn print_usage()
{
    eprintln!
    (
        "Usage: jpeg-encode <input.ppm> [options]\n\
         \n\
         Options:\n\
         \x20 -o, --output <file>       Output JPEG file (default: input.jpg)\n\
         \x20 -q, --quality <1-100>     Quality factor (default: 85)\n\
         \x20 -s, --subsampling <mode>  Chroma subsampling: 444, 422, 420 (default: 420)\n\
         \x20 -r, --restart <n>         Restart interval in MCUs (default: 0 = disabled)\n\
         \x20 -h, --help                Show this help"
    );
}