//! Exact squared Euclidean distances with reusable separable-transform storage.

const FAR: f32 = 1.0e12;

#[derive(Default)]
pub(crate) struct DistanceField {
    pub(crate) squared: Vec<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    sites: Vec<usize>,
    cuts: Vec<f32>,
}

impl DistanceField {
    pub(crate) fn compute(&mut self, width: usize, height: usize, feature: impl Fn(usize, usize) -> bool) {
        let n = width.max(height);
        self.squared.resize(width * height, FAR);
        self.input.resize(n, FAR);
        self.output.resize(n, FAR);
        self.sites.resize(n, 0);
        self.cuts.resize(n + 1, 0.0);
        if width == 0 || height == 0 { return; }
        for y in 0..height {
            fill_features(&mut self.input[..width], y, &feature);
            transform(&self.input[..width], &mut self.output, &mut self.sites, &mut self.cuts);
            self.squared[y * width..(y + 1) * width].copy_from_slice(&self.output[..width]);
        }
        for x in 0..width {
            copy_column(&mut self.input, &self.squared, x, width, height);
            transform(&self.input[..height], &mut self.output, &mut self.sites, &mut self.cuts);
            write_column(&mut self.squared, &self.output, x, width, height);
        }
    }
}

fn intersection(f: &[f32], p: usize, q: usize) -> f32 {
    ((f[q] - f[p]) + (q as f32 * q as f32 - p as f32 * p as f32))
        / (2.0 * (q as f32 - p as f32))
}

fn transform(f: &[f32], out: &mut [f32], sites: &mut [usize], cuts: &mut [f32]) {
    let mut k = 0;
    sites[0] = 0;
    cuts[0] = f32::NEG_INFINITY;
    cuts[1] = f32::INFINITY;
    for q in 1..f.len() {
        let mut cut = intersection(f, sites[k], q);
        while k > 0 && cut <= cuts[k] {
            k -= 1;
            cut = intersection(f, sites[k], q);
        }
        k += 1;
        sites[k] = q;
        cuts[k] = cut;
        cuts[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, value) in out.iter_mut().take(f.len()).enumerate() {
        while cuts[k + 1] < q as f32 { k += 1; }
        let delta = q as f32 - sites[k] as f32;
        *value = delta * delta + f[sites[k]];
    }
}


#[cfg(test)]
fn assert_distances(features: &[(usize, usize)], squared: &[f32]) {
    for (index, &actual) in squared.iter().enumerate() {
        let x = index % 9;
        let y = index / 9;
        let expected = features.iter().map(|&(fx, fy)| {
            let dx = x as f32 - fx as f32;
            let dy = y as f32 - fy as f32;
            dx * dx + dy * dy
        }).fold(FAR, f32::min);
        assert_eq!(actual, expected);
    }
}

fn copy_column(out: &mut [f32], grid: &[f32], x: usize, width: usize, height: usize) {
    for y in 0..height { out[y] = grid[y * width + x]; }
}

fn write_column(grid: &mut [f32], input: &[f32], x: usize, width: usize, height: usize) {
    for y in 0..height { grid[y * width + x] = input[y]; }
}

fn fill_features(row: &mut [f32], y: usize, feature: &impl Fn(usize, usize) -> bool) {
    for (x, value) in row.iter_mut().enumerate() {
        *value = if feature(x, y) { 0.0 } else { FAR };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_match_nearest_feature_including_empty_fields_and_reuse() {
        let mut field = DistanceField::default();
        for seed in 0..32 {
            let features: Vec<_> = (0..63).filter(|index| (index * 17 + seed) % 23 < seed % 5)
                .map(|index| (index % 9, index / 9)).collect();
            field.compute(9, 7, |x, y| features.contains(&(x, y)));
            assert_distances(&features, &field.squared);

        }
    }
}
