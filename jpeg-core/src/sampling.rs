//! # Sampling
//! 
//! Chroma subsampling (downsampling of Cb/Cr planes).
//!
//! # What is chroma subsampling?
//!
//! After converting the image from RGB to YCbCr (see [`crate::color`]), the
//! chrominance planes (Cb and Cr) can be reduced in resolution before
//! compression. This exploits the fact that the human visual system
//! resolves color at a lower spatial frequency than luminance.
//!
//! The subsampling is controlled by the *sampling factors* in the frame
//! header (T.81 §A.1.1, §B.2.2). The luminance component (Y) keeps the
//! full resolution. The chrominance components are downsampled by the ratio
//! of their sampling factors to the maximum:
//!
//! ```text
//!   subsampled_width = width * (H_chroma / H_max) = width / h_ratio
//!   subsampled_height = height * (V_chroma / V_max) = height / v_ratio
//! ```
//!
//! # Downsampling filter
//!
//! T.81 does not mandate a specific downsampling filter. That is left to
//! the encoder. This implementation uses a simple box filter (average of
//! the samples within each downsampling cell), which is the most common
//! approach. More sophisticated filters (e.g. Lanczos, windowed sinc)
//! could reduce aliasing but are not required for conformance.

use crate::types::Subsampling;

/// Downsample a single-component plane by integer factors.
///
/// Each output sample is the average (with rounding) of the `h_factor *
/// v_factor` source samples that fall within its cell. Boundary cells
/// that extend past the image edge use only the valid source samples
/// (the count is adjusted accordingly).
///
/// # Arguments
///
/// * `plane` - source samples in row-major order.
/// * `width`, `height` - source dimensions.
/// * `h_factor` - horizontal downsampling ratio (1 = no change, 2 = halve).
/// * `v_factor` - vertical downsampling ratio.
///
/// # Returns
///
/// The downsampled plane. If both factors are 1, this returns a copy of
/// the input with no averaging (the plane is already at full resolution).
pub fn downsample_plane
(
    plane: &[u8],
    width: u32,
    height: u32,
    h_factor: u8,
    v_factor: u8,
) -> Vec<u8>
{
    if h_factor == 1 && v_factor == 1
    {
        return plane.to_vec();
    }

    let h_factor_u32 = h_factor as u32;
    let v_factor_u32 = v_factor as u32;
    let out_width = (width + h_factor_u32 - 1) / h_factor_u32;
    let out_height = (height + v_factor_u32 - 1) / v_factor_u32;

    let mut downsampled_plane = Vec::with_capacity((out_width * out_height) as usize);

    for out_y in 0..out_height
    {
        for out_x in 0..out_width
        {
            let mut sum: u32 = 0;
            let mut valid_sample_count: u32 = 0;

            for offset_y in 0..v_factor_u32
            {
                let src_y = out_y * v_factor_u32 + offset_y;
                if src_y >= height { continue; }

                for offset_x in 0..h_factor_u32
                {
                    let src_x = out_x * h_factor_u32 + offset_x;
                    if src_x >= width { continue; }

                    sum += plane[(src_y * width + src_x) as usize] as u32;
                    valid_sample_count += 1;
                }
            }

            // Round to nearest: (sum + count/2) / count.
            let avg = ((sum + valid_sample_count / 2) / valid_sample_count) as u8;
            downsampled_plane.push(avg);
        }
    }

    downsampled_plane
}

