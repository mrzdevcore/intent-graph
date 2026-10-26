//! Screen ↔ world: `screen = (world - (x, y)) * zoom`.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (sx / self.zoom + self.x, sy / self.zoom + self.y)
    }

    pub fn to_screen(&self, r: [f32; 4]) -> [f32; 4] {
        [
            (r[0] - self.x) * self.zoom,
            (r[1] - self.y) * self.zoom,
            r[2] * self.zoom,
            r[3] * self.zoom,
        ]
    }

    /// Zooms by `f` keeping the world point under `(sx, sy)` fixed.
    pub fn zoom_at(&mut self, sx: f32, sy: f32, f: f32) {
        let (wx, wy) = self.to_world(sx, sy);
        self.zoom = (self.zoom * f).clamp(0.05, 8.0);
        self.x = wx - sx / self.zoom;
        self.y = wy - sy / self.zoom;
    }

    /// Fits world bounds `[x, y, w, h]` into a `w × h` screen with a margin, never above 100 %.
    pub fn fit(&mut self, b: [f32; 4], w: f32, h: f32) {
        let m = 40.0;
        self.zoom = ((w - 2.0 * m) / b[2].max(1.0))
            .min((h - 2.0 * m) / b[3].max(1.0))
            .clamp(0.05, 1.0);
        self.x = b[0] + b[2] / 2.0 - w / 2.0 / self.zoom;
        self.y = b[1] + b[3] / 2.0 - h / 2.0 / self.zoom;
    }

    /// The world rect a `w × h` screen shows.
    pub fn visible(&self, w: f32, h: f32) -> [f32; 4] {
        [self.x, self.y, w / self.zoom, h / self.zoom]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let mut c = Camera::default();
        let before = c.to_world(100.0, 50.0);
        c.zoom_at(100.0, 50.0, 2.0);
        assert_eq!(c.to_world(100.0, 50.0), before);
        c.fit([0.0, 0.0, 400.0, 200.0], 480.0, 280.0);
        assert_eq!((c.zoom, c.x, c.y), (1.0, -40.0, -40.0));
    }
}
