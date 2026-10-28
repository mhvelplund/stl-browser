//! A small CPU rasterizer: orthographic 3/4 isometric view fitted to the bounding box, z-buffer,
//! flat two-sided grey shading, rendered at 2× and box-downsampled for anti-aliasing.
use ::image::{Rgba, RgbaImage};

use crate::THUMB_SIZE;
use crate::mesh::Tri;

type V3 = [f32; 3];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { a }
}
fn mix(a: V3, wa: f32, b: V3, wb: f32) -> V3 {
    [a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb, a[2] * wa + b[2] * wb]
}

/// Render `tris` (Z up) to a `THUMB_SIZE` square with a transparent background.
pub fn render(tris: &[Tri]) -> RgbaImage {
    const SS: usize = 2;
    let n = THUMB_SIZE as usize * SS;
    // isometric camera: looking from (1, -1, 1), i.e. 45° around Z and 35.264° down
    let to_cam = norm([1.0, -1.0, 1.0]);
    let right = norm(cross([-to_cam[0], -to_cam[1], -to_cam[2]], [0.0, 0.0, 1.0]));
    let up = cross(right, [-to_cam[0], -to_cam[1], -to_cam[2]]);
    let (s25, c25) = 25f32.to_radians().sin_cos();
    let light = norm(mix(to_cam, c25, norm(sub(up, right)), s25));

    // project, and fit the projected bounds into the square with a 6 % margin each side
    let proj = |v: V3| [dot(v, right), -dot(v, up), dot(v, to_cam)];
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for t in tris {
        for &v in t {
            let p = proj(v);
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let scale = if extent > f32::EPSILON { n as f32 * 0.88 / extent } else { 1.0 };
    let off = [n as f32 / 2.0 - (lo[0] + hi[0]) / 2.0 * scale, n as f32 / 2.0 - (lo[1] + hi[1]) / 2.0 * scale];

    let mut depth = vec![f32::MIN; n * n];
    let mut grey = vec![0u8; n * n];
    for t in tris {
        let normal = norm(cross(sub(t[1], t[0]), sub(t[2], t[0])));
        let g = (60.0 + 180.0 * dot(normal, light).abs()).clamp(0.0, 255.0) as u8;
        let p = t.map(|v| {
            let q = proj(v);
            [q[0] * scale + off[0], q[1] * scale + off[1], q[2]]
        });
        let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if area.abs() < 1e-9 || !area.is_finite() {
            continue;
        }
        let x0 = p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let x1 = (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil() as usize).min(n);
        let y0 = p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let y1 = (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil() as usize).min(n);
        let edge = |a: [f32; 3], b: [f32; 3], x: f32, y: f32| ((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])) / area;
        for y in y0..y1 {
            for x in x0..x1 {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let (w0, w1, w2) = (edge(p[1], p[2], fx, fy), edge(p[2], p[0], fx, fy), edge(p[0], p[1], fx, fy));
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2];
                let i = y * n + x;
                if z > depth[i] {
                    depth[i] = z;
                    grey[i] = g;
                }
            }
        }
    }

    let mut out = RgbaImage::new(THUMB_SIZE, THUMB_SIZE);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let (mut covered, mut sum) = (0u32, 0u32);
        for dy in 0..SS {
            for dx in 0..SS {
                let i = (y as usize * SS + dy) * n + x as usize * SS + dx;
                if depth[i] > f32::MIN {
                    covered += 1;
                    sum += grey[i] as u32;
                }
            }
        }
        if covered > 0 {
            let g = (sum / covered) as u8;
            *px = Rgba([g, g, g, (covered * 255 / (SS * SS) as u32) as u8]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::cube;

    #[test]
    fn cube_renders_centred_and_non_empty() {
        let img = render(&cube(10.0));
        assert_eq!(img.dimensions(), (256, 256));
        let (mut n, mut sx, mut sy) = (0u32, 0u64, 0u64);
        for (x, y, p) in img.enumerate_pixels() {
            if p[3] > 128 {
                n += 1;
                sx += x as u64;
                sy += y as u64;
            }
        }
        let cov = n as f64 / (256.0 * 256.0);
        assert!((0.2..0.9).contains(&cov), "coverage {cov}");
        let (cx, cy) = (sx as f64 / n as f64, sy as f64 / n as f64);
        assert!((cx - 128.0).abs() < 16.0 && (cy - 128.0).abs() < 16.0, "centroid {cx},{cy}");
        // three visible faces get three different grey levels
        let mut greys: Vec<u8> = img.pixels().filter(|p| p[3] == 255).map(|p| p[0]).collect();
        greys.sort();
        greys.dedup();
        assert!(greys.len() >= 3, "{greys:?}");
    }

    #[test]
    fn far_from_origin_still_fits() {
        let tris: Vec<_> = cube(1.0).iter().map(|t| t.map(|v| [v[0] + 1e4, v[1] - 3e3, v[2] + 50.0])).collect();
        let img = render(&tris);
        assert!(img.pixels().filter(|p| p[3] > 128).count() > 256 * 256 / 5);
    }

    #[test]
    fn degenerate_mesh_renders() {
        let flat = vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
        let _ = render(&flat);
        let point = vec![[[1.0, 1.0, 1.0]; 3]];
        let _ = render(&point);
        let _ = render(&[]);
    }
}