/// Subsample both chrominance planes according to the given subsampling mode.
///
/// # Returns
///
/// A tuple `(cb_sub, cb_w, cb_h, cr_sub, cr_w, cr_h)` where:
///
/// * `cb_sub` / `cr_sub` - the downsampled Cb / Cr planes.
/// * `cb_w`, `cb_h` / `cr_w`, `cr_h` - the dimensions of each subsampled
///   plane.
///
/// For [`Subsampling::S444`] (no subsampling), the output dimensions and
/// data are identical to the input.
pub fn subsample_chroma
(
    cb: &[u8],
    cr: &[u8],
    width: u32,
    height: u32,
    subsampling: Subsampling,
) -> (Vec<u8>, u32, u32, Vec<u8>, u32, u32)
{
    let (_, _, h_cb, v_cb, _, _) = subsampling.factors();
    let h_max = subsampling.h_max();
    let v_max = subsampling.v_max();

    let h_ratio = h_max / h_cb;
    let v_ratio = v_max / v_cb;

    let cb_sub = downsample_plane(cb, width, height, h_ratio, v_ratio);
    let cr_sub = downsample_plane(cr, width, height, h_ratio, v_ratio);

    let sub_w = (width + h_ratio as u32 - 1) / h_ratio as u32;
    let sub_h = (height + v_ratio as u32 - 1) / v_ratio as u32;

    (cb_sub, sub_w, sub_h, cr_sub, sub_w, sub_h)
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn no_downsampling_returns_copy()
    {
        let plane = vec![10u8, 20, 30, 40];
        let result = downsample_plane(&plane, 2, 2, 1, 1);
        assert_eq!(result, plane);
    }

    #[test]
    fn horizontal_2x_downsample()
    {
        // 4*1 plane: [10, 20, 30, 40]
        // With h_factor=2: avg(10,20)=15, avg(30,40)=35
        let plane = vec![10u8, 20, 30, 40];
        let result = downsample_plane(&plane, 4, 1, 2, 1);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 15);
        assert_eq!(result[1], 35);
    }

    #[test]
    fn vertical_2x_downsample()
    {
        // 1*4 plane: [10, 20, 30, 40]
        // With v_factor=2: avg(10,20)=15, avg(30,40)=35
        let plane = vec![10u8, 20, 30, 40];
        let result = downsample_plane(&plane, 1, 4, 1, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 15);
        assert_eq!(result[1], 35);
    }

    #[test]
    fn downsample_2x2_box()
    {
        // 4*4 plane, downsample by 2 in both dimensions -> 2*2.
        let plane = vec![
            10, 20, 30, 40,
            50, 60, 70, 80,
            90,100,110,120,
           130,140,150,160,
        ];
        let result = downsample_plane(&plane, 4, 4, 2, 2);
        assert_eq!(result.len(), 4);
        // Top-left 2*2: avg(10,20,50,60) = 140/4 = 35
        assert_eq!(result[0], 35);
        // Top-right 2*2: avg(30,40,70,80) = 220/4 = 55
        assert_eq!(result[1], 55);
    }

    #[test]
    fn downsample_odd_dimension()
    {
        // 3*1 plane with h_factor=2: [10, 20, 30]
        // Cell 0: avg(10,20) = 15; Cell 1: avg(30) = 30 (only 1 sample).
        let plane = vec![10u8, 20, 30];
        let result = downsample_plane(&plane, 3, 1, 2, 1);
        assert_eq!(result.len(), 2); // ⌈3/2⌉ = 2
        assert_eq!(result[0], 15);
        assert_eq!(result[1], 30);
    }

    #[test]
    fn subsample_chroma_444_is_identity()
    {
        let cb = vec![100u8; 16];
        let cr = vec![200u8; 16];
        let (cb_s, cb_w, cb_h, cr_s, cr_w, cr_h) =
            subsample_chroma(&cb, &cr, 4, 4, Subsampling::S444);
        assert_eq!(cb_w, 4);
        assert_eq!(cb_h, 4);
        assert_eq!(cr_w, 4);
        assert_eq!(cr_h, 4);
        assert_eq!(cb_s, cb);
        assert_eq!(cr_s, cr);
    }

    #[test]
    fn subsample_chroma_420_halves_both()
    {
        let cb = vec![128u8; 8 * 8];
        let cr = vec![128u8; 8 * 8];
        let (_, cb_w, cb_h, _, cr_w, cr_h) =
            subsample_chroma(&cb, &cr, 8, 8, Subsampling::S420);
        assert_eq!(cb_w, 4);
        assert_eq!(cb_h, 4);
        assert_eq!(cr_w, 4);
        assert_eq!(cr_h, 4);
    }

    #[test]
    fn subsample_chroma_422_halves_horizontal_only()
    {
        let cb = vec![128u8; 8 * 8];
        let cr = vec![128u8; 8 * 8];
        let (_, cb_w, cb_h, _, cr_w, cr_h) =
            subsample_chroma(&cb, &cr, 8, 8, Subsampling::S422);
        assert_eq!(cb_w, 4);
        assert_eq!(cb_h, 8);
        assert_eq!(cr_w, 4);
        assert_eq!(cr_h, 8);
    }
}
