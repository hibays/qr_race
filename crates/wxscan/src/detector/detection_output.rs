//! The caffe-SSD `DetectionOutput` layer: prior box decoding plus per-class NMS.
//!
//! The parameters come from `detect.prototxt`: num_classes=2, share_location,
//! background_label_id=0, code_type=CENTER_SIZE, confidence threshold 0.2, NMS
//! threshold 0.45, top_k=100, keep_top_k=100.

use super::priorbox::Prior;

#[derive(Clone, Copy, Debug)]
pub struct DetectionOutputParams {
    pub num_classes: usize,
    pub background_label_id: i32,
    pub confidence_threshold: f32,
    pub nms_threshold: f32,
    pub top_k: usize,
    pub keep_top_k: usize,
}

impl Default for DetectionOutputParams {
    fn default() -> Self {
        Self {
            num_classes: 2,
            background_label_id: 0,
            confidence_threshold: 0.2,
            nms_threshold: 0.45,
            top_k: 100,
            keep_top_k: 100,
        }
    }
}

/// One detection; the field order matches the 7-tuple caffe emits.
#[derive(Clone, Copy, Debug)]
pub struct Detection {
    pub image_id: f32,
    pub label: f32,
    pub score: f32,
    pub xmin: f32,
    pub ymin: f32,
    pub xmax: f32,
    pub ymax: f32,
}

/// CENTER_SIZE decoding. The variance is not encoded into the target, so it is
/// applied here as a multiplier.
fn decode_bbox(prior: &Prior, loc: &[f32]) -> [f32; 4] {
    let p = &prior.bbox;
    let prior_width = p[2] - p[0];
    let prior_height = p[3] - p[1];
    let prior_center_x = (p[0] + p[2]) / 2.0;
    let prior_center_y = (p[1] + p[3]) / 2.0;

    let v = &prior.variance;
    let cx = v[0] * loc[0] * prior_width + prior_center_x;
    let cy = v[1] * loc[1] * prior_height + prior_center_y;
    let w = (v[2] * loc[2]).exp() * prior_width;
    let h = (v[3] * loc[3]).exp() * prior_height;

    [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0]
}

fn jaccard_overlap(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix1 = a[0].max(b[0]);
    let iy1 = a[1].max(b[1]);
    let ix2 = a[2].min(b[2]);
    let iy2 = a[3].min(b[3]);
    if ix2 <= ix1 || iy2 <= iy1 {
        return 0.0;
    }
    let inter = (ix2 - ix1) * (iy2 - iy1);
    let area_a = (a[2] - a[0]) * (a[3] - a[1]);
    let area_b = (b[2] - b[0]) * (b[3] - b[1]);
    inter / (area_a + area_b - inter)
}

/// Port of caffe's `ApplyNMSFast`: greedy suppression in descending score order.
fn apply_nms_fast(
    bboxes: &[[f32; 4]],
    scores: &[(f32, usize)],
    nms_threshold: f32,
    top_k: usize,
) -> Vec<usize> {
    let mut sorted: Vec<(f32, usize)> = scores.to_vec();
    // Descending by score; ties broken by ascending index, matching caffe's
    // stable sort
    sorted.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    sorted.truncate(top_k);

    let mut keep: Vec<usize> = Vec::new();
    for &(_, idx) in sorted.iter() {
        let mut suppressed = false;
        for &k in keep.iter() {
            if jaccard_overlap(&bboxes[idx], &bboxes[k]) > nms_threshold {
                suppressed = true;
                break;
            }
        }
        if !suppressed {
            keep.push(idx);
        }
    }
    keep
}

/// `loc`: num_priors*4. `conf`: num_priors*num_classes, already softmaxed.
pub fn forward(
    loc: &[f32],
    conf: &[f32],
    priors: &[Prior],
    params: &DetectionOutputParams,
) -> Vec<Detection> {
    let num_priors = priors.len();
    debug_assert_eq!(loc.len(), num_priors * 4);
    debug_assert_eq!(conf.len(), num_priors * params.num_classes);

    // 1) Decode all prior boxes (share_location: one box set for all classes)
    let bboxes: Vec<[f32; 4]> = (0..num_priors)
        .map(|i| decode_bbox(&priors[i], &loc[i * 4..i * 4 + 4]))
        .collect();

    // 2) Run NMS per class
    let mut all: Vec<Detection> = Vec::new();
    for c in 0..params.num_classes {
        if c as i32 == params.background_label_id {
            continue;
        }
        let scores: Vec<(f32, usize)> = (0..num_priors)
            .map(|i| (conf[i * params.num_classes + c], i))
            .filter(|(s, _)| *s > params.confidence_threshold)
            .collect();
        if scores.is_empty() {
            continue;
        }
        let keep = apply_nms_fast(&bboxes, &scores, params.nms_threshold, params.top_k);
        for idx in keep {
            let b = bboxes[idx];
            all.push(Detection {
                image_id: 0.0,
                label: c as f32,
                score: conf[idx * params.num_classes + c],
                xmin: b[0],
                ymin: b[1],
                xmax: b[2],
                ymax: b[3],
            });
        }
    }

    // 3) Keep the keep_top_k highest scoring detections across all classes
    if all.len() > params.keep_top_k {
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(params.keep_top_k);
    }
    all
}
