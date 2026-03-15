const FP_SHIFT: i32 = 16;
const FP_HALF: i32 = 1 << (FP_SHIFT - 1); // rounding bias

const YR: i32 = 19595;  // 0.299    * 65536
const YG: i32 = 38470;  // 0.587    * 65536
const YB: i32 = 7471;   // 0.114    * 65536
const CBR: i32 = -11059; // -0.1687 * 65536
const CBG: i32 = -21709; // -0.3313 * 65536
const CBB: i32 = 32768;  // 0.5     * 65536
const CRR: i32 = 32768;  // 0.5     * 65536
const CRG: i32 = -27439; // -0.4187 * 65536
const CRB: i32 = -5329;  // -0.0813 * 65536

#[inline(always)]
fn clamp_u8(v: i32) -> u8
{
    v.clamp(0, 255) as u8
}

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

pub fn grayscale_to_y_plane(gray: &[u8]) -> Vec<u8>
{
    gray.to_vec()
}
