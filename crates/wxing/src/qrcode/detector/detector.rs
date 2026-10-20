//! Port of `zxing/qrcode/detector/detector.{hpp,cpp}`.
//!
//! On top of the original, the fork adds:
//!   * the finder pattern spill state taking part in module size estimation
//!     (the `patternState` argument of `calculateModuleSizeOneWay`);
//!   * predicting the alignment pattern position by fitting the top-right and
//!     bottom-left edge lines and intersecting them
//!     (`findAlignmentWithFitLine`);
//!   * producing several alignment pattern candidates per detection (including
//!     fixed-up ones and a (0,0) fallback), which the caller tries to decode one
//!     by one;
//!   * allowing the dimension to be corrected by +-1 or +-2.

use crate::common::bitmatrix::BitMatrix;
use crate::common::grid_sampler::sample_grid;
use crate::common::mathutils;
use crate::common::perspective_transform::PerspectiveTransform;
use crate::common::unicomblock::UnicomBlock;
use crate::error::{reader, ZXResult};
use crate::qrcode::version::Version;
use crate::result::{DetectorResult, ResultPoint};
use cvlite::cv_round;

use super::alignment_pattern_finder::AlignmentPatternFinder;
use super::finder_pattern::{AlignmentPattern, FinderPatternInfo, PatternResult};
use super::finder_pattern::{HorizontalState, VerticalState};
use super::finder_pattern_finder::FinderPatternFinder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectorState {
    Start = 10,
    FindFinderPattern = 11,
    FindAlignPattern = 12,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Overflow state of a finder pattern along one direction, used by
/// `calculateModuleSizeOneWay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternState {
    Normal,
    LeftOrUpSpill,
    RightOrDownSpill,
}

impl PatternState {
    fn from_horizontal(s: HorizontalState) -> Self {
        match s {
            HorizontalState::LeftSpill => PatternState::LeftOrUpSpill,
            HorizontalState::RightSpill => PatternState::RightOrDownSpill,
            HorizontalState::Normal => PatternState::Normal,
        }
    }
    fn from_vertical(s: VerticalState) -> Self {
        match s {
            VerticalState::UpSpill => PatternState::LeftOrUpSpill,
            VerticalState::DownSpill => PatternState::RightOrDownSpill,
            VerticalState::Normal => PatternState::Normal,
        }
    }
}

