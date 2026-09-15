//! HIGHER CLOSED LOOP V1 —— 学习日（UTC+8）日期工具。
//!
//! 全项目口径（DEV-0049 §11.4）：学习日 = UTC+8 日历日。
//! 本模块只做纯日期运算，不读数据库、不依赖系统时区设置。

use chrono::{Duration, NaiveDate, Utc};

/// 本地学习日（UTC+8）：YYYY-MM-DD。
pub fn today_local() -> String {
    let tz = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 固定偏移恒合法");
    Utc::now().with_timezone(&tz).format("%Y-%m-%d").to_string()
}

/// 当前时刻（UTC，秒精度）：YYYY-MM-DDTHH:MM:SSZ。
pub fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// base ± days；base 非法 → Err（不静默返回原值）。
pub fn date_offset(base: &str, days: i64) -> Result<String, String> {
    let d = NaiveDate::parse_from_str(base, "%Y-%m-%d")
        .map_err(|e| format!("非法日期 {}：{}", base, e))?;
    let shifted = d
        .checked_add_signed(Duration::days(days))
        .ok_or_else(|| format!("日期溢出：{} ± {} 天", base, days))?;
    Ok(shifted.format("%Y-%m-%d").to_string())
}

/// to - from（天）；任一非法 → Err。to 早于 from → 负数。
pub fn days_between(from: &str, to: &str) -> Result<i64, String> {
    let a = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|e| format!("非法日期 {}：{}", from, e))?;
    let b =
        NaiveDate::parse_from_str(to, "%Y-%m-%d").map_err(|e| format!("非法日期 {}：{}", to, e))?;
    Ok((b - a).num_days())
}

/// 是否在闭区间 [start, end] 内；空值视为不约束。
pub fn in_range(date: &str, start: Option<&str>, end: Option<&str>) -> bool {
    if let Some(s) = start {
        if let (Ok(a), Ok(b)) = (
            NaiveDate::parse_from_str(s, "%Y-%m-%d"),
            NaiveDate::parse_from_str(date, "%Y-%m-%d"),
        ) {
            if b < a {
                return false;
            }
        }
    }
    if let Some(e) = end {
        if let (Ok(a), Ok(b)) = (
            NaiveDate::parse_from_str(e, "%Y-%m-%d"),
            NaiveDate::parse_from_str(date, "%Y-%m-%d"),
        ) {
            if b > a {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_crosses_month_and_year() {
        assert_eq!(date_offset("2026-03-01", -1).unwrap(), "2026-02-28");
        assert_eq!(date_offset("2026-01-01", -1).unwrap(), "2025-12-31");
        assert_eq!(date_offset("2026-02-28", 14).unwrap(), "2026-03-14");
    }

    #[test]
    fn days_between_is_signed() {
        assert_eq!(days_between("2026-03-01", "2026-03-08").unwrap(), 7);
        assert_eq!(days_between("2026-03-08", "2026-03-01").unwrap(), -7);
    }

    #[test]
    fn range_bounds_are_inclusive() {
        assert!(in_range(
            "2026-03-05",
            Some("2026-03-05"),
            Some("2026-03-05")
        ));
        assert!(!in_range("2026-03-04", Some("2026-03-05"), None));
        assert!(in_range("2026-03-05", None, None));
    }

    #[test]
    fn invalid_date_is_an_error_not_silent() {
        assert!(date_offset("not-a-date", -1).is_err());
        assert!(days_between("2026-13-40", "2026-01-01").is_err());
    }
}
