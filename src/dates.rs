//! Minimal UTC calendar math (days-from-civil algorithm), avoiding a date-time dependency.

pub fn civil(ts: i64) -> (i64, u32, u32) {
    let z = ts.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

pub fn fmt_date(ts: i64) -> String {
    let (y, m, d) = civil(ts);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn day_index(ts: i64) -> i64 {
    ts.div_euclid(86_400)
}

/// Monday-based week number since the epoch (1970-01-01 was a Thursday).
pub fn week_index(ts: i64) -> i64 {
    (day_index(ts) + 3).div_euclid(7)
}

pub fn month_index(ts: i64) -> i64 {
    let (y, m, _) = civil(ts);
    y * 12 + i64::from(m)
}
