//! The menu bar symbol and the app icon, drawn in code: a monitor with an arrow pointing out of it.
//! `screenyield --write-icons DIR` writes the app icon as PNG files for the bundle.

use std::path::Path;

/// Monitor and arrow in a unit square (0…1), `true` where the symbol is.
fn symbol(x: f64, y: f64) -> bool {
    // Screen outline
    let (left, right, top, bottom, line) = (0.06, 0.94, 0.12, 0.70, 0.085);
    let in_outer = x >= left && x <= right && y >= top && y <= bottom;
    let in_inner = x >= left + line && x <= right - line && y >= top + line && y <= bottom - line;
    let screen = in_outer && !in_inner;
    // Stand and foot
    let stand = (x - 0.5).abs() <= 0.06 && y > bottom && y <= 0.82;
    let foot = (x - 0.5).abs() <= 0.24 && y > 0.80 && y <= 0.88;
    // Arrow to the right inside the screen: shaft and head
    let mid = (top + bottom) / 2.0;
    let shaft = (0.26..=0.56).contains(&x) && (y - mid).abs() <= 0.055;
    let head = (0.52..=0.76).contains(&x) && (y - mid).abs() <= 0.17 * (0.76 - x) / 0.24;
    screen || stand || foot || shaft || head
}

/// Share of the pixel covered, sampled 4 × 4.
fn coverage(size: u32, px: u32, py: u32, shape: impl Fn(f64, f64) -> bool) -> f64 {
    let mut hits = 0;
    for sy in 0..4 {
        for sx in 0..4 {
            let x = (f64::from(px) + (f64::from(sx) + 0.5) / 4.0) / f64::from(size);
            let y = (f64::from(py) + (f64::from(sy) + 0.5) / 4.0) / f64::from(size);
            if shape(x, y) {
                hits += 1;
            }
        }
    }
    f64::from(hits) / 16.0
}

/// Menu bar symbol: black with alpha, macOS colors it as a template image.
pub fn menu_bar(size: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let a = coverage(size, x, y, symbol);
            rgba.extend_from_slice(&[0, 0, 0, (a * 255.0).round() as u8]);
        }
    }
    rgba
}

/// App icon: the symbol in white on a blue rounded square (macOS icon grid: 824 of 1024).
pub fn app(size: u32) -> Vec<u8> {
    let s = f64::from(size);
    let margin = 100.0 / 1024.0;
    let radius = 185.0 / 1024.0;
    let tile = |x: f64, y: f64| {
        let (lo, hi) = (margin, 1.0 - margin);
        let cx = x.clamp(lo + radius, hi - radius);
        let cy = y.clamp(lo + radius, hi - radius);
        x >= lo && x <= hi && y >= lo && y <= hi && (x - cx).hypot(y - cy) <= radius
    };
    // The symbol takes the middle of the tile
    let inset = |x: f64, y: f64| symbol((x - 0.22) / 0.56, (y - 0.24) / 0.56);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let t = coverage(size, x, y, tile);
            let m = coverage(size, x, y, inset);
            // Blue from top to bottom
            let g = f64::from(y) / s;
            let blue = [30.0 + 20.0 * g, 120.0 - 40.0 * g, 230.0 - 50.0 * g];
            let px = |c: f64| (c * (1.0 - m) + 255.0 * m).round() as u8;
            rgba.extend_from_slice(&[px(blue[0]), px(blue[1]), px(blue[2]), (t * 255.0).round() as u8]);
        }
    }
    rgba
}

/// Writes the PNG sizes `iconutil` needs for an `.icns` into `dir/ScreenYield.iconset`.
pub fn write_iconset(dir: &Path) -> std::io::Result<()> {
    let set = dir.join("ScreenYield.iconset");
    std::fs::create_dir_all(&set)?;
    for base in [16u32, 32, 128, 256, 512] {
        std::fs::write(set.join(format!("icon_{base}x{base}.png")), png(base, &app(base)))?;
        let double = base * 2;
        std::fs::write(
            set.join(format!("icon_{base}x{base}@2x.png")),
            png(double, &app(double)),
        )?;
    }
    Ok(())
}

/// A minimal PNG (RGBA, uncompressed deflate blocks).
pub fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    let mut raw = Vec::with_capacity(rgba.len() + size as usize);
    for row in rgba.chunks((size * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(u8::from(blocks.peek().is_none()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn symbols_have_the_right_size_and_some_ink() {
        let bar = menu_bar(36);
        assert_eq!(bar.len(), 36 * 36 * 4);
        assert!(bar.chunks(4).any(|p| p[3] == 255));
        assert!(bar.chunks(4).any(|p| p[3] == 0));
        let icon = app(64);
        assert_eq!(png(64, &icon)[..8], *b"\x89PNG\r\n\x1a\n");
    }
}
