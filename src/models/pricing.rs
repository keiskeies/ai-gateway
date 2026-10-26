use chrono::{DateTime, Local, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

/// 高峰时段；支持跨午夜（start > end）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricingWindow {
    pub start: String,
    pub end: String,
}

/// 高峰时段的价格覆盖；缺的字段回退基础价。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PeakTier {
    #[serde(default)]
    pub input: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
    #[serde(default)]
    pub output: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PeakPricing {
    #[serde(default)]
    pub windows: Vec<PricingWindow>,
    #[serde(default)]
    pub peak: Option<PeakTier>,
}

/// 热路径用的价格快照：基础价（默认/空闲）+ 可选高峰覆盖。
#[derive(Debug, Clone, PartialEq)]
pub struct Pricing {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub peak: Option<PeakPricing>,
}

fn parse_hhmm(raw: &str) -> Option<NaiveTime> {
    let (hour, minute) = raw.trim().split_once(':')?;
    let hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.parse().ok()?;
    NaiveTime::from_hms_opt(hour, minute, 0)
}

impl PeakPricing {
    fn active_tier(&self, at: DateTime<Local>) -> Option<&PeakTier> {
        let peak = self.peak.as_ref()?;
        let now = at.time();
        let current = now.hour() * 60 + now.minute();
        for window in &self.windows {
            let start = parse_hhmm(&window.start)?;
            let end = parse_hhmm(&window.end)?;
            let start = start.hour() * 60 + start.minute();
            let end = end.hour() * 60 + end.minute();
            if start <= end {
                if current >= start && current < end {
                    return Some(peak);
                }
            } else if current >= start || current < end {
                return Some(peak);
            }
        }
        None
    }

    /// 当前是否落在任一高峰窗口内（大屏的"高峰计费中"状态）
    pub fn is_active(&self, at: DateTime<Local>) -> bool {
        self.active_tier(at).is_some()
    }

    /// 下一次价格切换时刻：所有窗口起点/终点里最近的一个。
    /// 窗口按天重复，只需看今天和明天；跨午夜的窗口也由"取最近边界"自然覆盖。
    pub fn next_change(&self, at: DateTime<Local>) -> Option<DateTime<Local>> {
        let mut best: Option<DateTime<Local>> = None;
        for window in &self.windows {
            for raw in [&window.start, &window.end] {
                let Some(time) = parse_hhmm(raw) else { continue };
                // DST 空洞里的本地时刻不存在：跳过这一个边界，别让整份配置都算不出下一次切换
                let naive = at.date_naive().and_time(time);
                let Some(mut candidate) = Local.from_local_datetime(&naive).earliest() else {
                    continue;
                };
                if candidate <= at {
                    let Some(tomorrow) = Local
                        .from_local_datetime(&(naive + chrono::Duration::days(1)))
                        .earliest()
                    else {
                        continue;
                    };
                    candidate = tomorrow;
                }
                best = Some(match best {
                    Some(current) => current.min(candidate),
                    None => candidate,
                });
            }
        }
        best
    }
}

impl Pricing {
    /// token 明细与价格齐备才算成本；缓存 token 缺失按 0。
    /// 基础价视为空闲/默认价，高峰窗口命中时用 peak 覆盖、缺项回退基础价。
    pub fn cost(
        &self,
        token_input: Option<i64>,
        token_output: Option<i64>,
        cache_read: Option<i64>,
        cache_write: Option<i64>,
        at: DateTime<Local>,
    ) -> Option<f64> {
        let input = token_input? as f64;
        let output = token_output? as f64;
        let hit = cache_read.unwrap_or(0).max(0) as f64;
        let write = cache_write.unwrap_or(0).max(0) as f64;
        let fresh = (input - hit - write).max(0.0);
        let tier = self.peak.as_ref().and_then(|peak| peak.active_tier(at));
        let input_price = tier.and_then(|t| t.input).unwrap_or(self.input);
        let cache_read_price = tier.and_then(|t| t.cache_read).unwrap_or(self.cache_read);
        let cache_write_price = tier.and_then(|t| t.cache_write).unwrap_or(self.cache_write);
        let output_price = tier.and_then(|t| t.output).unwrap_or(self.output);
        Some(
            (fresh * input_price
                + hit * cache_read_price
                + write * cache_write_price
                + output * output_price)
                / 1_000_000.0,
        )
    }

