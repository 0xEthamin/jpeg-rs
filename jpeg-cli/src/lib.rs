//! # jpeg-cli
//!
//! Library portion of the command-line JPEG encoder.
//!
//! The encoding logic and argument parsing live here so they can be
//! tested independently of the binary entry point.

use std::fs::{self, File};
use std::path::Path;

use jpeg_core::{ColorSpace, EncoderConfig, RawImage, Subsampling};
use jpeg_io::ppm;

/// Parsed command-line options.
#[derive(Debug)]
pub struct Options
{
    /// Path to the input PPM file.
    pub input: String,

    /// Path to the output JPEG file.
    pub output: String,

    /// Quality factor (1–100).
    pub quality: u8,

    /// Chroma subsampling mode.
    pub subsampling: Subsampling,

    /// Restart interval in MCUs (0 = disabled).
    pub restart_interval: u16,
}

/// Parse a slice of command-line arguments (excluding the program name).
///
/// # Errors
///
/// Returns a human-readable error string for any invalid or missing
/// argument.
pub fn parse_args(args: &[String]) -> Result<Options, String>
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
                output = Some
                (
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
                subsampling = match args.get(i).map(String::as_str)
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
                return Err("__help__".into());
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

/// Execute the encode pipeline: read PPM -> encode JPEG -> write to disk.
///
/// # Errors
///
/// Returns any I/O, PPM parsing, or JPEG encoding error.
pub fn run(opts: &Options) -> Result<(), Box<dyn std::error::Error>>
{
    println!("Reading {} ...", opts.input);
    let file = File::open(&opts.input)?;
    let ppm_image = ppm::read_ppm(file)?;

    println!
    (
        " {}x{} pixels, {} bytes",
        ppm_image.width, ppm_image.height, ppm_image.data.len(),
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
        opts.quality, opts.subsampling, opts.restart_interval,
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

/// Return the usage/help text.
#[must_use]
pub fn usage_text() -> &'static str
{
    "Usage: jpeg-encode <input.ppm> [options]\n\
     \n\
     Options:\n\
     \x20 -o, --output <file>       Output JPEG file (default: input.jpg)\n\
     \x20 -q, --quality <1-100>     Quality factor (default: 85)\n\
     \x20 -s, --subsampling <mode>  Chroma subsampling: 444, 422, 420 (default: 420)\n\
     \x20 -r, --restart <n>         Restart interval in MCUs (default: 0 = disabled)\n\
     \x20 -h, --help                Show this help"
}

#[cfg(test)]
mod tests
{
    use super::*;

    /// Helper: turn a &[&str] into Vec<String> for parse_args.
    fn args(strs: &[&str]) -> Vec<String>
    {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_minimal_args()
    {
        let opts = parse_args(&args(&["photo.ppm"])).unwrap();
        assert_eq!(opts.input, "photo.ppm");
        assert_eq!(opts.output, "photo.jpg");
        assert_eq!(opts.quality, 85);
        assert_eq!(opts.subsampling, Subsampling::S420);
        assert_eq!(opts.restart_interval, 0);
    }

    #[test]
    fn parse_output_short()
    {
        let opts = parse_args(&args(&["in.ppm", "-o", "out.jpg"])).unwrap();
        assert_eq!(opts.output, "out.jpg");
    }

    #[test]
    fn parse_output_long()
    {
        let opts = parse_args(&args(&["in.ppm", "--output", "out.jpg"])).unwrap();
        assert_eq!(opts.output, "out.jpg");
    }

    #[test]
    fn parse_output_missing_value()
    {
        let result = parse_args(&args(&["in.ppm", "-o"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires"));
    }

    #[test]
    fn parse_quality_short()
    {
        let opts = parse_args(&args(&["in.ppm", "-q", "50"])).unwrap();
        assert_eq!(opts.quality, 50);
    }

    #[test]
    fn parse_quality_long()
    {
        let opts = parse_args(&args(&["in.ppm", "--quality", "100"])).unwrap();
        assert_eq!(opts.quality, 100);
    }

    #[test]
    fn parse_quality_zero_rejected()
    {
        let result = parse_args(&args(&["in.ppm", "-q", "0"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_quality_101_rejected()
    {
        let result = parse_args(&args(&["in.ppm", "-q", "101"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_quality_non_numeric()
    {
        let result = parse_args(&args(&["in.ppm", "-q", "abc"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_quality_missing_value()
    {
        let result = parse_args(&args(&["in.ppm", "-q"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_subsampling_444()
    {
        let opts = parse_args(&args(&["in.ppm", "-s", "444"])).unwrap();
        assert_eq!(opts.subsampling, Subsampling::S444);
    }

    #[test]
    fn parse_subsampling_422()
    {
        let opts = parse_args(&args(&["in.ppm", "-s", "422"])).unwrap();
        assert_eq!(opts.subsampling, Subsampling::S422);
    }

    #[test]
    fn parse_subsampling_420()
    {
        let opts = parse_args(&args(&["in.ppm", "--subsampling", "420"])).unwrap();
        assert_eq!(opts.subsampling, Subsampling::S420);
    }

    #[test]
    fn parse_subsampling_invalid()
    {
        let result = parse_args(&args(&["in.ppm", "-s", "411"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_subsampling_missing_value()
    {
        let result = parse_args(&args(&["in.ppm", "-s"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_restart_short()
    {
        let opts = parse_args(&args(&["in.ppm", "-r", "10"])).unwrap();
        assert_eq!(opts.restart_interval, 10);
    }

    #[test]
    fn parse_restart_long()
    {
        let opts = parse_args(&args(&["in.ppm", "--restart", "100"])).unwrap();
        assert_eq!(opts.restart_interval, 100);
    }

    #[test]
    fn parse_restart_missing_value()
    {
        let result = parse_args(&args(&["in.ppm", "-r"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_restart_non_numeric()
    {
        let result = parse_args(&args(&["in.ppm", "-r", "xyz"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_help_short()
    {
        let result = parse_args(&args(&["-h"]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "__help__");
    }

    #[test]
    fn parse_help_long()
    {
        let result = parse_args(&args(&["--help"]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "__help__");
    }

    #[test]
    fn parse_no_input()
    {
        let result = parse_args(&args(&[]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no input"));
    }

    #[test]
    fn parse_unknown_option()
    {
        let result = parse_args(&args(&["in.ppm", "--verbose"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown option"));
    }

    #[test]
    fn parse_multiple_inputs_rejected()
    {
        let result = parse_args(&args(&["a.ppm", "b.ppm"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("multiple"));
    }

    #[test]
    fn parse_all_flags_combined()
    {
        let opts = parse_args(&args(&[
            "photo.ppm", "-o", "out.jpg", "-q", "75",
            "-s", "444", "-r", "5",
        ])).unwrap();
        assert_eq!(opts.input, "photo.ppm");
        assert_eq!(opts.output, "out.jpg");
        assert_eq!(opts.quality, 75);
        assert_eq!(opts.subsampling, Subsampling::S444);
        assert_eq!(opts.restart_interval, 5);
    }

    #[test]
    fn default_output_replaces_extension()
    {
        let opts = parse_args(&args(&["image.ppm"])).unwrap();
        assert_eq!(opts.output, "image.jpg");
    }

    #[test]
    fn default_output_with_path()
    {
        let opts = parse_args(&args(&["/tmp/dir/image.ppm"])).unwrap();
        assert_eq!(opts.output, "/tmp/dir/image.jpg");
    }

    #[test]
    fn usage_text_contains_options()
    {
        let text = usage_text();
        assert!(text.contains("--quality"));
        assert!(text.contains("--output"));
        assert!(text.contains("--subsampling"));
        assert!(text.contains("--restart"));
        assert!(text.contains("--help"));
    }
}