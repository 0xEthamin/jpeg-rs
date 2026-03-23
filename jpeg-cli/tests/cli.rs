//! Integration tests for the `jpeg-encode` binary.
//!
//! These tests invoke the compiled binary as a subprocess, exercising
//! the full pipeline from PPM file to JPEG output, including argument
//! parsing, error reporting, and exit codes.

use std::fs;
use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::{NamedTempFile, TempDir};

/// Create a valid P6 PPM file (16×16 gradient) with enough variance
/// to exercise all Huffman table paths.
fn create_test_ppm() -> NamedTempFile
{
    let mut f = NamedTempFile::with_suffix(".ppm").unwrap();
    let w: u32 = 16;
    let h: u32 = 16;
    f.write_all(format!("P6\n{} {}\n255\n", w, h).as_bytes()).unwrap();
    for y in 0..h
    {
        for x in 0..w
        {
            let r = (x * 255 / (w - 1)) as u8;
            let g = (y * 255 / (h - 1)) as u8;
            let b = 128u8;
            f.write_all(&[r, g, b]).unwrap();
        }
    }
    f.flush().unwrap();
    f
}

/// Get a Command pointing at the jpeg-encode binary.
fn jpeg_encode() -> Command
{
    Command::cargo_bin("jpeg-encode").unwrap()
}

#[test]
fn encode_basic_ppm_to_jpeg()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("output.jpg");

    jpeg_encode()
        .arg(ppm.path())
        .arg("-o")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("Written"));

    // Verify output is a valid JPEG (starts with SOI, ends with EOI).
    let data = fs::read(&output).unwrap();
    assert!(data.len() > 4, "JPEG too small: {} bytes", data.len());
    assert_eq!(&data[0..2], &[0xFF, 0xD8], "missing SOI marker");
    assert_eq!(&data[data.len() - 2..], &[0xFF, 0xD9], "missing EOI marker");
}

#[test]
fn encode_with_quality_flag()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("q50.jpg");

    jpeg_encode()
        .arg(ppm.path())
        .arg("-q").arg("50")
        .arg("-o").arg(&output)
        .assert()
        .success();

    let data = fs::read(&output).unwrap();
    assert_eq!(&data[0..2], &[0xFF, 0xD8]);
}

#[test]
fn encode_with_subsampling_444()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("s444.jpg");

    jpeg_encode()
        .arg(ppm.path())
        .arg("-s").arg("444")
        .arg("-o").arg(&output)
        .assert()
        .success();

    assert!(fs::read(&output).unwrap().len() > 2);
}

#[test]
fn encode_with_restart_interval()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("rst.jpg");

    jpeg_encode()
        .arg(ppm.path())
        .arg("-r").arg("1")
        .arg("-o").arg(&output)
        .assert()
        .success();

    assert!(fs::read(&output).unwrap().len() > 2);
}

#[test]
fn encode_with_all_flags()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("all.jpg");

    jpeg_encode()
        .arg(ppm.path())
        .arg("-o").arg(&output)
        .arg("-q").arg("75")
        .arg("-s").arg("422")
        .arg("-r").arg("4")
        .assert()
        .success();

    let data = fs::read(&output).unwrap();
    assert_eq!(&data[0..2], &[0xFF, 0xD8]);
    assert_eq!(&data[data.len() - 2..], &[0xFF, 0xD9]);
}

#[test]
fn default_output_uses_jpg_extension()
{
    let ppm = create_test_ppm();
    let ppm_path = ppm.path().to_path_buf();

    let expected_output = ppm_path.with_extension("jpg");

    jpeg_encode()
        .arg(&ppm_path)
        .assert()
        .success();

    assert!(expected_output.exists(), "expected {:?} to exist", expected_output);

    let _ = fs::remove_file(&expected_output);
}
#[test]
fn no_arguments_shows_usage_and_fails()
{
    jpeg_encode()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn nonexistent_input_fails()
{
    jpeg_encode()
        .arg("/tmp/nonexistent_file_12345.ppm")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn invalid_quality_zero_fails()
{
    let ppm = create_test_ppm();

    jpeg_encode()
        .arg(ppm.path())
        .arg("-q").arg("0")
        .assert()
        .failure()
        .stderr(predicate::str::contains("quality"));
}

#[test]
fn invalid_quality_over_100_fails()
{
    let ppm = create_test_ppm();

    jpeg_encode()
        .arg(ppm.path())
        .arg("-q").arg("101")
        .assert()
        .failure()
        .stderr(predicate::str::contains("quality"));
}

#[test]
fn invalid_subsampling_fails()
{
    let ppm = create_test_ppm();

    jpeg_encode()
        .arg(ppm.path())
        .arg("-s").arg("411")
        .assert()
        .failure()
        .stderr(predicate::str::contains("subsampling"));
}

#[test]
fn unknown_option_fails()
{
    let ppm = create_test_ppm();

    jpeg_encode()
        .arg(ppm.path())
        .arg("--verbose")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown option"));
}

#[test]
fn help_flag_succeeds()
{
    jpeg_encode()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn help_flag_short_succeeds()
{
    jpeg_encode()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn corrupt_ppm_fails()
{
    let mut f = NamedTempFile::with_suffix(".ppm").unwrap();
    f.write_all(b"this is not a PPM file").unwrap();
    f.flush().unwrap();

    jpeg_encode()
        .arg(f.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn run_with_valid_ppm()
{
    let ppm = create_test_ppm();
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("run_test.jpg");

    let opts = jpeg_cli::Options
    {
        input: ppm.path().to_string_lossy().into_owned(),
        output: output.to_string_lossy().into_owned(),
        quality: 85,
        subsampling: jpeg_core::Subsampling::S420,
        restart_interval: 0,
    };

    jpeg_cli::run(&opts).unwrap();

    let data = fs::read(&output).unwrap();
    assert_eq!(&data[0..2], &[0xFF, 0xD8]);
}

#[test]
fn run_with_nonexistent_input_returns_error()
{
    let opts = jpeg_cli::Options
    {
        input: "/tmp/does_not_exist_98765.ppm".into(),
        output: "/tmp/out.jpg".into(),
        quality: 85,
        subsampling: jpeg_core::Subsampling::S420,
        restart_interval: 0,
    };

    assert!(jpeg_cli::run(&opts).is_err());
}