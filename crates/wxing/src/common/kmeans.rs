//! Port of `zxing/common/kmeans.{hpp,cpp}`.
//!
//! The only use is in FinderPatternFinder, clustering candidate finder patterns
//! by (count, moduleSize); the distance function deliberately weights the count
//! dimension 0 and the moduleSize dimension 1.

pub struct Cluster {
    pub centroid: Vec<f64>,
    pub samples: Vec<usize>,
}

/// As in the original: dimension 0 (count) has weight 0, dimension 1
/// (moduleSize) has weight 1, and any further dimension has weight 1.
pub fn cal_distance(a: &[f64], b: &[f64]) -> f64 {
    const KMEANS_COUNT_FACTOR: f64 = 0.0;
    const KMEANS_MS_FACTOR: f64 = 1.0;
    let mut val = 0.0f64;
    for i in 0..a.len() {
        let d = a[i] - b[i];
        val += match i {
            0 => KMEANS_COUNT_FACTOR * d * d,
            1 => KMEANS_MS_FACTOR * d * d,
            _ => d * d,
        };
    }
    val.powf(0.5)
}

pub fn k_means(
    train_x: &[Vec<f64>],
    k: usize,
    maxepoches: usize,
    minchanged: usize,
) -> Vec<Cluster> {
    let row_num = train_x.len();
    if row_num == 0 || k == 0 {
        return Vec::new();
    }
    let col_num = train_x[0].len();

    let step = row_num / k;
    let mut clusters: Vec<Cluster> = (0..k)
        .map(|i| Cluster {
            centroid: train_x[(i * step).min(row_num - 1)].clone(),
            samples: Vec::new(),
        })
        .collect();

    for _ in 0..maxepoches {
        for c in clusters.iter_mut() {
            c.samples.clear();
        }
        for (j, sample) in train_x.iter().enumerate() {
            let mut c = 0usize;
            let mut min_distance = cal_distance(sample, &clusters[0].centroid);
            for i in 1..k {
                let distance = cal_distance(sample, &clusters[i].centroid);
                if distance < min_distance {
                    min_distance = distance;
                    c = i;
                }
            }
            clusters[c].samples.push(j);
        }

        let mut changed = 0usize;
        for cluster in clusters.iter_mut() {
            let n = cluster.samples.len();
            if n == 0 {
                continue;
            }
            let mut val = vec![0.0f64; col_num];
            for &s in cluster.samples.iter() {
                for d in 0..col_num {
                    val[d] += train_x[s][d];
                }
            }
            for d in 0..col_num {
                let value = val[d] / n as f64;
                if cluster.centroid[d] != value {
                    cluster.centroid[d] = value;
                    changed += 1;
                }
            }
        }
        if changed <= minchanged {
            return clusters;
        }
    }
    clusters
}