    /// 缓存命中相对"按未命中价计费"的价差（正数=省下，负数=命中反而更贵）。
    /// 与 `cost` 使用同一份高峰快照，所以两边的档位永远一致。
    /// `cache_read` 为 None（上游没回缓存数据）时无从比较，返回 None。
    pub fn cache_savings(&self, cache_read: Option<i64>, at: DateTime<Local>) -> Option<f64> {
        let hit = cache_read?.max(0) as f64;
        let tier = self.peak.as_ref().and_then(|peak| peak.active_tier(at));
        let input_price = tier.and_then(|t| t.input).unwrap_or(self.input);
        let cache_read_price = tier.and_then(|t| t.cache_read).unwrap_or(self.cache_read);
        Some(hit * (input_price - cache_read_price) / 1_000_000.0)
    }
}

/// 管理接口落库前的校验与归一。null/空对象 → None；非法窗口/负价直接拒绝。
pub fn validate_peak_pricing(value: &serde_json::Value) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let parsed: PeakPricing = serde_json::from_value(value.clone())
        .map_err(|e| format!("peak_pricing 格式无效: {e}"))?;
    if parsed.windows.is_empty() && parsed.peak.is_none() {
        return Ok(None);
    }
    let Some(peak) = parsed.peak.as_ref() else {
        return Err("启用高峰价时至少需要一个高峰时段".to_string());
    };
    if parsed.windows.is_empty() {
        return Err("启用高峰价时至少需要一个高峰时段".to_string());
    }
    for window in &parsed.windows {
        let Some(start) = parse_hhmm(&window.start) else {
            return Err(format!("高峰时段起点无效: {}", window.start));
        };
        let Some(end) = parse_hhmm(&window.end) else {
            return Err(format!("高峰时段终点无效: {}", window.end));
        };
        if start == end {
            return Err("高峰时段的起点和终点不能相同".to_string());
        }
    }
    for (name, price) in [
        ("input", peak.input),
        ("cache_read", peak.cache_read),
        ("cache_write", peak.cache_write),
        ("output", peak.output),
    ] {
        if let Some(price) = price {
            if !price.is_finite() || price < 0.0 {
                return Err(format!("高峰价 {name} 必须是不小于 0 的数字"));
            }
        }
    }
    if peak.input.is_none() && peak.cache_read.is_none() && peak.cache_write.is_none() && peak.output.is_none() {
        return Err("高峰价至少要填一个价格".to_string());
    }
    serde_json::to_string(&parsed)
        .map(Some)
        .map_err(|e| format!("peak_pricing 序列化失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 26, hour, minute, 0).unwrap()
    }

    fn pricing() -> Pricing {
        Pricing {
            input: 1.0,
            output: 4.0,
            cache_read: 0.02,
            cache_write: 2.0,
            peak: Some(PeakPricing {
                windows: vec![PricingWindow { start: "08:30".into(), end: "17:30".into() }],
                peak: Some(PeakTier {
                    input: Some(2.0),
                    cache_read: Some(0.04),
                    cache_write: None,
                    output: Some(8.0),
                }),
            }),
        }
    }

    #[test]
    fn cache_hit_miss_and_write_are_priced_separately() {
        // 1M prompt = 200k hit + 100k write + 700k miss; 500k output
        let cost = pricing().cost(Some(1_000_000), Some(500_000), Some(200_000), Some(100_000), at(10, 0)).unwrap();
        // 0.7*2 + 0.2*0.04 + 0.1*2 (write fallback base) + 0.5*8
        assert!((cost - 5.608).abs() < 1e-12, "peak cost {cost}");
        let off = pricing().cost(Some(1_000_000), Some(500_000), Some(200_000), Some(100_000), at(3, 0)).unwrap();
        // 0.7*1 + 0.2*0.02 + 0.1*2 + 0.5*4
        assert!((off - 2.904).abs() < 1e-12, "off-peak cost {off}");
    }

    #[test]
    fn old_prices_keep_the_old_formula() {
        let flat = Pricing { input: 2.5, output: 4.0, cache_read: 2.5, cache_write: 2.5, peak: None };
        let cost = flat.cost(Some(1000), Some(500), Some(200), Some(0), at(12, 0)).unwrap();
        // hit/write fallback to input; fresh+hit = total input -> same as old
        assert!((cost - (1000.0 * 2.5 + 500.0 * 4.0) / 1_000_000.0).abs() < 1e-12);
    }

    #[test]
    fn windows_support_wrapping_midnight() {
        let p = PeakPricing {
            windows: vec![PricingWindow { start: "22:00".into(), end: "02:00".into() }],
            peak: Some(PeakTier { input: Some(9.0), ..Default::default() }),
        };
        assert!(p.active_tier(at(23, 0)).is_some());
        assert!(p.active_tier(at(1, 0)).is_some());
        assert!(p.active_tier(at(12, 0)).is_none());
        assert!(p.is_active(at(23, 0)));
        assert!(!p.is_active(at(12, 0)));
    }

    #[test]
    fn next_change_finds_the_nearest_boundary() {
        let p = PeakPricing {
            windows: vec![PricingWindow { start: "22:00".into(), end: "02:00".into() }],
            peak: Some(PeakTier { input: Some(9.0), ..Default::default() }),
        };
        // 高峰中 → 最近边界是次日 02:00；空闲 → 今天 22:00
        assert_eq!(p.next_change(at(23, 0)).unwrap().time(), parse_hhmm("02:00").unwrap());
        assert_eq!(p.next_change(at(1, 0)).unwrap().time(), parse_hhmm("02:00").unwrap());
        assert_eq!(p.next_change(at(12, 0)).unwrap().time(), parse_hhmm("22:00").unwrap());
        // 多个窗口取最近的边界
        let multi = PeakPricing {
            windows: vec![
                PricingWindow { start: "08:30".into(), end: "17:30".into() },
                PricingWindow { start: "22:00".into(), end: "02:00".into() },
            ],
            peak: Some(PeakTier { input: Some(9.0), ..Default::default() }),
        };
        assert_eq!(multi.next_change(at(17, 0)).unwrap().time(), parse_hhmm("17:30").unwrap());
    }

    #[test]
    fn cache_savings_use_the_same_tier_as_cost() {
        let p = pricing();
        // 高峰：命中价 0.04 vs 未命中 2.0，20 万命中 → 0.2 × 1.96
        let peak = p.cache_savings(Some(200_000), at(10, 0)).unwrap();
        assert!((peak - 0.392).abs() < 1e-12, "peak savings {peak}");
        // 空闲：0.02 vs 1.0
        let off = p.cache_savings(Some(200_000), at(3, 0)).unwrap();
        assert!((off - 0.196).abs() < 1e-12, "off-peak savings {off}");
        // 上游没回缓存数据时无从比较
        assert_eq!(p.cache_savings(None, at(10, 0)), None);
        // 零命中是真实的 0，不是"不知道"
        assert_eq!(p.cache_savings(Some(0), at(10, 0)), Some(0.0));
    }

    #[test]
    fn cache_savings_can_be_negative_when_hit_costs_more() {
        let p = Pricing { input: 1.0, output: 2.0, cache_read: 3.0, cache_write: 1.0, peak: None };
        let delta = p.cache_savings(Some(1_000_000), at(12, 0)).unwrap();
        assert!((delta + 2.0).abs() < 1e-12, "命中反而更贵应是负数 {delta}");
    }

    #[test]
    fn validation_rejects_bad_windows_and_negative_prices() {
        assert!(validate_peak_pricing(&serde_json::json!(null)).unwrap().is_none());
        assert!(validate_peak_pricing(&serde_json::json!({
            "windows": [{"start":"08:30","end":"17:30"}],
            "peak": {"input": 2}
        })).unwrap().is_some());
        assert!(validate_peak_pricing(&serde_json::json!({
            "windows": [{"start":"08:30","end":"08:30"}],
            "peak": {"input": 2}
        })).is_err());
        assert!(validate_peak_pricing(&serde_json::json!({
            "windows": [{"start":"08:30","end":"17:30"}],
            "peak": {"input": -1}
        })).is_err());
        assert!(validate_peak_pricing(&serde_json::json!({
            "windows": [],
            "peak": {"input": 2}
        })).is_err());
    }
}
