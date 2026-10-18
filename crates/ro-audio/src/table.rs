use serde::{Deserialize, Serialize};

/// Piecewise-linear lookup over `[x, y]` points sorted by x. Linear segments never overshoot, so a
/// monotonic table stays monotonic between its points.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Table(pub Vec<[f64; 2]>);

impl Table {
    pub fn validate(&self, name: &str) -> Result<(), String> {
        if self.0.is_empty() {
            return Err(format!("{name}: empty table"));
        }
        if self.0.windows(2).any(|w| w[1][0] <= w[0][0]) {
            return Err(format!("{name}: x values must increase strictly"));
        }
        Ok(())
    }

    /// x for `y` on a table whose y never decreases; clamped to the table's ends.
    pub fn invert_increasing(&self, y: f64) -> f64 {
        let points = &self.0;
        if y <= points[0][1] {
            return points[0][0];
        }
        for w in points.windows(2) {
            let ([x0, y0], [x1, y1]) = (w[0], w[1]);
            if y <= y1 {
                return if y1 > y0 { x0 + (y - y0) / (y1 - y0) * (x1 - x0) } else { x0 };
            }
        }
        points[points.len() - 1][0]
    }

    /// Every x at which the table passes through `y`. A non-monotonic table can give several; when
    /// `y` is outside the table's range, the x of the nearest point stands in.
    pub fn solutions(&self, y: f64) -> Vec<f64> {
        let points = &self.0;
        let mut found: Vec<f64> = Vec::new();
        for w in points.windows(2) {
            let ([x0, y0], [x1, y1]) = (w[0], w[1]);
            let (lo, hi) = (y0.min(y1), y0.max(y1));
            if y >= lo && y <= hi {
                let x = if y1 == y0 { x0 } else { x0 + (y - y0) / (y1 - y0) * (x1 - x0) };
                if found.last().is_none_or(|&last| (last - x).abs() > 1e-9) {
                    found.push(x);
                }
            }
        }
        if points.len() == 1 || found.is_empty() {
            let nearest = points.iter().min_by(|a, b| (a[1] - y).abs().total_cmp(&(b[1] - y).abs())).expect("validated non-empty");
            found.push(nearest[0]);
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverts_and_clamps_an_increasing_table() {
        let t = Table(vec![[0.0, 0.0], [0.5, 10.0], [1.0, 40.0]]);
        assert_eq!(t.invert_increasing(5.0), 0.25);
        assert_eq!(t.invert_increasing(25.0), 0.75);
        assert_eq!(t.invert_increasing(99.0), 1.0);
        assert_eq!(t.invert_increasing(-1.0), 0.0);
    }

    #[test]
    fn a_dip_gives_every_crossing() {
        let t = Table(vec![[130.0, -42.0], [150.0, -40.0], [200.0, -42.0], [300.0, -45.0]]);
        assert_eq!(t.solutions(-41.0), vec![140.0, 175.0]);
        assert_eq!(t.solutions(-44.0).len(), 1);
        assert_eq!(t.solutions(-30.0), vec![150.0]);
    }
}
