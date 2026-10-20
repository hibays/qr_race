//! Port of `finder_pattern.{hpp,cpp}` / `finder_pattern_info.{hpp,cpp}` /
//! `alignment_pattern.{hpp,cpp}` / `pattern_result.{hpp,cpp}`.

use crate::result::ResultPoint;

/// Horizontal and vertical check state of a finder pattern.
///
/// The fork allows a finder pattern to overflow the image boundary; these two
/// states record the overflow direction, which is used later when correcting
/// the position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalState {
    Normal = 0,
    LeftSpill = 1,
    RightSpill = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalState {
    Normal = 3,
    UpSpill = 4,
    DownSpill = 5,
}

#[derive(Clone, Copy, Debug)]
pub struct FinderPattern {
    pub point: ResultPoint,
    pub estimated_module_size: f32,
    pub count: i32,
    pub horizontal_state: HorizontalState,
    pub vertical_state: VerticalState,
}

impl FinderPattern {
    pub fn new(pos_x: f32, pos_y: f32, estimated_module_size: f32) -> Self {
        Self::with_count(pos_x, pos_y, estimated_module_size, 1)
    }

    pub fn with_count(pos_x: f32, pos_y: f32, estimated_module_size: f32, count: i32) -> Self {
        Self {
            point: ResultPoint::new(pos_x, pos_y),
            estimated_module_size,
            count,
            horizontal_state: HorizontalState::Normal,
            vertical_state: VerticalState::Normal,
        }
    }

    #[inline]
    pub fn x(&self) -> f32 {
        self.point.x
    }

    #[inline]
    pub fn y(&self) -> f32 {
        self.point.y
    }

    /// Note that the arguments are (i, j) = (row, column), as in the original.
    pub fn about_equals(&self, module_size: f32, i: f32, j: f32) -> bool {
        if (i - self.y()).abs() <= module_size && (j - self.x()).abs() <= module_size {
            let module_size_diff = (module_size - self.estimated_module_size).abs();
            return module_size_diff <= 1.0 || module_size_diff <= self.estimated_module_size;
        }
        false
    }

    /// Averages in a new observation. The position is only updated while the
    /// accumulated count is 3 or less, so that later noise cannot pull it off.
    pub fn combine_estimate(&self, i: f32, j: f32, new_module_size: f32) -> FinderPattern {
        let combined_count = self.count + 1;
        let mut combined_x = self.x();
        let mut combined_y = self.y();
        let mut combined_module_size = self.estimated_module_size;
        if combined_count <= 3 {
            combined_x = (self.count as f32 * self.x() + j) / combined_count as f32;
            combined_y = (self.count as f32 * self.y() + i) / combined_count as f32;
            combined_module_size = (self.count as f32 * self.estimated_module_size
                + new_module_size)
                / combined_count as f32;
        }
        FinderPattern::with_count(combined_x, combined_y, combined_module_size, combined_count)
    }

    pub fn set_horizontal_check_state(&mut self, state: i32) {
        self.horizontal_state = match state {
            1 => HorizontalState::LeftSpill,
            2 => HorizontalState::RightSpill,
            _ => HorizontalState::Normal,
        };
    }

    pub fn set_vertical_check_state(&mut self, state: i32) {
        self.vertical_state = match state {
            1 => VerticalState::UpSpill,
            2 => VerticalState::DownSpill,
            _ => VerticalState::Normal,
        };
    }
}

/// Port of `AlignmentPattern`
#[derive(Clone, Copy, Debug)]
pub struct AlignmentPattern {
    pub point: ResultPoint,
    pub estimated_module_size: f32,
}

impl AlignmentPattern {
    pub fn new(pos_x: f32, pos_y: f32, estimated_module_size: f32) -> Self {
        Self {
            point: ResultPoint::new(pos_x, pos_y),
            estimated_module_size,
        }
    }

    #[inline]
    pub fn x(&self) -> f32 {
        self.point.x
    }

    #[inline]
    pub fn y(&self) -> f32 {
        self.point.y
    }

    pub fn about_equals(&self, module_size: f32, i: f32, j: f32) -> bool {
        if (i - self.y()).abs() <= module_size && (j - self.x()).abs() <= module_size {
            let module_size_diff = (module_size - self.estimated_module_size).abs();
            return module_size_diff <= 1.0 || module_size_diff <= self.estimated_module_size;
        }
        false
    }

    pub fn combine_estimate(&self, i: f32, j: f32, new_module_size: f32) -> AlignmentPattern {
        let combined_x = (self.x() + j) / 2.0;
        let combined_y = (self.y() + i) / 2.0;
        let combined_module_size = (self.estimated_module_size + new_module_size) / 2.0;
        AlignmentPattern::new(combined_x, combined_y, combined_module_size)
    }
}

