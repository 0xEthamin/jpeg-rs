//! # Color
//!
//! RGB -> YCbCr color space conversion.
//!
//! # Why convert to YCbCr?
//!
//! JPEG achieves much of its compression by exploiting a property of human
//! vision: we perceive fine *brightness* detail much more acutely than fine
//! *color* detail. The YCbCr color model separates an image into:
//!
//!   - **Y** (luminance) - perceived brightness
//!   - **Cb** (blue-difference chrominance) - how "blue vs. yellow" a pixel is
//!   - **Cr** (red-difference chrominance) - how "red vs. cyan" a pixel is
//!
//! Once separated, the Cb and Cr planes can be *subsampled* (see
//! [`crate::sampling`]) - stored at a lower resolution - before they are
//! compressed. This alone can remove 50 % of the color data with negligible
//! visual impact.
//!
//! # Conversion formulae
//!
//! The coefficients below implement the CCIR 601 / BT.601 transform, which is
//! the standard assumed by the JFIF file format (JFIF v1.02, §4):
//!
//! ```text
//!   Y = 0.299*R + 0.587*G + 0.114*B
//!   Cb =  -0.16874*R - 0.33126*G + 0.5*B + 128
//!   Cr = 0.5*R - 0.41869*G - 0.08131*B + 128
//! ```
//!
//! The +128 offset on Cb and Cr shifts the signed range [-128, 127] to the
//! unsigned range [0, 255], matching what the encoder expects as unsigned
//! 8-bit samples before the level shift (T.81 §A.3.1 subtracts 2^(P-1) = 128
//! to obtain a signed representation).
//!
//! # Fixed-point implementation
//!
//! Floating-point conversion would be precise but slow for pixel-level work.
//! Instead, each coefficient is scaled by 2^16 = 65536 and the computation
//! is performed entirely in integer arithmetic. A single right-shift by 16
//! (with rounding) recovers the 8-bit result.

/// Bits of fractional precision in the fixed-point coefficients.
const FP_SHIFT: i32 = 16;

/// Rounding bias: 0.5 in fixed-point = 2^(FP_SHIFT - 1).
const FP_HALF: i32 = 1 << (FP_SHIFT - 1);

// Luminance (Y) coefficients - represent the row [0.299, 0.587, 0.114].
const YR: i32 = 19595; // round(0.299 * 65536)
const YG: i32 = 38470; // round(0.587 * 65536)
const YB: i32 = 7471;  // round(0.114 * 65536)

// Blue-difference chrominance (Cb) coefficients.
const CBR: i32 = -11059; // round( -0.16874 * 65536)
const CBG: i32 = -21709; // round( -0.33126 * 65536)
const CBB: i32 =  32768; // round( 0.5 * 65536)

// Red-difference chrominance (Cr) coefficients.
const CRR: i32 =  32768; // round( 0.5 * 65536)
const CRG: i32 = -27439; // round( -0.41869 * 65536)
const CRB: i32 =  -5329; // round( -0.08131 * 65536)

/// Clamp an integer to the unsigned 8-bit range [0, 255].
#[inline(always)]
fn clamp_u8(v: i32) -> u8
{
    v.clamp(0, 255) as u8
}

/// Convert a single RGB pixel to YCbCr.
///
/// Returns `(Y, Cb, Cr)` where each value is in the range [0, 255].
#[inline]
pub fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8)
{
    let ri = r as i32;
    let gi = g as i32;
    let bi = b as i32;

    let y = (YR * ri + YG * gi + YB * bi + FP_HALF) >> FP_SHIFT;
    let cb = (CBR * ri + CBG * gi + CBB * bi + FP_HALF) >> FP_SHIFT;
    let cr = (CRR * ri + CRG * gi + CRB * bi + FP_HALF) >> FP_SHIFT;

    (
        clamp_u8(y),
        clamp_u8(cb + 128),
        clamp_u8(cr + 128),
    )
}

