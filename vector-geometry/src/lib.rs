//! Shared polygon tessellation and drawable-aware boundary coverage.
pub use lyon_tessellation;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
    math::{Point, point},
    path::{Event, Path, iterator::PathIterator},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: Point,
    pub offset: [f32; 2],
    pub coverage: f32,
}

impl Vertex {
    pub fn midpoint(self, other: Self) -> Self {
        Self {
            position: (self.position + other.position.to_vector()) * 0.5,
            offset: [
                (self.offset[0] + other.offset[0]) * 0.5,
                (self.offset[1] + other.offset[1]) * 0.5,
            ],
            coverage: (self.coverage + other.coverage) * 0.5,
        }
    }
}

/// Fill a path and add a one-physical-pixel boundary fringe. The caller scales
/// positions in its shader; offsets remain in physical pixels at every density.
pub fn tessellate(path: &Path, tolerance: f32, rule: FillRule) -> Result<Vec<Vertex>, String> {
    let outline: Vec<_> = path
        .iter()
        .flattened(tolerance)
        .filter_map(|event| match event {
            Event::Line { from, to } => Some((from, to)),
            Event::End {
                last,
                first,
                close: true,
            } => Some((last, first)),
            _ => None,
        })
        .collect();
    let contains = |p: Point| {
        let mut winding = 0i32;
        for &(a, b) in &outline {
            if (a.y <= p.y && b.y > p.y) || (b.y <= p.y && a.y > p.y) {
                let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x > p.x {
                    winding += if b.y > a.y { 1 } else { -1 };
                }
            }
        }
        match rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    };
    let mut geometry: VertexBuffers<Point, u32> = VertexBuffers::new();
    FillTessellator::new()
        .tessellate_path(
            path,
            &FillOptions::default()
                .with_tolerance(tolerance)
                .with_fill_rule(rule),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| vertex.position()),
        )
        .map_err(|e| format!("Path tessellation failed: {e:?}"))?;
    let mut unique = BTreeMap::new();
    let mut points = Vec::new();
    let mapping: Vec<usize> = geometry
        .vertices
        .iter()
        .map(|p| {
            let key = (
                (if p.x == 0. { 0. } else { p.x }).to_bits(),
                (if p.y == 0. { 0. } else { p.y }).to_bits(),
            );
            *unique.entry(key).or_insert_with(|| {
                points.push(*p);
                points.len() - 1
            })
        })
        .collect();
    let mut triangles = Vec::new();
    let mut edges: BTreeMap<(usize, usize), (usize, usize, usize)> = BTreeMap::new();
    for triangle in geometry.indices.chunks_exact(3) {
        let [a, mut b, mut c] = [
            mapping[triangle[0] as usize],
            mapping[triangle[1] as usize],
            mapping[triangle[2] as usize],
        ];
        if triangles.len() > 300_000 {
            return Err("Path exceeds its geometry budget".into());
        }
        let area = (points[b] - points[a]).cross(points[c] - points[a]);
        // Positions may be in font em units as well as SVG canvas units.
        if area.abs() < 1e-12 {
            continue;
        }
        if area < 0. {
            std::mem::swap(&mut b, &mut c);
        }
        triangles.push([a, b, c]);
        for (a, b) in [(a, b), (b, c), (c, a)] {
            edges
                .entry((a.min(b), a.max(b)))
                .and_modify(|edge| edge.2 += 1)
                .or_insert((a, b, 1));
        }
    }
    let boundary: Vec<_> = edges
        .values()
        .filter(|edge| edge.2 == 1)
        .filter(|&&(a, b, _)| {
            // Do not add fringes to unpaired interior T-junctions.
            let d = points[b] - points[a];
            let length = d.length();
            if length == 0. {
                return false;
            }
            let midpoint = points[a] + d * 0.5;
            let probe =
                point(d.y / length, -d.x / length).to_vector() * (tolerance * 0.25).max(0.0000001);
            contains(midpoint + probe) != contains(midpoint - probe)
        })
        .map(|&(a, b, _)| (a, b))
        .collect();
    let mut normals = vec![[0., 0.]; points.len()];
    let mut first_normal = normals.clone();
    for &(a, b) in &boundary {
        let d = points[b] - points[a];
        let length = d.length();
        if length == 0. {
            continue;
        }
        let n = [d.y / length, -d.x / length];
        for index in [a, b] {
            normals[index][0] += n[0];
            normals[index][1] += n[1];
            first_normal[index] = n;
        }
    }
    for (index, normal) in normals.iter_mut().enumerate() {
        let length = normal[0].hypot(normal[1]);
        if length > 1e-6 {
            let unit = [normal[0] / length, normal[1] / length];
            let miter = 1.
                / (unit[0] * first_normal[index][0] + unit[1] * first_normal[index][1]).max(0.25);
            *normal = [unit[0] * miter, unit[1] * miter];
        }
    }
    let vertex = |index: usize, outside: bool| Vertex {
        position: points[index],
        offset: [
            normals[index][0] * if outside { 0.5 } else { -0.5 },
            normals[index][1] * if outside { 0.5 } else { -0.5 },
        ],
        coverage: if outside { 0. } else { 1. },
    };
    let mut vertices = Vec::with_capacity(triangles.len() * 3 + boundary.len() * 6);
    for [a, b, c] in triangles {
        vertices.extend([vertex(a, false), vertex(b, false), vertex(c, false)]);
    }
    for (a, b) in boundary {
        vertices.extend([
            vertex(a, false),
            vertex(a, true),
            vertex(b, false),
            vertex(b, false),
            vertex(a, true),
            vertex(b, true),
        ]);
    }
    if vertices.len() > 1_000_000 {
        return Err("Path exceeds its geometry budget".into());
    }
    Ok(vertices)
}
