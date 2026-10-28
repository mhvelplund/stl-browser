//! Test fixtures shared by the modules' tests.
use std::io::Cursor;

use ::image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::mesh::Tri;

/// An axis-aligned cube from the origin, 12 triangles.
pub fn cube(s: f32) -> Vec<Tri> {
    let p = |x: f32, y: f32, z: f32| [x * s, y * s, z * s];
    let quads = [
        [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.)],
        [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
        [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
        [p(0., 1., 0.), p(1., 1., 0.), p(1., 1., 1.), p(0., 1., 1.)],
        [p(0., 0., 0.), p(0., 1., 0.), p(0., 1., 1.), p(0., 0., 1.)],
        [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
    ];
    quads.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect()
}

/// Binary STL bytes for `tris`.
pub fn stl_bytes(tris: &[Tri]) -> Vec<u8> {
    let tris: Vec<stl_io::Triangle> = tris
        .iter()
        .map(|t| stl_io::Triangle {
            normal: stl_io::Vector::new([0.0, 0.0, 0.0]),
            vertices: [stl_io::Vector::new(t[0]), stl_io::Vector::new(t[1]), stl_io::Vector::new(t[2])],
        })
        .collect();
    let mut out = Vec::new();
    stl_io::write_stl(&mut out, tris.iter()).unwrap();
    out
}

/// A small solid-colour PNG.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([200, 40, 40, 255])))
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}
