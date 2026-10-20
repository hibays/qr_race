//! Port of `zxing/qrcode/detector/alignment_pattern_finder.{hpp,cpp}`: looks
//! for a 1:1:1 alignment pattern inside a small window around the estimated
//! position.

use crate::common::bitmatrix::BitMatrix;
use crate::error::{reader, ZXResult};

use super::finder_pattern::AlignmentPattern;

pub struct AlignmentPatternFinder {
    possible_centers: Vec<AlignmentPattern>,
    start_x: i32,
    start_y: i32,
    width: i32,
    height: i32,
    module_size: f32,
}

impl AlignmentPatternFinder {
    pub fn new(start_x: i32, start_y: i32, width: i32, height: i32, module_size: f32) -> Self {
        Self {
            possible_centers: Vec::new(),
            start_x,
            start_y,
            width,
            height,
            module_size,
        }
    }

    /// Scans outwards alternately from the middle row of the window, since the
    /// middle rows are the most likely to hit.
    pub fn find(&mut self, image: &BitMatrix) -> ZXResult<AlignmentPattern> {
        let max_j = self.start_x + self.width;
        let middle_i = self.start_y + (self.height >> 1);

        for i_gen in 0..self.height {
            let i = middle_i
                + if (i_gen & 0x01) == 0 {
                    (i_gen + 1) >> 1
                } else {
                    -((i_gen + 1) >> 1)
                };
            if i < 0 || i >= image.height() as i32 {
                continue;
            }
            let mut state_count = [0i32; 3];
            let mut j = self.start_x;
            while j < max_j && image.get(j as usize, i as usize) == 0 {
                j += 1;
            }
            let mut current_state = 0usize;
            while j < max_j {
                if image.get(j as usize, i as usize) != 0 {
                    if current_state == 1 {
                        state_count[current_state] += 1;
                    } else if current_state == 2 {
                        if self.found_pattern_cross(&state_count) {
                            if let Some(confirmed) =
                                self.handle_possible_center(image, &state_count, i, j)
                            {
                                return Ok(confirmed);
                            }
                        }
                        state_count[0] = state_count[2];
                        state_count[1] = 1;
                        state_count[2] = 0;
                        current_state = 1;
                    } else {
                        current_state += 1;
                        state_count[current_state] += 1;
                    }
                } else {
                    if current_state == 1 {
                        current_state += 1;
                    }
                    state_count[current_state] += 1;
                }
                j += 1;
            }
            if self.found_pattern_cross(&state_count) {
                if let Some(confirmed) = self.handle_possible_center(image, &state_count, i, max_j)
                {
                    return Ok(confirmed);
                }
            }
        }

        if let Some(first) = self.possible_centers.first() {
            return Ok(*first);
        }
        Err(reader("Could not find alignment pattern"))
    }

    fn center_from_end(state_count: &[i32; 3], end: i32) -> f32 {
        (end - state_count[2]) as f32 - state_count[1] as f32 / 2.0
    }

    fn found_pattern_cross(&self, state_count: &[i32; 3]) -> bool {
        let max_variance = self.module_size / 2.0;
        for i in 0..3 {
            if (self.module_size - state_count[i] as f32).abs() >= max_variance {
                return false;
            }
        }
        true
    }

    fn cross_check_vertical(
        &self,
        image: &BitMatrix,
        start_i: i32,
        center_j: i32,
        max_count: i32,
        original_state_count_total: i32,
    ) -> f32 {
        let max_i = image.height() as i32;
        if center_j < 0 || center_j >= image.width() as i32 {
            return f32::NAN;
        }
        let mut state_count = [0i32; 3];
        let mut i = start_i;
        while i >= 0 && image.get(center_j as usize, i as usize) != 0 && state_count[1] <= max_count
        {
            state_count[1] += 1;
            i -= 1;
        }
        if i < 0 || state_count[1] > max_count {
            return f32::NAN;
        }
        while i >= 0 && image.get(center_j as usize, i as usize) == 0 && state_count[0] <= max_count
        {
            state_count[0] += 1;
            i -= 1;
        }
        if state_count[0] > max_count {
            return f32::NAN;
        }

        let mut i = start_i + 1;
        while i < max_i
            && image.get(center_j as usize, i as usize) != 0
            && state_count[1] <= max_count
        {
            state_count[1] += 1;
            i += 1;
        }
        if i == max_i || state_count[1] > max_count {
            return f32::NAN;
        }
        while i < max_i
            && image.get(center_j as usize, i as usize) == 0
            && state_count[2] <= max_count
        {
            state_count[2] += 1;
            i += 1;
        }
        if state_count[2] > max_count {
            return f32::NAN;
        }

        let state_count_total = state_count[0] + state_count[1] + state_count[2];
        if 5 * (state_count_total - original_state_count_total).abs()
            >= 2 * original_state_count_total
        {
            return f32::NAN;
        }
        if self.found_pattern_cross(&state_count) {
            Self::center_from_end(&state_count, i)
        } else {
            f32::NAN
        }
    }

    fn handle_possible_center(
        &mut self,
        image: &BitMatrix,
        state_count: &[i32; 3],
        i: i32,
        j: i32,
    ) -> Option<AlignmentPattern> {
        let state_count_total = state_count[0] + state_count[1] + state_count[2];
        let center_j = Self::center_from_end(state_count, j);
        let center_i = self.cross_check_vertical(
            image,
            i,
            center_j as i32,
            2 * state_count[1],
            state_count_total,
        );
        if !center_i.is_nan() {
            let estimated_module_size = state_count_total as f32 / 3.0;
            for center in self.possible_centers.iter() {
                if center.about_equals(estimated_module_size, center_i, center_j) {
                    return Some(center.combine_estimate(
                        center_i,
                        center_j,
                        estimated_module_size,
                    ));
                }
            }
            self.possible_centers.push(AlignmentPattern::new(
                center_j,
                center_i,
                estimated_module_size,
            ));
        }
        None
    }
}
