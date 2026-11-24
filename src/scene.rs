//! An original orthographic canal district. CPU rasterization emits all visible-facing
//! surface candidates, including occluded ones. The compute passes select visibility.
use crate::Fragment;
use std::{fs::File, io::BufWriter, path::Path};

pub const WIDTH: u32 = 384;
pub const HEIGHT: u32 = 240;
type Point = [f32; 3];

fn project([x, y, z]: Point) -> Point {
    [
        192.0 + 10.5 * (x - y),
        129.0 + 5.25 * (x + y) - 10.5 * z,
        64.0 + x + y + z,
    ]
}

fn edge(a: Point, b: Point, x: f32, y: f32) -> f32 {
    (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
}

fn triangle(out: &mut Vec<Fragment>, p: [Point; 3], color: u32) {
    let [a, b, c] = p.map(project);
    let area = edge(a, b, c[0], c[1]);
    if area.abs() < 0.0001 {
        return;
    }
    let bounds = |axis: usize, limit: u32| {
        let lo = a[axis]
            .min(b[axis])
            .min(c[axis])
            .floor()
            .clamp(0.0, limit as f32) as u32;
        let hi = a[axis]
            .max(b[axis])
            .max(c[axis])
            .ceil()
            .clamp(0.0, limit as f32) as u32;
        lo..hi
    };
    for y in bounds(1, HEIGHT) {
        for x in bounds(0, WIDTH) {
            let (sx, sy) = (x as f32 + 0.5, y as f32 + 0.5);
            let u = edge(b, c, sx, sy) / area;
            let v = edge(c, a, sx, sy) / area;
            let w = edge(a, b, sx, sy) / area;
            if u.min(v).min(w) < -0.00001 {
                continue;
            }
            let depth = ((u * a[2] + v * b[2] + w * c[2]) * 4096.0).round() as u64;
            out.push(Fragment::new(y * WIDTH + x, depth << 32 | u64::from(color)));
        }
    }
}

fn shade(rgb: u32, percent: u32) -> u32 {
    let channel = |shift: u32| ((rgb >> shift) & 255) * percent / 100;
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

fn block(out: &mut Vec<Fragment>, lo: Point, hi: Point, rgb: u32) {
    let [x, y, z] = lo;
    let [r, s, t] = hi;
    let faces = [
        ([[x, y, t], [r, y, t], [r, s, t], [x, s, t]], rgb),
        ([[r, y, z], [r, s, z], [r, s, t], [r, y, t]], shade(rgb, 72)),
        ([[x, s, z], [r, s, z], [r, s, t], [x, s, t]], shade(rgb, 52)),
    ];
    for ([a, b, c, d], color) in faces {
        triangle(out, [a, b, c], color);
        triangle(out, [a, c, d], color);
    }
}

pub fn canal() -> Vec<Fragment> {
    let mut out = Vec::new();
    block(&mut out, [-8.0, -8.0, -0.8], [8.0, 8.0, 0.0], 0x35859a);
    for (lo, hi) in [(-8.0, -2.0), (2.0, 8.0)] {
        block(&mut out, [lo, -8.0, 0.0], [hi, 8.0, 0.42], 0xd7bbaa);
    }
    let colors = [0xe99871, 0x82aeab, 0xe6bf88, 0xb794b5, 0x85a6c2];
    for (row, y) in [-7.4, -4.1, 0.8, 4.1].into_iter().enumerate() {
        for (col, x) in [-7.4, -4.8, 2.6, 5.4].into_iter().enumerate() {
            let top = 2.6 + ((row * 3 + col * 5) % 7) as f32 * 0.7;
            let paint = colors[(row * 2 + col) % colors.len()];
            block(&mut out, [x, y, 0.42], [x + 1.95, y + 2.2, top], paint);
            block(
                &mut out,
                [x - 0.07, y - 0.07, top],
                [x + 2.02, y + 2.27, top + 0.17],
                0xf6e4c3,
            );
            block(
                &mut out,
                [x + 0.4, y + 0.4, top + 0.17],
                [x + 1.2, y + 1.4, top + 0.47],
                paint,
            );
            let mut z = 1.15;
            while z + 0.38 < top {
                for i in 0..3 {
                    let d = 0.22 + i as f32 * 0.58;
                    block(
                        &mut out,
                        [x + d, y + 2.2, z],
                        [x + d + 0.32, y + 2.23, z + 0.38],
                        0x456278,
                    );
                    block(
                        &mut out,
                        [x + 1.95, y + d, z],
                        [x + 1.98, y + d + 0.32, z + 0.38],
                        0xf8dca5,
                    );
                }
                z += 0.87;
            }
        }
    }
    for y in [-1.6, 7.1] {
        for x in [-1.5, 1.3] {
            block(&mut out, [x, y, 0.0], [x + 0.3, y + 0.8, 0.9], 0x9687a2);
        }
        block(
            &mut out,
            [-2.4, y - 0.1, 0.83],
            [2.4, y + 1.0, 1.04],
            0xb58c73,
        );
        for rail in [y - 0.1, y + 0.93] {
            block(
                &mut out,
                [-2.4, rail, 1.04],
                [2.4, rail + 0.07, 1.41],
                0xe5b687,
            );
        }
    }
    out
}

pub fn write_image(path: &Path, keys: &[u64]) -> Result<(), Box<dyn std::error::Error>> {
    if keys.len() != (WIDTH * HEIGHT) as usize {
        return Err("wrong image dimensions".into());
    }
    let mut rgb = Vec::with_capacity(keys.len() * 3);
    for (i, &key) in keys.iter().enumerate() {
        let t = (i as u32 / WIDTH) * 20 / HEIGHT;
        let color = if key == 0 {
            (17 + t) << 16 | (27 + t) << 8 | (45 + t)
        } else {
            key as u32
        };
        rgb.extend_from_slice(&[(color >> 16) as u8, (color >> 8) as u8, color as u8]);
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
