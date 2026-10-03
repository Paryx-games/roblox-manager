use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct Stats {
    pub n: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub variance: f64,
    pub stdev: f64,
    pub sem: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub cv_pct: f64,
    pub mad: f64,
    pub iqr: f64,
    pub trimmed_mean_5pct: f64,
    pub rms: f64,
    pub skewness: f64,
    pub excess_kurtosis: f64,
    pub p01: f64,
    pub p05: f64,
    pub p10: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

fn sorted(values: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    v
}

pub fn percentile(values: &[f64], q: f64) -> f64 {
    let v = sorted(values);
    if v.is_empty() {
        return f64::NAN;
    }
    if v.len() == 1 {
        return v[0];
    }
    let pos = (v.len() - 1) as f64 * q.clamp(0.0, 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return v[lo];
    }
    let frac = pos - lo as f64;
    v[lo] * (1.0 - frac) + v[hi] * frac
}

fn t95(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    if df == 0 {
        0.0
    } else if df <= TABLE.len() {
        TABLE[df - 1]
    } else {
        // Cornish-Fisher expansion is accurate beyond the tabulated small samples.
        let z: f64 = 1.959963984540054;
        let n = df as f64;
        z + (z.powi(3) + z) / (4.0 * n)
            + (5.0 * z.powi(5) + 16.0 * z.powi(3) + 3.0 * z) / (96.0 * n * n)
            + (3.0 * z.powi(7) + 19.0 * z.powi(5) + 17.0 * z.powi(3) - 15.0 * z)
                / (384.0 * n.powi(3))
    }
}

pub fn describe(values: &[f64]) -> Option<Stats> {
    let v = sorted(values);
    let n = v.len();
    if n == 0 {
        return None;
    }

    let mean = v.iter().sum::<f64>() / n as f64;
    let median = percentile(&v, 0.5);
    let variance = if n > 1 {
        v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    let stdev = variance.sqrt();
    let sem = if n > 0 {
        stdev / (n as f64).sqrt()
    } else {
        f64::NAN
    };
    let half = if n >= 2 { t95(n - 1) * sem } else { f64::NAN };
    let mad_vals: Vec<f64> = v.iter().map(|x| (x - median).abs()).collect();

    let trim = ((n as f64) * 0.05).floor() as usize;
    let trimmed = if n > trim * 2 {
        &v[trim..n - trim]
    } else {
        &v[..]
    };
    let trimmed_mean = trimmed.iter().sum::<f64>() / trimmed.len() as f64;
    let rms = (v.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();

    let m2 = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let skewness = if m2 > 0.0 && n >= 3 {
        let m3 = v.iter().map(|x| (x - mean).powi(3)).sum::<f64>() / n as f64;
        m3 / m2.powf(1.5)
    } else {
        0.0
    };
    let excess_kurtosis = if m2 > 0.0 && n >= 4 {
        let m4 = v.iter().map(|x| (x - mean).powi(4)).sum::<f64>() / n as f64;
        m4 / (m2 * m2) - 3.0
    } else {
        0.0
    };

    Some(Stats {
        n,
        min: v[0],
        max: v[n - 1],
        mean,
        median,
        variance,
        stdev,
        sem,
        ci95_low: mean - half,
        ci95_high: mean + half,
        cv_pct: if mean != 0.0 {
            stdev / mean.abs() * 100.0
        } else {
            f64::NAN
        },
        mad: percentile(&mad_vals, 0.5),
        iqr: percentile(&v, 0.75) - percentile(&v, 0.25),
        trimmed_mean_5pct: trimmed_mean,
        rms,
        skewness,
        excess_kurtosis,
        p01: percentile(&v, 0.01),
        p05: percentile(&v, 0.05),
        p10: percentile(&v, 0.10),
        p25: percentile(&v, 0.25),
        p50: percentile(&v, 0.50),
        p75: percentile(&v, 0.75),
        p90: percentile(&v, 0.90),
        p95: percentile(&v, 0.95),
        p99: percentile(&v, 0.99),
    })
}

pub fn regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64)> {
    if xs.len() < 2 || xs.len() != ys.len() || xs.iter().chain(ys).any(|v| !v.is_finite()) {
        return None;
    }
    let mx = xs.iter().sum::<f64>() / xs.len() as f64;
    let my = ys.iter().sum::<f64>() / ys.len() as f64;
    let sxx = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>();
    if sxx == 0.0 {
        return None;
    }
    let sxy = xs
        .iter()
        .zip(ys)
        .map(|(x, y)| (x - mx) * (y - my))
        .sum::<f64>();
    let slope = sxy / sxx;
    let ss_tot = ys.iter().map(|y| (y - my).powi(2)).sum::<f64>();
    let ss_res = xs
        .iter()
        .zip(ys)
        .map(|(x, y)| {
            let predicted = my + slope * (x - mx);
            (y - predicted).powi(2)
        })
        .sum::<f64>();
    let r2 = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        1.0
    };
    Some((slope, r2))
}

pub fn lag1_autocorr(values: &[f64]) -> f64 {
    if values.len() < 3 {
        return f64::NAN;
    }
    let a = &values[..values.len() - 1];
    let b = &values[1..];
    let ma = a.iter().sum::<f64>() / a.len() as f64;
    let mb = b.iter().sum::<f64>() / b.len() as f64;
    let num = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x - ma) * (y - mb))
        .sum::<f64>();
    let da = a.iter().map(|x| (x - ma).powi(2)).sum::<f64>().sqrt();
    let db = b.iter().map(|x| (x - mb).powi(2)).sum::<f64>().sqrt();
    if da == 0.0 || db == 0.0 {
        f64::NAN
    } else {
        num / (da * db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statistics_and_interpolated_tails() {
        let s = describe(&[1.0, 2.0, 3.0, 4.0, 5.0, f64::NAN, f64::INFINITY]).unwrap();
        assert_eq!(s.n, 5);
        assert_eq!(s.mean, 3.0);
        assert_eq!(s.median, 3.0);
        assert_eq!(s.variance, 2.5);
        assert!((s.p95 - 4.8).abs() < 1e-12);
        assert!((s.p99 - 4.96).abs() < 1e-12);
        assert!((s.stdev - 2.5f64.sqrt()).abs() < 1e-12);
        assert!((s.cv_pct - s.stdev / 3.0 * 100.0).abs() < 1e-12);
        assert!((s.ci95_high - (3.0 + 2.776 * s.sem)).abs() < 1e-12);
    }
    #[test]
    fn singleton_and_empty_do_not_claim_confidence() {
        assert!(describe(&[]).is_none());
        assert!(describe(&[f64::NAN]).is_none());
        let s = describe(&[7.0]).unwrap();
        assert!(s.ci95_low.is_nan());
        assert!(s.ci95_high.is_nan());
        assert!(describe(&[0.0, 0.0]).unwrap().cv_pct.is_nan());
        assert!(t95(31) > 1.96 && t95(31) < t95(30));
    }
    #[test]
    fn memory_trend_has_known_units_and_fit() {
        let (slope, r2) = regression(&[0.0, 0.5, 1.0], &[10.0, 11.0, 12.0]).unwrap();
        assert_eq!(slope, 2.0);
        assert_eq!(r2, 1.0);
        assert_eq!(regression(&[0.0, 1.0], &[10.0, 10.0]), Some((0.0, 1.0)));
        assert!(regression(&[1.0, 1.0], &[0.0, 1.0]).is_none());
        assert!(regression(&[0.0, 1.0], &[0.0, f64::NAN]).is_none());
    }
}
