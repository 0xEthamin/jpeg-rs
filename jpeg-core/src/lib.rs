pub mod bitstream;
pub mod block;
pub mod color;
pub mod dct;
pub mod encoder;
pub mod entropy;
pub mod error;
pub mod marker;
pub mod quantize;
pub mod sampling;
pub mod types;

pub use encoder::encode;
pub use error::Error;
pub use types::
{
    ColorSpace,
    EncoderConfig,
    RawImage,
    Subsampling,
};