/// Convert an interleaved RGB buffer to three separate YCbCr planes.
///
/// # Arguments
///
/// * `rgb` - packed RGB bytes: `[R0, G0, B0, R1, G1, B1, …]`
/// * `width`, `height` - image dimensions.
///
/// # Returns
///
/// Three `Vec<u8>` of length `width * height`, one per component:
/// `(y_plane, cb_plane, cr_plane)`.
///
/// Each plane stores samples in row-major order (left-to-right,
/// top-to-bottom), matching the orientation defined in T.81 §A.1.4.
pub fn rgb_to_ycbcr_planar
(
    rgb: &[u8],
    width: u32,
    height: u32,
) -> (Vec<u8>, Vec<u8>, Vec<u8>)
{
    let num_pixels = (width as usize) * (height as usize);

    let mut y_plane = Vec::with_capacity(num_pixels);
    let mut cb_plane = Vec::with_capacity(num_pixels);
    let mut cr_plane = Vec::with_capacity(num_pixels);

    // Step through the buffer three bytes at a time.
    let mut i = 0;
    while i + 2 < rgb.len()
    {
        let (y, cb, cr) = rgb_to_ycbcr(rgb[i], rgb[i + 1], rgb[i + 2]);
        y_plane.push(y);
        cb_plane.push(cb);
        cr_plane.push(cr);
        i += 3;
    }

    (y_plane, cb_plane, cr_plane)
}

/// Treat a grayscale buffer as a Y (luminance) plane with no conversion.
///
/// Grayscale images consist of a single component that is already luminance
/// data, so no transform is needed.
pub fn grayscale_to_y_plane(gray: &[u8]) -> Vec<u8>
{
    gray.to_vec()
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn pure_black()
    {
        let (y, cb, cr) = rgb_to_ycbcr(0, 0, 0);
        assert_eq!(y, 0);
        assert_eq!(cb, 128);
        assert_eq!(cr, 128);
    }

    #[test]
    fn pure_white()
    {
        let (y, cb, cr) = rgb_to_ycbcr(255, 255, 255);
        assert_eq!(y, 255);
        assert_eq!(cb, 128);
        assert_eq!(cr, 128);
    }

    #[test]
    fn pure_red()
    {
        let (y, cb, cr) = rgb_to_ycbcr(255, 0, 0);
        assert!((y as i16 - 76).abs() <= 1, "Y={y}");
        assert!((cb as i16 - 85).abs() <= 1, "Cb={cb}");
        assert_eq!(cr, 255);
    }

    #[test]
    fn pure_green()
    {
        let (y, cb, cr) = rgb_to_ycbcr(0, 255, 0);
        assert!((y as i16 - 150).abs() <= 1, "Y={y}");
        assert!((cb as i16 - 44).abs() <= 1, "Cb={cb}");
        assert!((cr as i16 - 21).abs() <= 1, "Cr={cr}");
    }

    #[test]
    fn pure_blue()
    {
        let (y, cb, cr) = rgb_to_ycbcr(0, 0, 255);
        assert!((y as i16 - 29).abs() <= 1, "Y={y}");
        assert_eq!(cb, 255);
        assert!((cr as i16 - 107).abs() <= 1, "Cr={cr}");
    }

    #[test]
    fn neutral_gray_has_no_chrominance()
    {
        let (y, cb, cr) = rgb_to_ycbcr(128, 128, 128);
        assert_eq!(y, 128);
        assert_eq!(cb, 128);
        assert_eq!(cr, 128);
    }

    #[test]
    fn output_always_in_u8_range()
    {
        for &r in &[0u8, 1, 127, 128, 254, 255]
        {
            for &g in &[0u8, 1, 127, 128, 254, 255]
            {
                for &b in &[0u8, 1, 127, 128, 254, 255]
                {
                    let (_y, _cb, _cr) = rgb_to_ycbcr(r, g, b);
                }
            }
        }
    }

    #[test]
    fn planar_correct_length()
    {
        let rgb = vec![128u8; 10 * 5 * 3];
        let (y, cb, cr) = rgb_to_ycbcr_planar(&rgb, 10, 5);
        assert_eq!(y.len(), 50);
        assert_eq!(cb.len(), 50);
        assert_eq!(cr.len(), 50);
    }

    #[test]
    fn planar_matches_per_pixel()
    {
        let rgb = [200u8, 100, 50, 10, 20, 30];
        let (y, cb, cr) = rgb_to_ycbcr_planar(&rgb, 2, 1);
        let (y0, cb0, cr0) = rgb_to_ycbcr(200, 100, 50);
        let (y1, cb1, cr1) = rgb_to_ycbcr(10, 20, 30);
        assert_eq!((y[0], cb[0], cr[0]), (y0, cb0, cr0));
        assert_eq!((y[1], cb[1], cr[1]), (y1, cb1, cr1));
    }

    #[test]
    fn grayscale_is_identity()
    {
        let gray = vec![0u8, 64, 128, 192, 255];
        assert_eq!(grayscale_to_y_plane(&gray), gray);
    }
}
