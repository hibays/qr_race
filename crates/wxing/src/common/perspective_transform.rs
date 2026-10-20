//! Port of `zxing/common/perspective_transform.{hpp,cpp}`.

#[derive(Clone, Copy, Debug)]
pub struct PerspectiveTransform {
    a11: f32,
    a12: f32,
    a13: f32,
    a21: f32,
    a22: f32,
    a23: f32,
    a31: f32,
    a32: f32,
    a33: f32,
}

impl PerspectiveTransform {
    /// Argument order matches the C++ constructor, column major:
    /// a11, a21, a31, a12, a22, a32, a13, a23, a33.
    #[allow(clippy::too_many_arguments)]
    fn new(
        a11: f32,
        a21: f32,
        a31: f32,
        a12: f32,
        a22: f32,
        a32: f32,
        a13: f32,
        a23: f32,
        a33: f32,
    ) -> Self {
        Self {
            a11,
            a12,
            a13,
            a21,
            a22,
            a23,
            a31,
            a32,
            a33,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn quadrilateral_to_quadrilateral(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        x0p: f32,
        y0p: f32,
        x1p: f32,
        y1p: f32,
        x2p: f32,
        y2p: f32,
        x3p: f32,
        y3p: f32,
    ) -> Self {
        let q_to_s = Self::quadrilateral_to_square(x0, y0, x1, y1, x2, y2, x3, y3);
        let s_to_q = Self::square_to_quadrilateral(x0p, y0p, x1p, y1p, x2p, y2p, x3p, y3p);
        s_to_q.times(&q_to_s)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn square_to_quadrilateral(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Self {
        let dx3 = x0 - x1 + x2 - x3;
        let dy3 = y0 - y1 + y2 - y3;
        if dx3.abs() <= 1e-6 && dy3.abs() <= 1e-6 {
            // Affine case
            Self::new(x1 - x0, x2 - x1, x0, y1 - y0, y2 - y1, y0, 0.0, 0.0, 1.0)
        } else {
            let dx1 = x1 - x2;
            let dx2 = x3 - x2;
            let dy1 = y1 - y2;
            let dy2 = y3 - y2;
            let denominator = dx1 * dy2 - dx2 * dy1;
            let a13 = (dx3 * dy2 - dx2 * dy3) / denominator;
            let a23 = (dx1 * dy3 - dx3 * dy1) / denominator;
            Self::new(
                x1 - x0 + a13 * x1,
                x3 - x0 + a23 * x3,
                x0,
                y1 - y0 + a13 * y1,
                y3 - y0 + a23 * y3,
                y0,
                a13,
                a23,
                1.0,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn quadrilateral_to_square(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Self {
        // The adjugate is the inverse up to a constant factor, which has no
        // effect in homogeneous coordinates
        Self::square_to_quadrilateral(x0, y0, x1, y1, x2, y2, x3, y3).build_adjoint()
    }

    pub fn build_adjoint(&self) -> Self {
        let s = self;
        Self::new(
            s.a22 * s.a33 - s.a23 * s.a32,
            s.a23 * s.a31 - s.a21 * s.a33,
            s.a21 * s.a32 - s.a22 * s.a31,
            s.a13 * s.a32 - s.a12 * s.a33,
            s.a11 * s.a33 - s.a13 * s.a31,
            s.a12 * s.a31 - s.a11 * s.a32,
            s.a12 * s.a23 - s.a13 * s.a22,
            s.a13 * s.a21 - s.a11 * s.a23,
            s.a11 * s.a22 - s.a12 * s.a21,
        )
    }

    pub fn times(&self, o: &PerspectiveTransform) -> Self {
        let s = self;
        Self::new(
            s.a11 * o.a11 + s.a21 * o.a12 + s.a31 * o.a13,
            s.a11 * o.a21 + s.a21 * o.a22 + s.a31 * o.a23,
            s.a11 * o.a31 + s.a21 * o.a32 + s.a31 * o.a33,
            s.a12 * o.a11 + s.a22 * o.a12 + s.a32 * o.a13,
            s.a12 * o.a21 + s.a22 * o.a22 + s.a32 * o.a23,
            s.a12 * o.a31 + s.a22 * o.a32 + s.a32 * o.a33,
            s.a13 * o.a11 + s.a23 * o.a12 + s.a33 * o.a13,
            s.a13 * o.a21 + s.a23 * o.a22 + s.a33 * o.a23,
            s.a13 * o.a31 + s.a23 * o.a32 + s.a33 * o.a33,
        )
    }

    /// Transform a point list of the form [x0,y0,x1,y1,...] in place.
    pub fn transform_points(&self, points: &mut [f32]) {
        let mut i = 0;
        while i + 1 < points.len() {
            let x = points[i];
            let y = points[i + 1];
            let denominator = self.a13 * x + self.a23 * y + self.a33;
            let w = 1.0 / denominator;
            points[i] = (self.a11 * x + self.a21 * y + self.a31) * w;
            points[i + 1] = (self.a12 * x + self.a22 * y + self.a32) * w;
            i += 2;
        }
    }
}