/// Port of `FinderPatternInfo`: three finder patterns plus a confidence value
/// derived from the shape of the triangle they form.
#[derive(Clone, Copy, Debug)]
pub struct FinderPatternInfo {
    pub bottom_left: FinderPattern,
    pub top_left: FinderPattern,
    pub top_right: FinderPattern,
    pub possible_fix: f32,
    pub angle_possible_fix: f32,
}

impl FinderPatternInfo {
    pub fn new(pattern_centers: [FinderPattern; 3]) -> Self {
        let mut info = Self {
            bottom_left: pattern_centers[0],
            top_left: pattern_centers[1],
            top_right: pattern_centers[2],
            possible_fix: 0.0,
            angle_possible_fix: 0.0,
        };
        info.estimate_finder_pattern_info();
        info
    }

    /// Returns (long side squared, short side 1 squared, short side 2 squared);
    /// note that the original compares squared distances.
    pub fn calculate_sides(
        center_a: &FinderPattern,
        center_b: &FinderPattern,
        center_c: &FinderPattern,
    ) -> (f32, f32, f32) {
        let ab_s = {
            let dx = center_a.x() - center_b.x();
            let dy = center_a.y() - center_b.y();
            dx * dx + dy * dy
        };
        let ac_s = {
            let dx = center_a.x() - center_c.x();
            let dy = center_a.y() - center_c.y();
            dx * dx + dy * dy
        };
        let bc_s = {
            let dx = center_b.x() - center_c.x();
            let dy = center_b.y() - center_c.y();
            dx * dx + dy * dy
        };
        if ab_s > bc_s && ab_s > ac_s {
            (ab_s, ac_s, bc_s)
        } else if bc_s > ab_s && bc_s > ac_s {
            (bc_s, ab_s, ac_s)
        } else {
            (ac_s, ab_s, bc_s)
        }
    }

    /// The closer the triangle is to a right isosceles triangle, the higher the
    /// score; that score and the number of repeat hits on the finder patterns
    /// each contribute half.
    fn estimate_finder_pattern_info(&mut self) {
        let (long_side, short_side1, short_side2) =
            Self::calculate_sides(&self.bottom_left, &self.top_left, &self.top_right);

        let cos_long = (short_side1 + short_side2 - long_side)
            / (2.0 * short_side1.sqrt() * short_side2.sqrt());
        let cos_short1 =
            (long_side + short_side1 - short_side2) / (2.0 * long_side.sqrt() * short_side1.sqrt());
        let cos_short2 =
            (long_side + short_side2 - short_side1) / (2.0 * long_side.sqrt() * short_side2.sqrt());

        let to_deg = 180.0 / std::f32::consts::PI;
        let f_angle_long = cos_long.acos() * to_deg;
        let mut f_angle_short1 = cos_short1.acos() * to_deg;
        let mut f_angle_short2 = cos_short2.acos() * to_deg;
        if f_angle_short1 < f_angle_short2 {
            std::mem::swap(&mut f_angle_short1, &mut f_angle_short2);
        }

        let f_long_score = 100.0 - (f_angle_long - 90.0).abs();
        let f_short_diff = (f_angle_short1 - 45.0)
            .abs()
            .max((f_angle_short2 - 45.0).abs());
        let f_short_score = 100.0 - 2.0 * f_short_diff;
        let f_final_score = f_short_score.min(f_long_score);

        self.angle_possible_fix = f_final_score / 100.0;
        let total_count = self.bottom_left.count + self.top_left.count + self.top_right.count;
        let f_count_score = (total_count.min(10).max(3) - 3) as f32 / (10.0 - 3.0);
        self.possible_fix = self.angle_possible_fix * 0.5 + f_count_score * 0.5;
    }
}

/// Port of `PatternResult`: one group of finder patterns plus the version,
/// dimension and alignment pattern candidates derived from it.
pub struct PatternResult {
    pub finder_pattern_info: FinderPatternInfo,
    pub possible_alignment_patterns: Vec<AlignmentPattern>,
    pub confirmed_alignment_pattern: Option<AlignmentPattern>,
    pub possible_dimension: i32,
    pub possible_version: u32,
    pub possible_fix: f32,
    pub possible_module_size: f32,
}

impl PatternResult {
    pub fn new(info: FinderPatternInfo) -> Self {
        Self {
            finder_pattern_info: info,
            possible_alignment_patterns: Vec::new(),
            confirmed_alignment_pattern: None,
            possible_dimension: 0,
            possible_version: 0,
            possible_fix: 0.0,
            possible_module_size: 0.0,
        }
    }

    pub fn set_confirmed_alignment_pattern(&mut self, index: usize) {
        if index >= self.possible_alignment_patterns.len() {
            return;
        }
        self.confirmed_alignment_pattern = Some(self.possible_alignment_patterns[index]);
    }
}
