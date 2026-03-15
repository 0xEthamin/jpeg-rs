use crate::types::Subsampling;

pub fn downsample_plane(
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

    let hf = h_factor as u32;
    let vf = v_factor as u32;
    let out_w = (width + hf - 1) / hf;
    let out_h = (height + vf - 1) / vf;

    let mut out = Vec::with_capacity((out_w * out_h) as usize);

    for oy in 0..out_h
    {
        for ox in 0..out_w
        {
            let mut sum: u32 = 0;
            let mut count: u32 = 0;

            for dy in 0..vf
            {
                let sy = oy * vf + dy;
                if sy >= height
                {
                    continue;
                }

                for dx in 0..hf
                {
                    let sx = ox * hf + dx;
                    if sx >= width
                    {
                        continue;
                    }

                    sum += plane[(sy * width + sx) as usize] as u32;
                    count += 1;
                }
            }

            let avg = ((sum + count / 2) / count) as u8;
            out.push(avg);
        }
    }

    out
}

pub fn subsample_chroma(
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
