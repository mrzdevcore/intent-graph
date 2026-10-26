//! Triangles for the GPU (lyon): fills of closed outlines (pen strokes, paper) and strokes of
//! sketch polylines.

use crate::sketch::Polyline;
use lyon_tessellation::geometry_builder::simple_builder;
use lyon_tessellation::math::point;
use lyon_tessellation::path::Polygon;
use lyon_tessellation::{
    FillOptions, FillTessellator, LineCap, LineJoin, StrokeOptions, StrokeTessellator,
    VertexBuffers,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f32; 2]>,
    /// Triangles: three indices each.
    pub indices: Vec<u32>,
}

impl Mesh {
    fn from(b: VertexBuffers<lyon_tessellation::math::Point, u16>) -> Mesh {
        Mesh {
            vertices: b.vertices.iter().map(|p| [p.x, p.y]).collect(),
            indices: b.indices.iter().map(|&i| i as u32).collect(),
        }
    }

    /// The triangles' corners in order (three per triangle).
    pub fn corners(&self) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.indices.iter().map(|&i| self.vertices[i as usize])
    }
}

fn pts(poly: &[[f32; 2]]) -> Vec<lyon_tessellation::math::Point> {
    poly.iter().map(|p| point(p[0], p[1])).collect()
}

/// The inside of a closed outline (non-zero, so a self-crossing pen outline stays filled).
pub fn fill(poly: &[[f32; 2]]) -> Mesh {
    if poly.len() < 3 {
        return Mesh::default();
    }
    let p = pts(poly);
    let mut b = VertexBuffers::new();
    let opts = FillOptions::non_zero().with_tolerance(0.1);
    let polygon = Polygon {
        points: &p,
        closed: true,
    };
    // a failure (a degenerate outline) leaves what was built: never a panic
    let _ = FillTessellator::new().tessellate_polygon(polygon, &opts, &mut simple_builder(&mut b));
    Mesh::from(b)
}

/// An open line `width` world px wide, with round caps and joins.
pub fn stroke(line: &Polyline, width: f32) -> Mesh {
    if line.len() < 2 {
        return Mesh::default();
    }
    let p = pts(line);
    let mut b = VertexBuffers::new();
    let opts = StrokeOptions::default()
        .with_line_width(width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
        .with_tolerance(0.1);
    let polygon = Polygon {
        points: &p,
        closed: false,
    };
    let _ =
        StrokeTessellator::new().tessellate_polygon(polygon, &opts, &mut simple_builder(&mut b));
    Mesh::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_and_stroke_produce_triangles() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let f = fill(&square);
        assert_eq!(f.indices.len() % 3, 0);
        assert!(f.indices.len() >= 6, "{f:?}");
        let s = stroke(&vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], 1.5);
        assert_eq!(s.indices.len() % 3, 0);
        assert!(s.indices.len() >= 12, "{s:?}");
        assert_eq!(s.corners().count(), s.indices.len());
        assert!(fill(&square[..2]).indices.is_empty());
        assert!(stroke(&vec![[0.0, 0.0]], 1.0).indices.is_empty());
    }
}
