//! Port of `zxing/qrcode/detector/finder_pattern_finder.{hpp,cpp}`.
//!
//! This is the part the fork changed most. On top of the original ZXing
//! 1:1:3:1:1 scan it adds:
//!   * a fast scan based on the BitMatrix row run-length cache, instead of
//!     running a state machine pixel by pixel;
//!   * spill states (LEFT/RIGHT_SPILL) that let a finder pattern overflow the
//!     image boundary, so codes flush against an edge are still recognized;
//!   * an extra search pass based on connected-component area ratios (24:16:9);
//!   * clustering the candidate points with k-means before combining them, so
//!     that several FinderPatternInfo groups are produced instead of one.

use crate::common::bitmatrix::BitMatrix;
use crate::common::kmeans::k_means;
use crate::common::unicomblock::UnicomBlock;
use crate::error::{reader, ZXResult};

use super::finder_pattern::{FinderPattern, FinderPatternInfo};

/// Result state of the horizontal and vertical cross checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossCheckState {
    Normal = 0,
    LeftSpill = 1,
    RightSpill = 2,
    LeftRightSpill = 3,
    NotPattern = 4,
}

const CENTER_QUORUM: i32 = 2;
const MIN_SKIP: i32 = 1;
/// The module count of version 40. The original uses it to compute iSkip and
/// getMinModuleSize, but tryHarder is always true and getMinModuleSize has no
/// callers, so it is kept here only for reference.
#[allow(dead_code)]
const MAX_MODULES: i32 = 177;
const INTEGER_MATH_SHIFT: i32 = 8;
const FP_INPUT_CNN_MAX_NUM: usize = 10;
const FP_IS_SELECT_BEST: bool = true;
const FP_IS_SELECT_FILE_BEST: bool = true;
const FP_INPUT_MAX_NUM: usize = 100;
const FPS_CLUSTER_MAX: i32 = 4;
const FPS_RESULT_MAX: usize = 12;
const K_FACTOR: f32 = 2.0;
const FPS_MS_VAL: f32 = 1.0;
const FP_COUNT_MIN: i32 = 2;
const FP_MS_MIN: f32 = 1.0;
const FP_RIGHT_ANGLE: f32 = 0.342;
const FP_SMALL_ANGLE1: f32 = 0.8191;
const FP_SMALL_ANGLE2: f32 = 0.5736;
const QR_MIN_FP_AREA_ERR: f32 = 3.0;
const QR_MIN_FP_MS_ERR: f32 = 1.0;
const QR_MIN_FP_ACCEPT: f32 = 4.0;
const CENTER_CHECK_TIME: usize = 3;

#[derive(Clone, Copy)]
struct HorizontalCheckedResult {
    center_i: usize,
    center_j: f32,
}

pub struct FinderPatternFinder {
    pub possible_centers: Vec<FinderPattern>,
    has_skipped: bool,
    compared_finder_counts: usize,
    current_check_state: CrossCheckState,
    horizontal_checked_result: Vec<Vec<HorizontalCheckedResult>>,
}

impl Default for FinderPatternFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderPatternFinder {
    pub fn new() -> Self {
        Self {
            possible_centers: Vec::new(),
            has_skipped: false,
            compared_finder_counts: 0,
            current_check_state: CrossCheckState::Normal,
            horizontal_checked_result: Vec::new(),
        }
    }

    /// Main entry point. `use_nn_detector` corresponds to
    /// `DecodeHints::getUseNNDetector()`.
    pub fn find(
        &mut self,
        image: &mut BitMatrix,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
    ) -> ZXResult<Vec<FinderPatternInfo>> {
        self.find_cancellable(image, block, use_nn_detector, &crate::never_cancel())
    }

