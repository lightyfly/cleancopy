//! Development-only X11 screenshot helper; not part of the desktop backend.
//! Usage: cargo run -p cleancopy-desktop --example screenshot -- <output.png> [window-id]

use std::{error::Error, fs::File, io, io::BufWriter};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder, VisualClass},
};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .ok_or_else(|| invalid("usage: screenshot <output.png> [window-id]"))?;
    let window = args
        .next()
        .map(
            |id| match id.strip_prefix("0x").or_else(|| id.strip_prefix("0X")) {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => id.parse::<u32>(),
            },
        )
        .transpose()?;
    if args.next().is_some() {
        return Err(invalid("usage: screenshot <output.png> [window-id]").into());
    }

    let (conn, screen_index) = x11rb::connect(None)?;
    let setup = conn.setup();
    let window = window.unwrap_or(setup.roots[screen_index].root);
    let geometry = conn.get_geometry(window)?.reply()?;
    let attributes = conn.get_window_attributes(window)?.reply()?;
    let visual = setup
        .roots
        .iter()
        .flat_map(|screen| &screen.allowed_depths)
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.visual_id == attributes.visual)
        .ok_or_else(|| invalid("window visual not found in X11 setup"))?;
    if visual.class != VisualClass::TRUE_COLOR {
        return Err(invalid("only TrueColor windows are supported").into());
    }
    let image = conn
        .get_image(
            ImageFormat::Z_PIXMAP,
            window,
            0,
            0,
            geometry.width,
            geometry.height,
            u32::MAX,
        )?
        .reply()?;
    let format = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == image.depth)
        .ok_or_else(|| invalid("image pixmap format not found"))?;
    let bpp = usize::from(format.bits_per_pixel);
    let pad = usize::from(format.scanline_pad);
    if !matches!(bpp, 8 | 16 | 24 | 32) || pad == 0 {
        return Err(invalid("unsupported pixel format").into());
    }
    let width = usize::from(geometry.width);
    let height = usize::from(geometry.height);
    let stride = (width * bpp).div_ceil(pad) * pad / 8;
    if image.data.len() < stride * height {
        return Err(invalid("truncated X11 image").into());
    }
    let channel = |pixel: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let max = u64::from(mask >> shift);
        let value = u64::from((pixel & mask) >> shift);
        ((value * 255 + max / 2) / max) as u8
    };
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in image.data.chunks_exact(stride).take(height) {
        for bytes in row[..width * (bpp / 8)].chunks_exact(bpp / 8) {
            let pixel = if setup.image_byte_order == ImageOrder::LSB_FIRST {
                bytes
                    .iter()
                    .rev()
                    .fold(0u32, |p, &b| (p << 8) | u32::from(b))
            } else {
                bytes.iter().fold(0u32, |p, &b| (p << 8) | u32::from(b))
            };
            rgb.extend([
                channel(pixel, visual.red_mask),
                channel(pixel, visual.green_mask),
                channel(pixel, visual.blue_mask),
            ]);
        }
    }
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(&output)?),
        u32::from(geometry.width),
        u32::from(geometry.height),
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgb)?;
    println!("Saved {}x{} to {output}", geometry.width, geometry.height);
    Ok(())
}
