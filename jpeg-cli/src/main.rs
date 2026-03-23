//! Binary entry point for the JPEG encoder CLI.
//!
//! This is a thin wrapper around [`jpeg_cli`]. All logic lives in the
//! library crate so it can be tested.

use std::env;
use std::process;

use jpeg_cli::{parse_args, run, usage_text};

fn main()
{
    let args: Vec<String> = env::args().collect();

    if args.len() < 2
    {
        eprintln!("{}", usage_text());
        process::exit(1);
    }

    let opts = match parse_args(&args[1..])
    {
        Ok(o) => o,
        Err(e) if e == "__help__" =>
        {
            println!("{}", usage_text());
            process::exit(0);
        }
        Err(e) =>
        {
            eprintln!("Error: {}\n", e);
            eprintln!("{}", usage_text());
            process::exit(1);
        }
    };

    if let Err(e) = run(&opts)
    {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}