    /// Same as [`Self::find`], but checks `cancel` once per scanned row. When the
    /// four binarizations run in parallel, once an earlier one succeeds there is
    /// no point finishing the later ones, since the result depends on the order
    /// and a later one's result may not be used.
    pub fn find_cancellable(
        &mut self,
        image: &mut BitMatrix,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> ZXResult<Vec<FinderPatternInfo>> {
        let max_i = image.height();
        let max_j = image.width();
        self.horizontal_checked_result.clear();
        self.horizontal_checked_result.resize(max_j, Vec::new());

        // The original computes iSkip = 3*maxI/(4*MAX_MODULES), but tryHarder is
        // always true, so it always falls back to MIN_SKIP
        let mut i_skip = MIN_SKIP;

        image.init_row_counters();

        let tp0 = crate::clock::Instant::now();
        // Pass 1: find 1:1:3:1:1 patterns from the row runs
        let mut i: i64 = i_skip as i64 - 1;
        while i < max_i as i64 {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            let iu = i as usize;
            // Compute this row's run-length records first (they are lazy and
            // need &mut); everything after that uses read-only accessors that
            // borrow slices directly. This used to be two to_vec() calls: a
            // &mut accessor can hand out only one slice at a time, so holding
            // two at once required copying, which cost tens of thousands of
            // heap allocations and full copies per frame.
            image.ensure_row_records(iu);
            let row_counter_width = image.row_counter_offset_end_at(iu) as usize;
            let irow_states = image.row_records_at(iu);
            let irow_offsets = image.row_records_offset_at(iu);
            let mut rj: usize = usize::from(image.row_first_is_white(iu));

            while rj + 4 < row_counter_width && rj + 4 < max_j {
                let mut state_count = [0i32; 5];
                for k in 0..5 {
                    state_count[k] = irow_states[rj + k] as i32;
                }
                let j = irow_offsets[rj + 4] as usize + state_count[4] as usize;
                if j > max_j {
                    rj = row_counter_width.saturating_sub(1);
                    rj += 2;
                    continue;
                }
                if self.found_pattern_cross(&state_count) {
                    if j == max_j {
                        let confirmed = self.handle_possible_center(image, &state_count, iu, max_j);
                        if confirmed {
                            i_skip =
                                self.possible_centers.last().unwrap().estimated_module_size as i32;
                            if i_skip < 1 {
                                i_skip = 1;
                            }
                        }
                        rj = row_counter_width.saturating_sub(1);
                        rj += 2;
                        continue;
                    } else {
                        let confirmed = self.handle_possible_center(image, &state_count, iu, j);
                        if confirmed {
                            i_skip = 2;
                            if !self.has_skipped {
                                let row_skip = self.find_row_skip();
                                if row_skip > state_count[2] {
                                    i += (row_skip - state_count[2] - i_skip) as i64;
                                    rj = row_counter_width.saturating_sub(1);
                                }
                            }
                        } else {
                            rj += 2;
                            continue;
                        }
                        rj += 4;
                    }
                }
                rj += 2;
            }
            i += i_skip as i64;
        }

        let tp1 = crate::clock::Instant::now();
        let centers_after_pass1 = self.possible_centers.len() as u64;
        // Pass 2: extra search by connected-component area ratio
        // (outer black ring : white ring : center dot is about 24:16:9)
        let mut i: i64 = i_skip as i64 - 1;
        while i < max_i as i64 {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            let iu = i as usize;
            // Compute this row's run-length records first (they are lazy and
            // need &mut); everything after that uses read-only accessors that
            // borrow slices directly. This used to be two to_vec() calls: a
            // &mut accessor can hand out only one slice at a time, so holding
            // two at once required copying, which cost tens of thousands of
            // heap allocations and full copies per frame.
            image.ensure_row_records(iu);
            let row_counter_width = image.row_counter_offset_end_at(iu) as usize;
            let irow_states = image.row_records_at(iu);
            let irow_offsets = image.row_records_offset_at(iu);
            let mut rj: usize = usize::from(image.row_first_is_white(iu));

            while rj + 4 < row_counter_width {
                let off = |k: usize| irow_offsets[rj + k] as usize;
                let idx0 = block.get_unicom_block_index(image, iu, off(0));
                let idx1 = block.get_unicom_block_index(image, iu, off(1));
                let idx2 = block.get_unicom_block_index(image, iu, off(2));
                let idx3 = block.get_unicom_block_index(image, iu, off(3));
                let idx4 = block.get_unicom_block_index(image, iu, off(4));

                if idx0 == idx4 && idx1 == idx3 && idx0 != idx2 {
                    let i_black_cir = block.get_unicom_block_size(image, iu, off(0));
                    let i_white_cir = block.get_unicom_block_size(image, iu, off(1));
                    let i_black_pnt = block.get_unicom_block_size(image, iu, off(2));
                    if i_black_cir == -1 || i_white_cir == -1 {
                        rj += 2;
                        continue;
                    }
                    let f_black_cir = (i_black_cir as f32 / 24.0).sqrt();
                    let f_white_cir = (i_white_cir as f32 / 16.0).sqrt();
                    let f_black_pnt = (i_black_pnt as f32 / 9.0).sqrt();
                    let f_real_ms = ((i_white_cir + i_black_pnt) as f32 / 25.0).sqrt();
                    let mut i_total_count = 0i32;
                    for k in 1..4 {
                        i_total_count += irow_states[rj + k] as i32;
                    }
                    let f_est_row_ms = i_total_count as f32 / 5.0;

                    if (f_black_cir - f_white_cir).abs() <= QR_MIN_FP_AREA_ERR
                        && (f_white_cir - f_black_pnt).abs() <= QR_MIN_FP_AREA_ERR
                        && (f_real_ms - f_est_row_ms).abs() < QR_MIN_FP_MS_ERR
                    {
                        let (center_i, center_j);
                        if f_real_ms < QR_MIN_FP_ACCEPT {
                            center_i = iu as i32;
                            center_j = off(2) as i32 + irow_states[rj + 2] as i32 / 2;
                        } else {
                            let (min_y, min_x) =
                                block.get_min_point(image, iu, off(1)).unwrap_or((0, 0));
                            let (max_y, max_x) =
                                block.get_max_point(image, iu, off(3)).unwrap_or((0, 0));
                            center_i = (max_y + min_y) / 2;
                            center_j = (max_x + min_x) / 2;
                        }
                        self.try_to_push_to_centers(
                            center_i as f32,
                            center_j as f32,
                            f_real_ms,
                            CrossCheckState::Normal,
                            CrossCheckState::Normal,
                        );
                        let row_skip = self.find_row_skip();
                        if row_skip > irow_states[rj + 2] as i32 {
                            i += (row_skip - irow_states[rj + 2] as i32 - i_skip) as i64;
                            rj = row_counter_width.saturating_sub(1);
                        }
                        rj += 4;
                    }
                }
                rj += 2;
            }
            i += i_skip as i64;
        }
        let tp2 = crate::clock::Instant::now();
        record_finder_us(
            (tp1 - tp0).as_micros() as u64,
            (tp2 - tp1).as_micros() as u64,
            centers_after_pass1,
            self.possible_centers.len() as u64,
        );

        let mut pattern_infos = self.get_pattern_infos_file_mode(use_nn_detector)?;
        pattern_infos.sort_by(|a, b| {
            b.possible_fix
                .partial_cmp(&a.possible_fix)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(pattern_infos)
    }

    /// Merges a candidate center into `possible_centers`. If a nearby center
    /// already exists the two are combined and false is returned.
    fn try_to_push_to_centers(
        &mut self,
        center_i: f32,
        center_j: f32,
        estimated_module_size: f32,
        horizontal_state: CrossCheckState,
        vertical_state: CrossCheckState,
    ) -> bool {
        for index in 0..self.possible_centers.len() {
            let center = self.possible_centers[index];
            if center.about_equals(estimated_module_size, center_i, center_j) {
                let mut combined =
                    center.combine_estimate(center_i, center_j, estimated_module_size);
                combined.horizontal_state = if horizontal_state == CrossCheckState::Normal {
                    center.horizontal_state
                } else {
                    map_horizontal(horizontal_state)
                };
                combined.vertical_state = if vertical_state == CrossCheckState::Normal {
                    center.vertical_state
                } else {
                    map_vertical(vertical_state)
                };
                self.possible_centers[index] = combined;
                return false;
            }
        }
        let mut new_pattern = FinderPattern::new(center_j, center_i, estimated_module_size);
        new_pattern.horizontal_state = map_horizontal(horizontal_state);
        new_pattern.vertical_state = map_vertical(vertical_state);
        self.possible_centers.push(new_pattern);
        true
    }

    fn is_equal_result(src: &FinderPatternInfo, dst: &FinderPatternInfo) -> bool {
        src.top_left
            .about_equals(1.0, dst.top_left.y(), dst.top_left.x())
            && src
                .bottom_left
                .about_equals(1.0, dst.bottom_left.y(), dst.bottom_left.x())
            && src
                .top_right
                .about_equals(1.0, dst.top_right.y(), dst.top_right.x())
    }

    /// The three points must have close enough module sizes and form a right
    /// isosceles triangle.
    fn is_possible_find_pattern_info(
        a: &FinderPattern,
        b: &FinderPattern,
        c: &FinderPattern,
    ) -> bool {
        let a_ms = a.estimated_module_size;
        let b_ms = b.estimated_module_size;
        let c_ms = c.estimated_module_size;
        let avg = (a_ms + b_ms + c_ms) / 3.0;
        let val = ((a_ms - avg) * (a_ms - avg)
            + (b_ms - avg) * (b_ms - avg)
            + (c_ms - avg) * (c_ms - avg))
            .sqrt();
        if val >= FPS_MS_VAL {
            return false;
        }
        let mut long_size = 0.0f32;
        check_isosceles_right_triangle(a, b, c, &mut long_size)
    }

    fn push_to_result(
        a: FinderPattern,
        b: FinderPattern,
        c: FinderPattern,
        pattern_infos: &mut Vec<FinderPatternInfo>,
    ) {
        let ordered = order_best_patterns([a, b, c]);
        let pattern_info = FinderPatternInfo::new(ordered);
        for existing in pattern_infos.iter() {
            if Self::is_equal_result(existing, &pattern_info) {
                return;
            }
        }
        pattern_infos.push(pattern_info);
    }

    /// Port of `getPatternInfosFileMode`: builds several finder pattern triples
    /// out of the candidate points.
    fn get_pattern_infos_file_mode(
        &mut self,
        use_nn_detector: bool,
    ) -> ZXResult<Vec<FinderPatternInfo>> {
        let start_size = self.possible_centers.len();
        if start_size < 3 {
            return Err(reader("Could not find three finder patterns"));
        }
        let mut pattern_infos: Vec<FinderPatternInfo> = Vec::new();
        if start_size == 3 {
            Self::push_to_result(
                self.possible_centers[0],
                self.possible_centers[1],
                self.possible_centers[2],
                &mut pattern_infos,
            );
            return Ok(pattern_infos);
        }

        if FP_IS_SELECT_BEST {
            if let Ok(fp) = self.select_best_patterns() {
                Self::push_to_result(fp[0], fp[1], fp[2], &mut pattern_infos);
            }
        }
        if FP_IS_SELECT_FILE_BEST {
            if let Ok(fp) = self.select_file_best_patterns() {
                Self::push_to_result(fp[0], fp[1], fp[2], &mut pattern_infos);
            }
        }

        // Sort by module size, largest first
        self.possible_centers.sort_by(|a, b| {
            b.estimated_module_size
                .partial_cmp(&a.estimated_module_size)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut standard_centers: Vec<FinderPattern> = Vec::new();
        for c in self.possible_centers.iter() {
            if c.estimated_module_size >= FP_MS_MIN && c.count >= FP_COUNT_MIN {
                standard_centers.push(*c);
                if standard_centers.len() >= FP_INPUT_MAX_NUM {
                    break;
                }
                if use_nn_detector && standard_centers.len() >= FP_INPUT_CNN_MAX_NUM {
                    break;
                }
            }
        }
        if standard_centers.len() < 3 {
            return Err(reader("Could not find three finder patterns"));
        }

        // With few candidates, enumerate all triples directly
        if standard_centers.len() <= FP_INPUT_CNN_MAX_NUM {
            for x in 0..standard_centers.len() {
                for y in x + 1..standard_centers.len() {
                    for z in y + 1..standard_centers.len() {
                        if Self::is_possible_find_pattern_info(
                            &standard_centers[x],
                            &standard_centers[y],
                            &standard_centers[z],
                        ) {
                            Self::push_to_result(
                                standard_centers[x],
                                standard_centers[y],
                                standard_centers[z],
                                &mut pattern_infos,
                            );
                        }
                    }
                }
            }
            return Ok(pattern_infos);
        }

        // With many candidates: run k-means over (count, moduleSize) first, then
        // combine within each cluster
        const MAXEPOCHES: usize = 100;
        const MINCHANGED: usize = 0;
        let mut k = ((standard_centers.len() as f32).ln() * K_FACTOR) as i32 - 1;
        if k < 1 {
            k = 1;
        }
        let train_x: Vec<Vec<f64>> = standard_centers
            .iter()
            .map(|c| vec![c.count as f64, c.estimated_module_size as f64])
            .collect();
        let mut clusters_out = k_means(&train_x, k as usize, MAXEPOCHES, MINCHANGED);

        for i in 0..clusters_out.len() {
            let mut cluster_select = 0i32;
            if clusters_out[i].samples.len() < 3 {
                // Merge a too-small cluster into the next one
                if i < clusters_out.len() - 1 && clusters_out[i + 1].samples.len() < 3 {
                    let moved = clusters_out[i].samples.clone();
                    clusters_out[i + 1].samples.extend(moved);
                }
                continue;
            }
            let mut cluster_patterns: Vec<FinderPattern> = clusters_out[i]
                .samples
                .iter()
                .map(|&s| standard_centers[s])
                .collect();
            cluster_patterns.sort_by(best_comparator2);

            let n = clusters_out[i].samples.len();
            'outer: for x in 0..n {
                for y in x + 1..n {
                    for z in y + 1..n {
                        if cluster_select > FPS_CLUSTER_MAX || pattern_infos.len() > FPS_RESULT_MAX
                        {
                            break 'outer;
                        }
                        if Self::is_possible_find_pattern_info(
                            &cluster_patterns[x],
                            &cluster_patterns[y],
                            &cluster_patterns[z],
                        ) {
                            Self::push_to_result(
                                cluster_patterns[x],
                                cluster_patterns[y],
                                cluster_patterns[z],
                                &mut pattern_infos,
                            );
                            cluster_select += 1;
                        }
                    }
                }
            }
        }
        Ok(pattern_infos)
    }

    /// Computes the center column from the 5 runs.
    fn center_from_end(state_count: &[i32; 5], end: usize) -> f32 {
        end as f32
            - state_count[4] as f32
            - (state_count[3] + state_count[2] + state_count[1]) as f32 / 2.0
    }

    /// Checks whether the 5 runs match 1:1:3:1:1, recording whether either end
    /// spills over into `current_check_state`.
    fn found_pattern_cross(&mut self, state_count: &[i32; 5]) -> bool {
        let mut total_module_size = 0i32;
        let mut state_count_int = [0i32; 5];
        let min_module_size_int = 3i32 << INTEGER_MATH_SHIFT;

        for i in 0..5 {
            if state_count[i] <= 0 {
                return false;
            }
            state_count_int[i] = state_count[i] << INTEGER_MATH_SHIFT;
            total_module_size += state_count[i];
        }
        if total_module_size < 7 {
            return false;
        }
        self.current_check_state = CrossCheckState::NotPattern;
        let total_module_size = total_module_size << INTEGER_MATH_SHIFT;

        let mut module_size = (total_module_size - state_count_int[0] - state_count_int[4]) / 5;
        let mut max_variance = module_size;
        if module_size > min_module_size_int {
            max_variance = module_size / 2;
        }
        let left_fit = (module_size - state_count_int[0]).abs() <= max_variance;
        let right_fit = (module_size - state_count_int[4]).abs() <= max_variance;

        if left_fit {
            if right_fit {
                module_size = total_module_size / 7;
                self.current_check_state = CrossCheckState::Normal;
            } else {
                module_size = (total_module_size - state_count_int[4]) / 6;
                self.current_check_state = CrossCheckState::RightSpill;
            }
        } else if right_fit {
            module_size = (total_module_size - state_count_int[0]) / 6;
            self.current_check_state = CrossCheckState::LeftSpill;
        } else {
            self.current_check_state = CrossCheckState::LeftRightSpill;
        }

        (module_size - state_count_int[1]).abs() <= max_variance
            && (3 * module_size - state_count_int[2]).abs() <= 3 * max_variance
            && (module_size - state_count_int[3]).abs() <= max_variance
    }

    fn get_state_count_total(state_count: &[i32; 5], check_state: CrossCheckState) -> i32 {
        let mut total = state_count[1] + state_count[2] + state_count[3];
        match check_state {
            CrossCheckState::Normal => total += state_count[0] + state_count[4],
            CrossCheckState::LeftSpill => total += state_count[1] + state_count[4],
            CrossCheckState::RightSpill => total += state_count[0] + state_count[3],
            CrossCheckState::LeftRightSpill => total += state_count[1] + state_count[3],
            CrossCheckState::NotPattern => {}
        }
        total
    }

    /// Vertical cross check; returns the center row coordinate, or NaN on
    /// failure.
    fn cross_check_vertical(
        &mut self,
        image: &BitMatrix,
        start_i: usize,
        center_j: usize,
        max_count: i32,
        original_state_count_total: i32,
        estimated_vertical_module_size: &mut f32,
    ) -> f32 {
        let max_i = image.height() as i32;
        let mut state_count = [0i32; 5];
        let mut start_i = start_i as i32;

        if image.get(center_j, start_i as usize) == 0 {
            if start_i + 1 < max_i && image.get(center_j, (start_i + 1) as usize) != 0 {
                start_i += 1;
            } else if start_i - 1 > 0 && image.get(center_j, (start_i - 1) as usize) != 0 {
                start_i -= 1;
            } else {
                return f32::NAN;
            }
        }

        let mut ii = start_i;
        while ii >= 0 && image.get(center_j, ii as usize) != 0 {
            state_count[2] += 1;
            ii -= 1;
        }
        if ii < 0 {
            return f32::NAN;
        }
        while ii >= 0 && image.get(center_j, ii as usize) == 0 && state_count[1] <= max_count {
            state_count[1] += 1;
            ii -= 1;
        }
        if ii < 0 || state_count[1] > max_count {
            return f32::NAN;
        }
        let mut tmp_check_state = CrossCheckState::Normal;
        while ii >= 0 && image.get(center_j, ii as usize) != 0 {
            state_count[0] += 1;
            ii -= 1;
        }
        if state_count[0] >= max_count {
            tmp_check_state = CrossCheckState::LeftSpill;
        }

        let mut ii = start_i + 1;
        while ii < max_i && image.get(center_j, ii as usize) != 0 {
            state_count[2] += 1;
            ii += 1;
        }
        if ii == max_i {
            return f32::NAN;
        }
        while ii < max_i && image.get(center_j, ii as usize) == 0 && state_count[3] < max_count {
            state_count[3] += 1;
            ii += 1;
        }
        if ii == max_i || state_count[3] >= max_count {
            return f32::NAN;
        }
        if tmp_check_state == CrossCheckState::LeftSpill {
            while ii < max_i && image.get(center_j, ii as usize) != 0 && state_count[4] < max_count
            {
                state_count[4] += 1;
                ii += 1;
            }
            if state_count[4] >= max_count {
                return f32::NAN;
            }
        } else {
            while ii < max_i && image.get(center_j, ii as usize) != 0 {
                state_count[4] += 1;
                ii += 1;
            }
            if state_count[4] >= max_count {
                tmp_check_state = CrossCheckState::RightSpill;
            }
        }

        if !self.found_pattern_cross(&state_count) {
            return f32::NAN;
        }
        if (self.current_check_state == CrossCheckState::LeftSpill
            && tmp_check_state == CrossCheckState::RightSpill)
            || (self.current_check_state == CrossCheckState::RightSpill
                && tmp_check_state == CrossCheckState::LeftSpill)
        {
            return f32::NAN;
        }
        let state_count_total = Self::get_state_count_total(&state_count, self.current_check_state);
        if 5 * (state_count_total - original_state_count_total).abs()
            >= 2 * original_state_count_total
        {
            return f32::NAN;
        }
        *estimated_vertical_module_size = state_count_total as f32 / 7.0;
        Self::center_from_end(&state_count, ii as usize)
    }

    /// Horizontal cross check; returns the center column coordinate, or NaN on
    /// failure.
    fn cross_check_horizontal(
        &mut self,
        image: &BitMatrix,
        start_j: usize,
        center_i: usize,
        max_count: i32,
        original_state_count_total: i32,
        estimated_horizontal_module_size: &mut f32,
    ) -> f32 {
        let max_j = image.width() as i32;
        let mut state_count = [0i32; 5];
        let mut start_j = start_j as i32;

        if image.get(start_j as usize, center_i) == 0 {
            if start_j + 1 < max_j && image.get((start_j + 1) as usize, center_i) != 0 {
                start_j += 1;
            } else if start_j - 1 > 0 && image.get((start_j - 1) as usize, center_i) != 0 {
                start_j -= 1;
            } else {
                return f32::NAN;
            }
        }

        let row = image.row_bits(center_i);
        let mut j = start_j;
        while j >= 0 && row[j as usize] != 0 {
            state_count[2] += 1;
            j -= 1;
        }
        if j < 0 {
            return f32::NAN;
        }
        while j >= 0 && row[j as usize] == 0 && state_count[1] <= max_count {
            state_count[1] += 1;
            j -= 1;
        }
        if j < 0 || state_count[1] > max_count {
            return f32::NAN;
        }
        let mut tmp_check_state = CrossCheckState::Normal;
        while j >= 0 && row[j as usize] != 0 {
            state_count[0] += 1;
            j -= 1;
        }
        if state_count[0] >= max_count {
            tmp_check_state = CrossCheckState::LeftSpill;
        }

        let mut j = start_j + 1;
        while j < max_j && row[j as usize] != 0 {
            state_count[2] += 1;
            j += 1;
        }
        if j == max_j {
            return f32::NAN;
        }
        while j < max_j && row[j as usize] == 0 && state_count[3] < max_count {
            state_count[3] += 1;
            j += 1;
        }
        if j == max_j || state_count[3] >= max_count {
            return f32::NAN;
        }
        if tmp_check_state == CrossCheckState::LeftSpill {
            while j < max_j && row[j as usize] != 0 && state_count[4] <= max_count {
                state_count[4] += 1;
                j += 1;
            }
            if state_count[4] >= max_count {
                return f32::NAN;
            }
        } else {
            while j < max_j && row[j as usize] != 0 {
                state_count[4] += 1;
                j += 1;
            }
            if state_count[4] >= max_count {
                tmp_check_state = CrossCheckState::RightSpill;
            }
        }
        // The original has another advancing loop here (in practice a white pixel
        // has already been reached, so the loop never runs)
        while j < max_j && row[j as usize] != 0 {
            state_count[4] += 1;
            j += 1;
        }

        if !self.found_pattern_cross(&state_count) {
            return f32::NAN;
        }
        if (self.current_check_state == CrossCheckState::LeftSpill
            && tmp_check_state == CrossCheckState::RightSpill)
            || (self.current_check_state == CrossCheckState::RightSpill
                && tmp_check_state == CrossCheckState::LeftSpill)
        {
            return f32::NAN;
        }
        let state_count_total = Self::get_state_count_total(&state_count, self.current_check_state);
        if 5 * (state_count_total - original_state_count_total).abs() >= original_state_count_total
        {
            return f32::NAN;
        }
        *estimated_horizontal_module_size = state_count_total as f32 / 7.0;
        Self::center_from_end(&state_count, j as usize)
    }

    fn has_horizontal_checked_result(&self, start_j: usize, center_i: usize) -> f32 {
        if start_j >= self.horizontal_checked_result.len() {
            return -1.0;
        }
        for r in self.horizontal_checked_result[start_j].iter() {
            if r.center_i == center_i {
                return r.center_j;
            }
        }
        -1.0
    }

    fn add_horizontal_checked_result(&mut self, start_j: usize, center_i: usize, center_j: f32) {
        if start_j < self.horizontal_checked_result.len() {
            self.horizontal_checked_result[start_j]
                .push(HorizontalCheckedResult { center_i, center_j });
        }
    }

    /// After a horizontal hit, confirms a center with a vertical check and then
    /// another horizontal check, each allowed to retry at an offset of plus or
    /// minus one module size.
    fn handle_possible_center(
        &mut self,
        image: &BitMatrix,
        state_count: &[i32; 5],
        i: usize,
        j: usize,
    ) -> bool {
        #[cfg(feature = "profiling")]
        let _t = HpcTimer::new();
        let tmp_horizontal_state = self.current_check_state;
        let mut center_j = Self::center_from_end(state_count, j);
        let mut state_count_total = state_count[1] + state_count[2] + state_count[3];
        match tmp_horizontal_state {
            CrossCheckState::Normal => state_count_total += state_count[0] + state_count[4],
            CrossCheckState::LeftSpill => state_count_total += state_count[1] + state_count[4],
            CrossCheckState::RightSpill => state_count_total += state_count[0] + state_count[3],
            _ => {}
        }
        let estimated_horizontal_module_size = state_count_total as f32 / 7.0;
        let mut tolerate_module_size = if estimated_horizontal_module_size > 4.0 {
            estimated_horizontal_module_size / 2.0
        } else {
            1.0
        };
        let possible_center_js = [
            center_j,
            center_j - tolerate_module_size,
            center_j + tolerate_module_size,
        ];
        let image_height = image.height() as f32;
        let image_width = image.width() as f32;

        for k in 0..CENTER_CHECK_TIME {
            let possible_center_j = possible_center_js[k];
            if possible_center_j < 0.0 || possible_center_j >= image_width {
                continue;
            }
            let mut estimated_vertical_module_size = 0.0f32;
            let center_i = self.cross_check_vertical(
                image,
                i,
                possible_center_j as usize,
                state_count[2],
                state_count_total,
                &mut estimated_vertical_module_size,
            );
            if center_i.is_nan() || center_i < 0.0 {
                continue;
            }
            let tmp_vertical_state = self.current_check_state;
            let module_size_diff =
                (estimated_horizontal_module_size - estimated_vertical_module_size).abs();
            if module_size_diff > estimated_horizontal_module_size
                || module_size_diff > estimated_vertical_module_size
            {
                return false;
            }
            tolerate_module_size = if estimated_vertical_module_size > 4.0 {
                estimated_vertical_module_size / 2.0
            } else {
                1.0
            };
            let possible_center_is = [
                center_i,
                center_i - tolerate_module_size,
                center_i + tolerate_module_size,
            ];
            for l in 0..CENTER_CHECK_TIME {
                let possible_center_i = possible_center_is[l];
                if possible_center_i < 0.0 || possible_center_i >= image_height {
                    continue;
                }
                let mut re_estimated_horizontal_module_size = 0.0f32;
                let cj = self
                    .has_horizontal_checked_result(center_j as usize, possible_center_i as usize);
                if cj >= 0.0 {
                    center_j = cj;
                } else {
                    let cj = center_j;
                    let ccj = self.cross_check_horizontal(
                        image,
                        cj as usize,
                        possible_center_i as usize,
                        state_count[2],
                        state_count_total,
                        &mut re_estimated_horizontal_module_size,
                    );
                    if !ccj.is_nan() {
                        center_j = ccj;
                        self.add_horizontal_checked_result(
                            cj as usize,
                            possible_center_i as usize,
                            ccj,
                        );
                    }
                }
                if !center_j.is_nan() {
                    self.try_to_push_to_centers(
                        center_i,
                        center_j,
                        (estimated_horizontal_module_size + estimated_vertical_module_size) / 2.0,
                        tmp_horizontal_state,
                        tmp_vertical_state,
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Once two centers of the same size have been confirmed, a number of rows
    /// can be skipped outright.
    fn find_row_skip(&mut self) -> i32 {
        let max = self.possible_centers.len();
        if max <= 1 {
            return 0;
        }
        if max <= self.compared_finder_counts {
            return 0;
        }
        for i in 0..max - 1 {
            let first = self.possible_centers[i];
            if first.count >= CENTER_QUORUM {
                let first_module_size = first.estimated_module_size;
                let j_start = if i < self.compared_finder_counts {
                    self.compared_finder_counts
                } else {
                    i + 1
                };
                for j in j_start..max {
                    let second = self.possible_centers[j];
                    if second.count >= CENTER_QUORUM {
                        let module_size_diff =
                            (first_module_size - second.estimated_module_size).abs();
                        if module_size_diff < 1.0 {
                            self.has_skipped = true;
                            return (((first.x() - second.x()).abs()
                                - (first.y() - second.y()).abs())
                                / 2.0) as i32;
                        }
                    }
                }
            }
        }
        self.compared_finder_counts = max;
        0
    }

    /// Port of `selectBestPatterns`: a chain of heuristics that picks the three
    /// most plausible centers.
    fn select_best_patterns(&mut self) -> ZXResult<[FinderPattern; 3]> {
        let start_size = self.possible_centers.len();
        if start_size < 3 {
            return Err(reader("Could not find three finder patterns"));
        }
        if start_size == 3 {
            return Ok([
                self.possible_centers[0],
                self.possible_centers[1],
                self.possible_centers[2],
            ]);
        }

        self.possible_centers.sort_by(|a, b| b.count.cmp(&a.count));

        // The top three are hit noticeably more often than the fourth
        if (self.possible_centers[2].count - self.possible_centers[3].count) > 1
            && self.possible_centers[2].count > 1
        {
            return Ok([
                self.possible_centers[0],
                self.possible_centers[1],
                self.possible_centers[2],
            ]);
        } else if self.possible_centers[3].count > 1 {
            // Drop the most outlying module size among the top four
            let total: f32 = self.possible_centers[..4]
                .iter()
                .map(|c| c.estimated_module_size)
                .sum();
            let average = total / 4.0;
            let mut max_diff = 0.0f32;
            let mut max_id = 0usize;
            for i in 0..4 {
                let diff = (self.possible_centers[i].estimated_module_size - average).abs();
                if diff > max_diff {
                    max_diff = diff;
                    max_id = i;
                }
            }
            let idx: [usize; 3] = match max_id {
                0 => [1, 2, 3],
                1 => [0, 2, 3],
                2 => [0, 1, 3],
                _ => [0, 1, 2],
            };
            return Ok([
                self.possible_centers[idx[0]],
                self.possible_centers[idx[1]],
                self.possible_centers[idx[2]],
            ]);
        } else if self.possible_centers[1].count > 1 && self.possible_centers[2].count == 1 {
            // Only two reliable centers: among the rest, find the third point
            // that forms the largest right isosceles triangle with them
            let possible_module_size = (self.possible_centers[0].estimated_module_size
                + self.possible_centers[1].estimated_module_size)
                / 2.0;
            let mut possible_third_center: Vec<FinderPattern> = Vec::new();
            for i in 2..start_size {
                if (self.possible_centers[i].estimated_module_size - possible_module_size).abs()
                    < 0.5 * possible_module_size
                {
                    possible_third_center.push(self.possible_centers[i]);
                }
            }
            let mut longest_side = 0.0f32;
            let mut longest_id = 0usize;
            for (i, third) in possible_third_center.iter().enumerate() {
                let mut tmp_long_side = 0.0f32;
                if check_isosceles_right_triangle(
                    &self.possible_centers[0],
                    &self.possible_centers[1],
                    third,
                    &mut tmp_long_side,
                ) && tmp_long_side >= longest_side
                {
                    longest_side = tmp_long_side;
                    longest_id = i;
                }
            }
            if longest_id >= possible_third_center.len() {
                return Err(reader("Not find any available possibleThirdCenter"));
            }
            return Ok([
                self.possible_centers[0],
                self.possible_centers[1],
                possible_third_center[longest_id],
            ]);
        }

        // Drop centers whose module size is an outlier
        if start_size > 3 {
            let mut total_module_size = 0.0f32;
            let mut square = 0.0f32;
            for c in self.possible_centers.iter() {
                let size = c.estimated_module_size;
                total_module_size += size;
                square += size * size;
            }
            let average = total_module_size / start_size as f32;
            let std_dev = (square / start_size as f32 - average * average).sqrt();
            self.possible_centers.sort_by(|a, b| {
                let da = (a.estimated_module_size - average).abs();
                let db = (b.estimated_module_size - average).abs();
                db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
            });
            let limit = (0.5 * average).max(std_dev);
            let mut i = 0usize;
            while i < self.possible_centers.len() && self.possible_centers.len() > 3 {
                if (self.possible_centers[i].estimated_module_size - average).abs() > limit {
                    self.possible_centers.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        const TRY_HARD_POSSIBLE_CENTER_SIZE: usize = 15;
        const POSSIBLE_CENTER_SIZE: usize = 12;
        if self.possible_centers.len() > TRY_HARD_POSSIBLE_CENTER_SIZE {
            self.possible_centers.sort_by(|a, b| b.count.cmp(&a.count));
            self.possible_centers
                .truncate(TRY_HARD_POSSIBLE_CENTER_SIZE);
        } else if self.possible_centers.len() > POSSIBLE_CENTER_SIZE {
            self.possible_centers.sort_by(|a, b| b.count.cmp(&a.count));
            self.possible_centers.truncate(POSSIBLE_CENTER_SIZE);
        }

        if self.possible_centers.len() >= 6 {
            // Take the 4 smallest and 2 largest by x, then narrow each group down
            // to 3 by y
            self.possible_centers.sort_by(|a, b| {
                a.x()
                    .partial_cmp(&b.x())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let len = self.possible_centers.len();
            self.possible_centers.drain(4..len - 2);
            self.possible_centers[0..4].sort_by(|a, b| {
                a.y()
                    .partial_cmp(&b.y())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.possible_centers.drain(1..3);
            let len = self.possible_centers.len();
            self.possible_centers[len - 2..].sort_by(|a, b| {
                a.y()
                    .partial_cmp(&b.y())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.possible_centers.truncate(len - 1);
        } else if self.possible_centers.len() > 3 {
            let total: f32 = self
                .possible_centers
                .iter()
                .map(|c| c.estimated_module_size)
                .sum();
            let average = total / self.possible_centers.len() as f32;
            self.possible_centers
                .sort_by(|a, b| center_comparator(a, b, average));
            self.possible_centers.truncate(3);
        }

        if self.possible_centers.len() < 3 {
            return Err(reader("Could not find three finder patterns"));
        }
        Ok([
            self.possible_centers[0],
            self.possible_centers[1],
            self.possible_centers[2],
        ])
    }

    /// Port of `selectFileBestPatterns`: sorts by (count, moduleSize) and takes
    /// the first run of three consecutive points that form a triangle.
    fn select_file_best_patterns(&mut self) -> ZXResult<[FinderPattern; 3]> {
        let start_size = self.possible_centers.len();
        if start_size < 3 {
            return Err(reader("Could not find three finder patterns"));
        }
        if start_size == 3 {
            return Ok([
                self.possible_centers[0],
                self.possible_centers[1],
                self.possible_centers[2],
            ]);
        }
        self.possible_centers.sort_by(best_comparator);
        let mut result = [
            self.possible_centers[0],
            self.possible_centers[1],
            self.possible_centers[2],
        ];
        for i in 0..self.possible_centers.len() - 2 {
            let mut tmp_long_side = 0.0f32;
            let mut i_count_diff = 0i32;
            let mut f_module_size_diff = 0.0f32;
            for j in 0..3 {
                i_count_diff += (self.possible_centers[i + j].count
                    - self.possible_centers[i + ((j + 1) % 3)].count)
                    .abs();
                f_module_size_diff += (self.possible_centers[i + j].estimated_module_size
                    - self.possible_centers[i + ((j + 1) % 3)].estimated_module_size)
                    .abs();
            }
            if i_count_diff > 2 {
                continue;
            }
            if f_module_size_diff > 5.0 {
                continue;
            }
            if check_isosceles_right_triangle(
                &self.possible_centers[i],
                &self.possible_centers[i + 1],
                &self.possible_centers[i + 2],
                &mut tmp_long_side,
            ) {
                result = [
                    self.possible_centers[i],
                    self.possible_centers[i + 1],
                    self.possible_centers[i + 2],
                ];
                break;
            }
        }
        Ok(result)
    }
}

fn map_horizontal(state: CrossCheckState) -> super::finder_pattern::HorizontalState {
    use super::finder_pattern::HorizontalState as H;
    match state {
        CrossCheckState::LeftSpill => H::LeftSpill,
        CrossCheckState::RightSpill => H::RightSpill,
        _ => H::Normal,
    }
}

fn map_vertical(state: CrossCheckState) -> super::finder_pattern::VerticalState {
    use super::finder_pattern::VerticalState as V;
    match state {
        // The vertical direction reuses the same CrossCheckState: LEFT=UP,
        // RIGHT=DOWN
        CrossCheckState::LeftSpill => V::UpSpill,
        CrossCheckState::RightSpill => V::DownSpill,
        _ => V::Normal,
    }
}

/// Port of `BestComparator`
fn best_comparator(a: &FinderPattern, b: &FinderPattern) -> std::cmp::Ordering {
    if a.count != b.count {
        b.count.cmp(&a.count)
    } else {
        b.estimated_module_size
            .partial_cmp(&a.estimated_module_size)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Port of `BestComparator2`: at equal count, prefers candidates that do not
/// spill.
fn best_comparator2(a: &FinderPattern, b: &FinderPattern) -> std::cmp::Ordering {
    use super::finder_pattern::{HorizontalState, VerticalState};
    if a.count != b.count {
        return b.count.cmp(&a.count);
    }
    let err = |p: &FinderPattern| {
        let mut e = 0;
        if p.horizontal_state != HorizontalState::Normal {
            e += 1;
        }
        if p.vertical_state != VerticalState::Normal {
            e += 1;
        }
        e
    };
    let (ae, be) = (err(a), err(b));
    if ae != be {
        ae.cmp(&be)
    } else {
        b.estimated_module_size
            .partial_cmp(&a.estimated_module_size)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Port of `CenterComparator`
fn center_comparator(a: &FinderPattern, b: &FinderPattern, average: f32) -> std::cmp::Ordering {
    if a.count != b.count {
        b.count.cmp(&a.count)
    } else {
        let da = (a.estimated_module_size - average).abs();
        let db = (b.estimated_module_size - average).abs();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Port of `FinderPatternFinder::orderBestPatterns`: returns
/// [bottomLeft, topLeft, topRight].
pub fn order_best_patterns(patterns: [FinderPattern; 3]) -> [FinderPattern; 3] {
    let dist = |a: &FinderPattern, b: &FinderPattern| {
        let dx = a.x() - b.x();
        let dy = a.y() - b.y();
        (dx * dx + dy * dy).sqrt()
    };
    let ab = dist(&patterns[0], &patterns[1]);
    let bc = dist(&patterns[1], &patterns[2]);
    let ac = dist(&patterns[0], &patterns[2]);

    // The point opposite the longest side is the top-left corner
    let (top_left, mut top_right, mut bottom_left) = if bc >= ab && bc >= ac {
        (patterns[0], patterns[1], patterns[2])
    } else if ac >= bc && ac >= ab {
        (patterns[1], patterns[0], patterns[2])
    } else {
        (patterns[2], patterns[0], patterns[1])
    };

    if (bottom_left.y() - top_left.y()) * (top_right.x() - top_left.x())
        < (bottom_left.x() - top_left.x()) * (top_right.y() - top_left.y())
    {
        std::mem::swap(&mut top_right, &mut bottom_left);
    }
    [bottom_left, top_left, top_right]
}

/// Port of `checkIsoscelesRightTriangle`: whether the three points form a large
/// enough right isosceles triangle.
pub fn check_isosceles_right_triangle(
    center_a: &FinderPattern,
    center_b: &FinderPattern,
    center_c: &FinderPattern,
    long_side: &mut f32,
) -> bool {
    let (ls, short_side1, short_side2) =
        FinderPatternInfo::calculate_sides(center_a, center_b, center_c);
    *long_side = ls;

    let short_side_sqrt1 = short_side1.sqrt();
    let short_side_sqrt2 = short_side2.sqrt();
    let long_side_sqrt = ls.sqrt();
    let min_side = short_side_sqrt1.min(short_side_sqrt2).min(long_side_sqrt);
    let max_module_size = center_a
        .estimated_module_size
        .max(center_b.estimated_module_size)
        .max(center_c.estimated_module_size);
    // The points must be at least 14 modules apart; points closer than that
    // cannot be three corners of the same code
    if min_side <= max_module_size * 14.0 {
        return false;
    }

    let cos_long = (short_side1 + short_side2 - ls) / (2.0 * short_side_sqrt1 * short_side_sqrt2);
    let cos_short1 = (ls + short_side1 - short_side2) / (2.0 * long_side_sqrt * short_side_sqrt1);
    let cos_short2 = (ls + short_side2 - short_side1) / (2.0 * long_side_sqrt * short_side_sqrt2);

    !(cos_long.abs() > FP_RIGHT_ANGLE
        || (cos_short1 < FP_SMALL_ANGLE2 || cos_short1 > FP_SMALL_ANGLE1)
        || (cos_short2 < FP_SMALL_ANGLE2 || cos_short2 > FP_SMALL_ANGLE1))
}

/// Call count and cumulative time of `handle_possible_center`. On noisy images
/// it is called tens of thousands of times per frame, so like the BFS
/// instrumentation it is compiled only under the `profiling` feature.
#[cfg(feature = "profiling")]
pub static HPC_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static HPC_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "profiling")]
struct HpcTimer(crate::clock::Instant);
#[cfg(feature = "profiling")]
impl HpcTimer {
    fn new() -> Self {
        HPC_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(crate::clock::Instant::now())
    }
}
#[cfg(feature = "profiling")]
impl Drop for HpcTimer {
    fn drop(&mut self) {
        HPC_US.fetch_add(
            self.0.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// Takes and resets: (cumulative handle_possible_center us, call count)
#[cfg(feature = "profiling")]
pub fn take_hpc_us() -> (u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (HPC_US.swap(0, Relaxed), HPC_N.swap(0, Relaxed))
}

/// Per-stage timings and candidate counts of the finder; used only for locating
/// bottlenecks.
pub static FP_PASS1_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FP_PASS2_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FP_C1: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FP_C2: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn record_finder_us(pass1: u64, pass2: u64, c1: u64, c2: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    FP_PASS1_US.fetch_add(pass1, Relaxed);
    FP_PASS2_US.fetch_add(pass2, Relaxed);
    FP_C1.fetch_add(c1, Relaxed);
    FP_C2.fetch_add(c2, Relaxed);
    FP_N.fetch_add(1, Relaxed);
}

/// Takes and resets: (pass 1 us, pass 2 us, candidates after pass 1, candidates
/// after pass 2, call count)
pub fn take_finder_us() -> (u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        FP_PASS1_US.swap(0, Relaxed),
        FP_PASS2_US.swap(0, Relaxed),
        FP_C1.swap(0, Relaxed),
        FP_C2.swap(0, Relaxed),
        FP_N.swap(0, Relaxed),
    )
}