pub struct Detector {
    pub possible_pattern_results: Vec<PatternResult>,
    pub state: DetectorState,
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl Detector {
    pub fn new() -> Self {
        Self {
            possible_pattern_results: Vec::new(),
            state: DetectorState::Start,
        }
    }

    /// Finds every candidate finder pattern triple.
    pub fn detect(
        &mut self,
        image: &mut BitMatrix,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
    ) -> ZXResult<()> {
        self.detect_cancellable(image, block, use_nn_detector, crate::never_cancel())
    }

    /// Same as [`Self::detect`], but passes the cancellation flag through to the
    /// finder; see `find_cancellable`.
    pub fn detect_cancellable(
        &mut self,
        image: &mut BitMatrix,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> ZXResult<()> {
        let mut finder = FinderPatternFinder::new();
        let finder_infos = finder.find_cancellable(image, block, use_nn_detector, cancel)?;
        self.possible_pattern_results.clear();
        for info in finder_infos {
            self.possible_pattern_results.push(PatternResult::new(info));
        }
        self.state = DetectorState::FindFinderPattern;
        Ok(())
    }

    pub fn possible_pattern_count(&self) -> usize {
        self.possible_pattern_results.len()
    }

    /// Lazily computes the alignment pattern candidates for finder pattern group
    /// `idx` and returns how many there are.
    pub fn possible_alignment_count(&mut self, image: &BitMatrix, idx: usize) -> i32 {
        if idx >= self.possible_pattern_results.len() {
            return -1;
        }
        if self.possible_pattern_results[idx]
            .possible_alignment_patterns
            .is_empty()
        {
            let info = self.possible_pattern_results[idx].finder_pattern_info;
            match self.process_finder_pattern_info(image, info) {
                Ok(result) => self.possible_pattern_results[idx] = result,
                Err(_) => return -1,
            }
        }
        self.possible_pattern_results[idx]
            .possible_alignment_patterns
            .len() as i32
    }

    /// Performs perspective sampling with the parameters of group
    /// (patternIdx, alignmentIdx) to obtain the module matrix.
    pub fn get_result_via_alignment(
        &self,
        image: &BitMatrix,
        pattern_idx: usize,
        alignment_idx: usize,
        possible_dimension: i32,
    ) -> ZXResult<DetectorResult> {
        if pattern_idx >= self.possible_pattern_results.len() {
            return Err(reader("bad pattern index"));
        }
        let pr = &self.possible_pattern_results[pattern_idx];
        if alignment_idx >= pr.possible_alignment_patterns.len() {
            return Err(reader("bad alignment index"));
        }
        let possible_dimension = if possible_dimension <= 0 {
            pr.possible_dimension
        } else {
            possible_dimension
        };
        let info = pr.finder_pattern_info;
        let alignment = pr.possible_alignment_patterns[alignment_idx];

        let transform = create_transform(
            &info.top_left.point,
            &info.top_right.point,
            &info.bottom_left.point,
            Some(&alignment.point),
            possible_dimension,
        );
        let bits = sample_grid(image, possible_dimension as usize, &transform)?;

        // Map the four corners (bottom-left, top-left, top-right, bottom-right)
        // back to source image coordinates
        let d = possible_dimension as f32;
        let mut points = vec![0.0, d, 0.0, 0.0, d, 0.0, d, d];
        transform.transform_points(&mut points);
        let corners = vec![
            ResultPoint::new(points[0], points[1]),
            ResultPoint::new(points[2], points[3]),
            ResultPoint::new(points[4], points[5]),
            ResultPoint::new(points[6], points[7]),
        ];
        Ok(DetectorResult {
            bits,
            points: corners,
            dimension: possible_dimension,
            modulesize: pr.possible_module_size,
        })
    }

    fn has_same_result(
        possible_alignment_patterns: &[AlignmentPattern],
        alignment_pattern: &AlignmentPattern,
    ) -> bool {
        let mut module_size = alignment_pattern.estimated_module_size / 5.0;
        if module_size < 1.0 {
            module_size = 1.0;
        }
        possible_alignment_patterns
            .iter()
            .any(|p| p.about_equals(module_size, alignment_pattern.y(), alignment_pattern.x()))
    }

    /// Enlarges the search window by 2, 4, 8 and so on times the module size
    /// until an alignment pattern is found.
    fn get_nearest_alignment_pattern(
        &self,
        image: &BitMatrix,
        try_find_range: i32,
        module_size: f32,
        est_alignment_x: i32,
        est_alignment_y: i32,
    ) -> Option<AlignmentPattern> {
        let mut i = 2;
        while i <= try_find_range {
            if let Ok(ap) = self.find_alignment_in_region(
                image,
                module_size,
                est_alignment_x,
                est_alignment_y,
                i as f32,
            ) {
                return Some(ap);
            }
            i <<= 1;
        }
        None
    }

    /// Port of `processFinderPatternInfo`: derives the module size, dimension,
    /// version and alignment pattern candidates from the three finder patterns.
    pub fn process_finder_pattern_info(
        &mut self,
        image: &BitMatrix,
        info: FinderPatternInfo,
    ) -> ZXResult<PatternResult> {
        let top_left = info.top_left;
        let top_right = info.top_right;
        let bottom_left = info.bottom_left;
        let mut result = PatternResult::new(info);

        let mut module_size_x = self.calculate_module_size_one_way(
            image,
            &top_left.point,
            &top_right.point,
            PatternState::from_horizontal(top_left.horizontal_state),
            PatternState::from_horizontal(top_right.horizontal_state),
        );
        let mut module_size_y = self.calculate_module_size_one_way(
            image,
            &top_left.point,
            &bottom_left.point,
            PatternState::from_vertical(top_left.vertical_state),
            PatternState::from_vertical(bottom_left.vertical_state),
        );
        if module_size_x < 1.0 || module_size_y < 1.0 {
            return Err(reader("bad midule size"));
        }
        let mut module_size = (module_size_x + module_size_y) / 2.0;
        // If the estimate is clearly larger than the sizes the three finder
        // patterns report themselves, use their average instead
        if module_size > top_left.estimated_module_size * 1.05
            && module_size > top_right.estimated_module_size * 1.05
            && module_size > bottom_left.estimated_module_size * 1.05
        {
            module_size = (top_left.estimated_module_size
                + top_right.estimated_module_size
                + bottom_left.estimated_module_size)
                / 3.0;
            module_size_x = module_size;
            module_size_y = module_size;
        }
        result.possible_module_size = module_size;
        if module_size < 1.0 {
            return Err(reader("bad midule size"));
        }

        let ori_dimension = compute_dimension(
            &top_left.point,
            &top_right.point,
            &bottom_left.point,
            module_size_x,
            module_size_y,
        );
        let mut dimension = ori_dimension;
        let mut provisional_version: Option<&'static Version> = None;
        for diff in [0, 1, -1, 2, -2] {
            dimension = ori_dimension + diff;
            if dimension <= 0 {
                continue;
            }
            if let Ok(v) = Version::provisional_for_dimension(dimension as usize) {
                provisional_version = Some(v);
                break;
            }
        }
        let provisional_version = match provisional_version {
            Some(v) => v,
            None => return Err(reader("Cannot get version number")),
        };
        result.possible_dimension = dimension;
        result.possible_version = provisional_version.version_number() as u32;

        let modules_between_fp_centers = provisional_version.dimension_for_version()? as i32 - 7;

        // Extrapolate the bottom-right corner from the three finder patterns,
        // then scale back proportionally to the alignment pattern position
        let bottom_right_x = top_right.x() - top_left.x() + bottom_left.x();
        let bottom_right_y = top_right.y() - top_left.y() + bottom_left.y();
        let correction_to_top_left = 1.0 - 3.0 / modules_between_fp_centers as f32;
        let est_alignment_x =
            (top_left.x() + correction_to_top_left * (bottom_right_x - top_left.x())) as i32;
        let est_alignment_y =
            (top_left.y() + correction_to_top_left * (bottom_right_y - top_left.y())) as i32;
        let estimate_center =
            AlignmentPattern::new(est_alignment_x as f32, est_alignment_y as f32, module_size);

        // Intersect the two fitted lines
        let mut found_fit_line = false;
        let mut fit_line_center = AlignmentPattern::new(0.0, 0.0, module_size);
        if let Ok(c) = self.find_alignment_with_fit_line(
            image,
            &top_left.point,
            &top_right.point,
            &bottom_left.point,
            module_size,
        ) {
            if mathutils::is_in_range(c.x(), c.y(), image.width() as f32, image.height() as f32) {
                found_fit_line = true;
                fit_line_center = c;
            }
        }

        if !provisional_version.alignment_pattern_centers().is_empty() {
            let try_find_range = provisional_version.dimension_for_version()? as i32 / 2;
            if found_fit_line {
                if let Some(fit_ap) = self.get_nearest_alignment_pattern(
                    image,
                    try_find_range,
                    module_size,
                    fit_line_center.x() as i32,
                    fit_line_center.y() as i32,
                ) {
                    if !Self::has_same_result(&result.possible_alignment_patterns, &fit_ap) {
                        result.possible_alignment_patterns.push(fit_ap);
                    }
                }
            }
            if let Some(est_ap) = self.get_nearest_alignment_pattern(
                image,
                try_find_range,
                module_size,
                estimate_center.x() as i32,
                estimate_center.y() as i32,
            ) {
                if !Self::has_same_result(&result.possible_alignment_patterns, &est_ap) {
                    result.possible_alignment_patterns.push(est_ap);
                }
            }
        }

        if found_fit_line
            && !Self::has_same_result(&result.possible_alignment_patterns, &fit_line_center)
        {
            let (mut ax, mut ay) = (fit_line_center.x(), fit_line_center.y());
            self.fix_alignment_pattern(image, &mut ax, &mut ay, module_size);
            let fixed = AlignmentPattern::new(ax, ay, module_size);
            if !Self::has_same_result(&result.possible_alignment_patterns, &fixed) {
                result.possible_alignment_patterns.push(fixed);
            }
            if !Self::has_same_result(&result.possible_alignment_patterns, &fit_line_center) {
                result.possible_alignment_patterns.push(fit_line_center);
            }
        }

        if !Self::has_same_result(&result.possible_alignment_patterns, &estimate_center) {
            let (mut ax, mut ay) = (estimate_center.x(), estimate_center.y());
            self.fix_alignment_pattern(image, &mut ax, &mut ay, module_size);
            let fixed = AlignmentPattern::new(ax, ay, module_size);
            if !Self::has_same_result(&result.possible_alignment_patterns, &fixed) {
                result.possible_alignment_patterns.push(fixed);
            }
            if !Self::has_same_result(&result.possible_alignment_patterns, &estimate_center) {
                result.possible_alignment_patterns.push(estimate_center);
            }
        }

        // (0,0) means no alignment pattern is used; createTransform then takes
        // the purely affine extrapolation branch
        result
            .possible_alignment_patterns
            .push(AlignmentPattern::new(0.0, 0.0, module_size));
        if !result.possible_alignment_patterns.is_empty() {
            result.confirmed_alignment_pattern = Some(result.possible_alignment_patterns[0]);
        }
        self.state = DetectorState::FindAlignPattern;
        Ok(result)
    }

    fn calculate_module_size_one_way(
        &self,
        image: &BitMatrix,
        pattern: &ResultPoint,
        other_pattern: &ResultPoint,
        pattern_state: PatternState,
        other_pattern_state: PatternState,
    ) -> f32 {
        let est1 = self.size_of_black_white_black_run_both_ways(
            image,
            pattern.x as i32,
            pattern.y as i32,
            other_pattern.x as i32,
            other_pattern.y as i32,
            pattern_state,
            false,
        );
        let est2 = self.size_of_black_white_black_run_both_ways(
            image,
            other_pattern.x as i32,
            other_pattern.y as i32,
            pattern.x as i32,
            pattern.y as i32,
            other_pattern_state,
            true,
        );
        if est1.is_nan() {
            return est2 / 7.0;
        }
        if est2.is_nan() {
            return est1 / 7.0;
        }
        (est1 + est2) / 14.0
    }

    /// Measures the black-white-black span from the finder pattern center in two
    /// opposite directions; on the side that overflows, the other side times 2
    /// is used instead.
    #[allow(clippy::too_many_arguments)]
    fn size_of_black_white_black_run_both_ways(
        &self,
        image: &BitMatrix,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        pattern_state: PatternState,
        is_reverse: bool,
    ) -> f32 {
        let result1 = self.size_of_black_white_black_run(image, from_x, from_y, to_x, to_y);

        let mut scale = 1.0f32;
        let mut other_to_x = from_x - (to_x - from_x);
        if other_to_x < 0 {
            scale = from_x as f32 / (from_x - other_to_x) as f32;
            other_to_x = 0;
        } else if other_to_x >= image.width() as i32 {
            scale = (image.width() as i32 - 1 - from_x) as f32 / (other_to_x - from_x) as f32;
            other_to_x = image.width() as i32 - 1;
        }
        let mut other_to_y = (from_y as f32 - (to_y - from_y) as f32 * scale) as i32;
        scale = 1.0;
        if other_to_y < 0 {
            scale = from_y as f32 / (from_y - other_to_y) as f32;
            other_to_y = 0;
        } else if other_to_y >= image.height() as i32 {
            scale = (image.height() as i32 - 1 - from_y) as f32 / (other_to_y - from_y) as f32;
            other_to_y = image.height() as i32 - 1;
        }
        let other_to_x = (from_x as f32 + (other_to_x - from_x) as f32 * scale) as i32;

        let result2 =
            self.size_of_black_white_black_run(image, from_x, from_y, other_to_x, other_to_y);

        let result = match pattern_state {
            PatternState::LeftOrUpSpill => {
                if !is_reverse {
                    result1 * 2.0
                } else {
                    result2 * 2.0
                }
            }
            PatternState::RightOrDownSpill => {
                if !is_reverse {
                    result2 * 2.0
                } else {
                    result1 * 2.0
                }
            }
            PatternState::Normal => result1 + result2,
        };
        result - 1.0
    }

    /// Walks along a line with Bresenham and measures the lengths of the three
    /// black, white, black runs.
    fn size_of_black_white_black_run(
        &self,
        image: &BitMatrix,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    ) -> f32 {
        let (mut from_x, mut from_y, mut to_x, mut to_y) = (from_x, from_y, to_x, to_y);
        let steep = (to_y - from_y).abs() > (to_x - from_x).abs();
        if steep {
            std::mem::swap(&mut from_x, &mut from_y);
            std::mem::swap(&mut to_x, &mut to_y);
        }
        let dx = (to_x - from_x).abs();
        let dy = (to_y - from_y).abs();
        let mut error = -dx >> 1;
        let xstep = if from_x < to_x { 1 } else { -1 };
        let ystep = if from_y < to_y { 1 } else { -1 };
        let mut state = 0;
        let x_limit = to_x + xstep;

        let mut x = from_x;
        let mut y = from_y;
        while x != x_limit {
            let real_x = if steep { y } else { x };
            let real_y = if steep { x } else { y };
            if real_x < 0
                || real_y < 0
                || real_x >= image.width() as i32
                || real_y >= image.height() as i32
            {
                return f32::NAN;
            }
            let pixel = image.get(real_x as usize, real_y as usize) != 0;
            if !((state == 1) ^ pixel) {
                if state == 2 {
                    return mathutils::distance_4_int(x, y, from_x, from_y);
                }
                state += 1;
            }
            error += dy;
            if error > 0 {
                if y == to_y {
                    break;
                }
                y += ystep;
                error -= dx;
            }
            x += xstep;
        }
        if state == 2 {
            return mathutils::distance_4_int(to_x + xstep, to_y, from_x, from_y);
        }
        f32::NAN
    }

    fn find_alignment_in_region(
        &self,
        image: &BitMatrix,
        overall_est_module_size: f32,
        est_alignment_x: i32,
        est_alignment_y: i32,
        allowance_factor: f32,
    ) -> ZXResult<AlignmentPattern> {
        let allowance = (allowance_factor * overall_est_module_size) as i32;
        let left_x = 0.max(est_alignment_x - allowance);
        let right_x = (image.width() as i32 - 1).min(est_alignment_x + allowance);
        if ((right_x - left_x) as f32) < overall_est_module_size * 3.0 {
            return Err(reader("region too small to hold alignment pattern"));
        }
        let top_y = 0.max(est_alignment_y - allowance);
        let bottom_y = (image.height() as i32 - 1).min(est_alignment_y + allowance);
        if ((bottom_y - top_y) as f32) < overall_est_module_size * 3.0 {
            return Err(reader("region too small to hold alignment pattern"));
        }
        let mut finder = AlignmentPatternFinder::new(
            left_x,
            top_y,
            right_x - left_x,
            bottom_y - top_y,
            overall_est_module_size,
        );
        finder.find(image)
    }

    /// Fits the top-right and bottom-left outer edge lines; their intersection
    /// is roughly the center of the alignment pattern.
    fn find_alignment_with_fit_line(
        &self,
        image: &BitMatrix,
        top_left: &ResultPoint,
        top_right: &ResultPoint,
        bottom_left: &ResultPoint,
        module_size: f32,
    ) -> ZXResult<AlignmentPattern> {
        let img_width = image.width() as i32;
        let img_height = image.height() as i32;
        let rect_size = module_size * 7.0;

        let mk_rect = |p: &ResultPoint| {
            let mut r = Rect::default();
            r.x = if p.x - rect_size / 2.0 > 0.0 {
                (p.x - rect_size / 2.0) as i32
            } else {
                0
            };
            r.y = if p.y - rect_size / 2.0 > 0.0 {
                (p.y - rect_size / 2.0) as i32
            } else {
                0
            };
            r.width = (p.x as i32 - r.x) * 2;
            if r.x + r.width > img_width {
                r.width = img_width - r.x;
            }
            r.height = (p.y as i32 - r.y) * 2;
            if r.y + r.height > img_height {
                r.height = img_height - r.y;
            }
            r
        };
        let bottom_left_rect = mk_rect(bottom_left);
        let top_right_rect = mk_rect(top_right);

        let (top_right_points, bottom_left_points) = self.find_points_for_line(
            image,
            top_left,
            top_right,
            bottom_left,
            top_right_rect,
            bottom_left_rect,
            module_size,
        );

        let (k1, b1, a1) = fit_line(&top_right_points)
            .ok_or_else(|| reader("Cannot find a valid divide for line fit"))?;
        let (k2, b2, a2) = fit_line(&bottom_left_points)
            .ok_or_else(|| reader("Cannot find a valid divide for line fit"))?;

        let (alignment_x, alignment_y);
        if a1 == 0 {
            if a2 == 0 {
                return Err(reader("Cannot find a valid divide for line fit"));
            }
            alignment_x = -b1;
            alignment_y = b2 - b1 * k2;
        } else if a2 == 0 {
            alignment_x = -b2;
            alignment_y = b1 - b2 * k1;
        } else if k1 == k2 {
            return Err(reader("Cannot find a valid divide for line fit"));
        } else {
            alignment_x = (b2 - b1) / (k1 - k2);
            alignment_y = k1 * alignment_x + b1;
        }
        Ok(AlignmentPattern::new(alignment_x, alignment_y, module_size))
    }

    /// If the predicted point lands in a white area, searches outwards ring by
    /// ring for the nearest black point.
    fn fix_alignment_pattern(
        &self,
        image: &BitMatrix,
        alignment_x: &mut f32,
        alignment_y: &mut f32,
        module_size: f32,
    ) {
        let img_width = image.width() as i32;
        let img_height = image.height() as i32;
        let max_fix_step = (module_size * 2.0) as i32;
        let mut fix_step = 0;

        let get = |x: i32, y: i32| -> bool {
            x > 0
                && y > 0
                && x < img_width
                && y < img_height
                && image.get(x as usize, y as usize) != 0
        };

        while (*alignment_x) < img_width as f32
            && (*alignment_y) < img_height as f32
            && *alignment_x > 0.0
            && *alignment_y > 0.0
            && !get(*alignment_x as i32, *alignment_y as i32)
            && fix_step < max_fix_step
        {
            fix_step += 1;
            let ax = *alignment_x as i32;
            let ay = *alignment_y as i32;
            for y in ay - fix_step..=ay + fix_step {
                if y == ay - fix_step || y == ay + fix_step {
                    for x in ax - fix_step..=ax + fix_step {
                        if get(x, y) {
                            *alignment_x = x as f32;
                            *alignment_y = y as f32;
                            return;
                        }
                    }
                } else {
                    for x in [ax - fix_step, ax + fix_step] {
                        if get(x, y) {
                            *alignment_x = x as f32;
                            *alignment_y = y as f32;
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Samples a series of points along the edges inside the rectangles around
    /// the top-right and bottom-left finder patterns, for line fitting.
    #[allow(clippy::too_many_arguments)]
    fn find_points_for_line(
        &self,
        image: &BitMatrix,
        top_left: &ResultPoint,
        top_right: &ResultPoint,
        bottom_left: &ResultPoint,
        top_right_rect: Rect,
        bottom_left_rect: Rect,
        model_size: f32,
    ) -> (Vec<ResultPoint>, Vec<ResultPoint>) {
        let img_width = image.width() as i32;
        let img_height = image.height() as i32;
        let top_left_x = top_left.x as i32;
        let top_left_y = top_left.y as i32;
        let top_right_x = top_right.x as i32;
        let top_right_y = top_right.y as i32;
        let ms = model_size as i32;

        let mut top_right_points: Vec<ResultPoint> = Vec::new();
        let mut bottom_left_points: Vec<ResultPoint> = Vec::new();

        let get = |x: i32, y: i32| -> bool {
            x >= 0
                && y >= 0
                && x < img_width
                && y < img_height
                && image.get(x as usize, y as usize) != 0
        };

        // Sweep a region, taking per row (or per column) the first point with a
        // black-white transition
        let scan_rows = |x_min: i32,
                         x_max: i32,
                         y_min: i32,
                         y_max: i32,
                         cond: &dyn Fn(i32, i32) -> bool,
                         flag: i32,
                         rect: Rect,
                         other: &ResultPoint,
                         out: &mut Vec<ResultPoint>| {
            let (mut x_min, mut x_max) = (x_min, x_max);
            let (mut y_min, mut y_max) = (y_min, y_max);
            mathutils::get_range_values(&mut x_min, &mut x_max, 0, img_width - 1);
            mathutils::get_range_values(&mut y_min, &mut y_max, 0, img_height - 1);
            for i in y_min..y_max {
                for j in x_min..x_max {
                    if cond(j, i) {
                        let p = ResultPoint::new(j as f32, i as f32);
                        if check_tolerance(top_left, other, &rect, model_size as f64, &p, flag) {
                            out.push(p);
                            break;
                        }
                    }
                }
            }
        };
        let scan_cols = |x_min: i32,
                         x_max: i32,
                         y_min: i32,
                         y_max: i32,
                         cond: &dyn Fn(i32, i32) -> bool,
                         flag: i32,
                         rect: Rect,
                         other: &ResultPoint,
                         out: &mut Vec<ResultPoint>| {
            let (mut x_min, mut x_max) = (x_min, x_max);
            let (mut y_min, mut y_max) = (y_min, y_max);
            mathutils::get_range_values(&mut x_min, &mut x_max, 0, img_width - 1);
            mathutils::get_range_values(&mut y_min, &mut y_max, 0, img_height - 1);
            for j in x_min..x_max {
                for i in y_min..y_max {
                    if cond(j, i) {
                        let p = ResultPoint::new(j as f32, i as f32);
                        if check_tolerance(top_left, other, &rect, model_size as f64, &p, flag) {
                            out.push(p);
                            break;
                        }
                    }
                }
            }
        };

        let black_then_white_right = |j: i32, i: i32| get(j, i) && !get(j + 1, i);
        let white_then_black_right = |j: i32, i: i32| !get(j, i) && get(j + 1, i);
        let black_then_white_down = |j: i32, i: i32| get(j, i) && !get(j, i + 1);
        let white_then_black_down = |j: i32, i: i32| !get(j, i) && get(j, i + 1);

        // The branch conditions match the original: first decide whether the code
        // is laid out horizontally or vertically, then decide the left/right or
        // top/bottom orientation
        let horizontal = top_left_y == top_right_y
            || (top_right_y != top_left_y
                && ((top_right_x - top_left_x) / (top_right_y - top_left_y)).abs() >= 1);

        if horizontal {
            if top_left_x < top_right_x {
                scan_rows(
                    top_right_rect.x,
                    top_right_rect.x + ms * 2,
                    top_right_rect.y + ms,
                    top_right_rect.y - ms + top_right_rect.height,
                    &black_then_white_right,
                    0,
                    top_right_rect,
                    top_right,
                    &mut top_right_points,
                );
                scan_cols(
                    bottom_left_rect.x + ms,
                    bottom_left_rect.x - ms + bottom_left_rect.width,
                    bottom_left_rect.y,
                    bottom_left_rect.y + 2 * ms,
                    &black_then_white_down,
                    2,
                    bottom_left_rect,
                    bottom_left,
                    &mut bottom_left_points,
                );
            } else {
                scan_rows(
                    top_right_rect.x + top_right_rect.width - 2 * ms,
                    top_right_rect.x + top_right_rect.width,
                    top_right_rect.y + ms,
                    top_right_rect.y - ms + top_right_rect.height,
                    &white_then_black_right,
                    1,
                    top_right_rect,
                    top_right,
                    &mut top_right_points,
                );
                scan_cols(
                    bottom_left_rect.x + ms,
                    bottom_left_rect.x - ms + bottom_left_rect.width,
                    bottom_left_rect.y + bottom_left_rect.height - 2 * ms,
                    bottom_left_rect.y + bottom_left_rect.height,
                    &white_then_black_down,
                    3,
                    bottom_left_rect,
                    bottom_left,
                    &mut bottom_left_points,
                );
            }
        } else if top_left_y < top_right_y {
            scan_cols(
                top_right_rect.x + ms,
                top_right_rect.x - ms + top_right_rect.width,
                top_right_rect.y,
                top_right_rect.y + 2 * ms,
                &black_then_white_down,
                2,
                top_right_rect,
                top_right,
                &mut top_right_points,
            );
            scan_rows(
                top_right_rect.x + top_right_rect.width - 2 * ms,
                top_right_rect.x + top_right_rect.width,
                top_right_rect.y + ms,
                top_right_rect.y - ms + top_right_rect.height,
                &white_then_black_right,
                1,
                bottom_left_rect,
                bottom_left,
                &mut bottom_left_points,
            );
        } else {
            scan_cols(
                top_right_rect.x + ms,
                top_right_rect.x - ms + top_right_rect.width,
                top_right_rect.y + top_right_rect.height - 2 * ms,
                top_right_rect.y + top_right_rect.height,
                &white_then_black_down,
                3,
                top_right_rect,
                top_right,
                &mut top_right_points,
            );
            scan_rows(
                bottom_left_rect.x,
                bottom_left_rect.x + 2 * ms,
                bottom_left_rect.y + ms,
                bottom_left_rect.y + bottom_left_rect.height - ms,
                &black_then_white_right,
                0,
                bottom_left_rect,
                bottom_left,
                &mut bottom_left_points,
            );
        }

        (top_right_points, bottom_left_points)
    }
}

/// Port of `Detector::fitLine`: first discards outliers by the spacing between
/// adjacent points, then applies least squares. Returns (k, b, a); a == 0 means
/// the fitted line is the vertical line x = -b.
fn fit_line(old_points: &[ResultPoint]) -> Option<(f32, f32, i32)> {
    if old_points.len() < 2 {
        return None;
    }
    let tolerance = 2.0f32;
    let mut fit_points: Vec<ResultPoint> = Vec::new();
    let mut pre_diff = -1.0f32;
    if old_points.len() >= 3 {
        for idx in 1..old_points.len() - 1 {
            if pre_diff < 0.0 {
                let dx = old_points[idx - 1].x - old_points[idx].x;
                let dy = old_points[idx - 1].y - old_points[idx].y;
                pre_diff = dx * dx + dy * dy;
            }
            let dx = old_points[idx + 1].x - old_points[idx].x;
            let dy = old_points[idx + 1].y - old_points[idx].y;
            let diff = dx * dx + dy * dy;
            if pre_diff <= tolerance && diff <= tolerance {
                fit_points.push(old_points[idx]);
            }
            pre_diff = diff;
        }
    }
    let num = fit_points.len();
    if num < 2 {
        return None;
    }
    let (mut x, mut y, mut xx, mut xy) = (0f64, 0f64, 0f64, 0f64);
    for p in fit_points.iter() {
        // The original truncates the coordinates to int before accumulating
        let px = p.x as i32 as f64;
        let py = p.y as i32 as f64;
        x += px;
        y += py;
        xx += px * px;
        xy += px * py;
    }
    let n = num as f64;
    let tem = xx * n - x * x;
    if tem.abs() < 0.0000001 {
        return Some((1.0, (-x / n) as f32, 0));
    }
    let mut k = ((n * xy - x * y) / tem) as f32;
    let b = ((y - k as f64 * x) / n) as f32;
    if k.abs() < 0.01 {
        k = 0.0;
    }
    Some((k, b, 1))
}

/// Port of `Detector::checkTolerance`: whether the point lies inside the band of
/// +-2.5 modules around the line through the two finder patterns.
fn check_tolerance(
    top_left: &ResultPoint,
    top_right: &ResultPoint,
    rect: &Rect,
    model_size: f64,
    p: &ResultPoint,
    flag: i32,
) -> bool {
    let top_left_x = top_left.x as i32;
    let top_left_y = top_left.y as i32;
    let top_right_x = top_right.x as i32;
    let top_right_y = top_right.y as i32;

    if flag < 2 {
        if top_right_x == top_left_x {
            return false;
        }
        let tolerance_x = if flag == 1 {
            (rect.x + rect.width) as f64
        } else {
            rect.x as f64
        };
        let k = (top_right_y - top_left_y) as f64 / (top_right_x - top_left_x) as f64;
        let b = top_right_y as f64 - k * top_right_x as f64;
        let tmp_1 = model_size * 2.5;
        let tmp_2 = tmp_1 * k;
        let b_tolerance = (tmp_1 * tmp_1 + tmp_2 * tmp_2).sqrt();
        let y1 = k * tolerance_x + (b - b_tolerance);
        let y2 = k * tolerance_x + (b + b_tolerance);
        !((p.y as f64) < y1 || (p.y as f64) > y2)
    } else {
        if top_right_y == top_left_y {
            return false;
        }
        let tolerance_y = if flag == 3 {
            (rect.y + rect.height) as f64
        } else {
            rect.y as f64
        };
        let k = (top_right_x - top_left_x) as f64 / (top_right_y - top_left_y) as f64;
        let b = top_right_x as f64 - k * top_right_y as f64;
        let tmp_1 = model_size * 2.5;
        let tmp_2 = tmp_1 / k;
        let b_tolerance = (tmp_1 * tmp_1 + tmp_2 * tmp_2).sqrt();
        let x1 = k * tolerance_y + (b - b_tolerance);
        let x2 = k * tolerance_y + (b + b_tolerance);
        !((p.x as f64) < x1 || (p.x as f64) > x2)
    }
}

/// Port of `Detector::createTransform`.
pub fn create_transform(
    top_left: &ResultPoint,
    top_right: &ResultPoint,
    bottom_left: &ResultPoint,
    alignment_pattern: Option<&ResultPoint>,
    dimension: i32,
) -> PerspectiveTransform {
    let dim_minus_three = dimension as f32 - 3.5;
    let (bottom_right_x, bottom_right_y, source_bottom_right_x, source_bottom_right_y);

    // Note that the original's test is `alignmentPattern && alignmentPattern->getX()`:
    // an alignment pattern whose x is exactly 0 (the (0,0) fallback entry) takes the
    // affine extrapolation branch
    match alignment_pattern.filter(|ap| ap.x != 0.0) {
        Some(ap) => {
            bottom_right_x = ap.x;
            bottom_right_y = ap.y;
            source_bottom_right_x = dim_minus_three - 3.0;
            source_bottom_right_y = source_bottom_right_x;
        }
        None => {
            let mut brx = (top_right.x - top_left.x) + bottom_left.x;
            let mut bry = (top_right.y - top_left.y) + bottom_left.y;
            let mut delta_x = top_left.x - bottom_left.x;
            let mut delta_y = top_left.y - bottom_left.y;
            if delta_x.abs() < delta_y.abs() {
                delta_y = top_left.y - top_right.y;
            } else {
                delta_x = top_left.x - top_right.x;
            }
            brx += 2.0 * delta_x;
            bry += 2.0 * delta_y;
            bottom_right_x = brx;
            bottom_right_y = bry;
            source_bottom_right_x = dim_minus_three;
            source_bottom_right_y = dim_minus_three;
        }
    }

    PerspectiveTransform::quadrilateral_to_quadrilateral(
        3.5,
        3.5,
        dim_minus_three,
        3.5,
        source_bottom_right_x,
        source_bottom_right_y,
        3.5,
        dim_minus_three,
        top_left.x,
        top_left.y,
        top_right.x,
        top_right.y,
        bottom_right_x,
        bottom_right_y,
        bottom_left.x,
        bottom_left.y,
    )
}

/// Port of `Detector::computeDimension`: estimates the dimension and rounds it
/// to 4k+1.
pub fn compute_dimension(
    top_left: &ResultPoint,
    top_right: &ResultPoint,
    bottom_left: &ResultPoint,
    module_size_x: f32,
    module_size_y: f32,
) -> i32 {
    let tltr = (ResultPoint::distance(top_left, top_right) / module_size_x) as i32;
    let tlbl = (ResultPoint::distance(top_left, bottom_left) / module_size_y) as i32;
    let tmp_dimension = ((tltr + tlbl) as f32 / 2.0) + 7.0;
    let mut dimension = cv_round(tmp_dimension as f64);
    match dimension & 0x03 {
        0 => dimension += 1,
        2 => dimension -= 1,
        _ => {}
    }
    dimension
}

/// Port of `Detector::checkConvexQuadrilateral`: whether the four points form a
/// convex quadrilateral.
pub fn check_convex_quadrilateral(
    top_left: &ResultPoint,
    top_right: &ResultPoint,
    bottom_left: &ResultPoint,
    bottom_right: &ResultPoint,
) -> bool {
    let v1 = [top_left.x - top_right.x, top_left.y - top_right.y];
    let v2 = [top_right.x - bottom_right.x, top_right.y - bottom_right.y];
    let v3 = [
        bottom_right.x - bottom_left.x,
        bottom_right.y - bottom_left.y,
    ];
    let v4 = [bottom_left.x - top_left.x, bottom_left.y - top_left.y];
    let c1 = mathutils::vec_cross(v1, v2);
    let c2 = mathutils::vec_cross(v2, v3);
    let c3 = mathutils::vec_cross(v3, v4);
    let c4 = mathutils::vec_cross(v4, v1);
    (c1 < 0.0 && c2 < 0.0 && c3 < 0.0 && c4 < 0.0) || (c1 > 0.0 && c2 > 0.0 && c3 > 0.0 && c4 > 0.0)
}
