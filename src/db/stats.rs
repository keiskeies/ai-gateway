use crate::error::AppResult;
use crate::models::stats::*;
use crate::db::DbPool;
use std::collections::{BTreeSet, HashSet};

/// 规范化时间范围参数：解析 RFC3339 后统一转为 UTC 秒精度字符串（`...SS+00:00`），
/// 与带纳秒的 `created_at` 做字典序比较。带小数一律进位到下一秒，两个边界都不截断：
/// `to` 是 `<` 的上界，向下截断会把这一秒内写入的行全部排除（实时尾巴上最新那一两条
/// 因此永远查不出来）；`from` 是 `>=` 的下界，向下截断会把这一秒开头的行多算进来。
fn normalize_bound(s: &str, field: &str) -> AppResult<String> {
    use chrono::Timelike;
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| {
            let utc = dt.with_timezone(&chrono::Utc);
            let floored = utc.with_nanosecond(0).unwrap_or(utc);
            if floored == utc {
                floored
            } else {
                floored + chrono::Duration::seconds(1)
            }
        })
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
        .map_err(|_| crate::error::AppError::BadRequest(format!("invalid {} timestamp, expected RFC3339", field)))
}

/// 统计接口通用时间范围（已规范化的 UTC 秒精度边界，半开区间 [from, to)）。
/// 概览页所有统计（KPI/错误/热力图/分位数/proxy/platform/时序）共用同一过滤口径。
struct TimeRange {
    from: Option<String>,
    to: Option<String>,
}

impl TimeRange {
    fn parse(from: Option<&str>, to: Option<&str>) -> AppResult<Self> {
        Ok(Self {
            from: from.map(|s| normalize_bound(s, "from")).transpose()?,
            to: to.map(|s| normalize_bound(s, "to")).transpose()?,
        })
    }

    fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }

    /// 列上的 AND 过滤片段（追加到已有 WHERE 之后），条件顺序与 params() 一致：from 在前、to 在后
    fn and(&self, col: &str) -> String {
        let mut s = String::new();
        if self.from.is_some() {
            s.push_str(&format!(" AND {col} >= ?"));
        }
        if self.to.is_some() {
            s.push_str(&format!(" AND {col} < ?"));
        }
        s
    }

    /// 完整 WHERE 子句（无条件时返回空串，保持原 SQL 不变）
    fn where_clause(&self, col: &str) -> String {
        if self.is_empty() {
            String::new()
        } else {
            format!(" WHERE 1=1{}", self.and(col))
        }
    }

    fn params(&self) -> Vec<&str> {
        self.from
            .iter()
            .chain(self.to.iter())
            .map(|s| s.as_str())
            .collect()
    }
}

/// D 第二阶段：把秒级半开区间 [from, to) 拆成「整小时段 + 两个边界」的预聚合查询框架。
///
/// 小时表只按整小时桶聚合，而 from/to 是任意秒。直接全用小时表会丢失边界内的精度
/// （例如 [14:37:12, 15:01:00) 会整桶吞掉 14:00 与 15:00 两小时的全部数据）。
/// 做法：中间被完整覆盖的小时走 `request_stats_hourly`，首尾两个不完整小时回退明细表。
/// 三个查询 UNION ALL 后语义与原来全程查明细完全一致。
///
/// 桶的时区口径与写端 upsert_hourly_aggregate 一致：本地时区 "%Y-%m-%d %H"。
#[derive(Debug)]
struct AggRange {
    /// 整小时段下界（本地时区桶字符串，包含），None = 没有完整小时（区间不足一小时）
    mid_from: Option<String>,
    /// 整小时段上界（本地时区桶字符串，不包含）
    mid_to: Option<String>,
    /// 边界明细回退查询用的原边界（沿用调用方已规范化的 UTC from/to，原样透传）
    from: Option<String>,
    to: Option<String>,
    /// 整小时段下界的 UTC 整秒字符串（created_at 可比，含），供前边界明细段作上界
    core_from: Option<String>,
    /// 整小时段上界的 UTC 整秒字符串（created_at 可比，不含），供后边界明细段作下界
    core_to: Option<String>,
}
/// 对已规范化的 [from, to) 计算整小时段（本地时区桶口径，与写端一致）：
/// - mid_from = from 进位到下一个整小时；mid_to = to 截断到当前整小时
/// - 区间内不足一个完整小时（mid_from >= mid_to）→ 返回 None，调用方整体回退明细
/// - 开放式范围（缺 from 或缺 to）没有安全边界 → None，回退明细
/// 小时段边界同时给出两套表示：桶字符串（hourly 表 bucket 比较）与 UTC 整秒
/// （明细 created_at 比较）——两者各司其职，混用会导致字符串比较失真（'T' vs ' '）。
fn hourly_agg_range(from: Option<&str>, to: Option<&str>) -> AggRange {
    use chrono::{DateTime, TimeZone, Timelike};
    // 返回 (桶字符串, UTC 整秒)。把本地 naive 时刻解释回本地再转 UTC，防 DST 歧义：
    // 时区跳变导致时刻不存在/重复时解析失败，整体回退明细（安全降级，只丢小时表加速）。
    let ceil_hour = |s: &str| -> Option<(String, String)> {
        let dt = DateTime::parse_from_rfc3339(s).ok()?.with_timezone(&chrono::Local);
        let base = dt.date_naive().and_hms_opt(dt.hour(), 0, 0).expect("hour");
        let dt_naive = dt.naive_local();
        let bucket_naive = if base == dt_naive { base } else { base + chrono::Duration::hours(1) };
        let bucket = bucket_naive.format("%Y-%m-%d %H").to_string();
        let utc = chrono::Local
            .from_local_datetime(&bucket_naive)
            .single()?
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        Some((bucket, utc))
    };
    let floor_hour = |s: &str| -> Option<(String, String)> {
        let dt = DateTime::parse_from_rfc3339(s).ok()?.with_timezone(&chrono::Local);
        let naive = dt.date_naive().and_hms_opt(dt.hour(), 0, 0)?;
        let bucket = naive.format("%Y-%m-%d %H").to_string();
        let utc = chrono::Local
            .from_local_datetime(&naive)
            .single()?
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        Some((bucket, utc))
    };
    match (from, to) {
        (Some(f), Some(t)) => {
            let (from_ceil, to_floor) = (ceil_hour(f), floor_hour(t));
            let (mid_from, mid_to, core_from, core_to) = match (from_ceil, to_floor) {
                (Some((a, ca)), Some((b, cb))) if a < b => (Some(a), Some(b), Some(ca), Some(cb)),
                _ => (None, None, None, None),
            };
            AggRange { mid_from, mid_to, core_from, core_to, from: Some(f.to_string()), to: Some(t.to_string()) }
        }
        // 开放式范围没有"完整小时段"可言，回退全程明细（与现状一致）
        _ => AggRange {
            mid_from: None,
            mid_to: None,
            core_from: None,
            core_to: None,
            from: from.map(str::to_string),
            to: to.map(str::to_string),
        },
    }
}

/// 生成时间范围内的连续周期 key（格式与 SQL 里 STRFTIME 一致），填补无数据周期造成的折线跳空。/// - hour 粒度按服务器本地时区（与聚合表达式 datetime(...,'localtime') 口径一致）；
/// - day/month/year 沿用 UTC（与聚合用裸 created_at 一致）。
/// 上限 4000 点防失控；解析失败返回空（调用方回退为"仅有数据的周期"）。
fn fill_periods(granularity: &str, from: &str, to: &str) -> Vec<String> {
    use chrono::{Datelike, Timelike, Utc};
    let f = chrono::DateTime::parse_from_rfc3339(from).ok().map(|v| v.with_timezone(&Utc));
    let t = chrono::DateTime::parse_from_rfc3339(to).ok().map(|v| v.with_timezone(&Utc));
    let (f, t) = match (f, t) {
        (Some(f), Some(t)) if t >= f => (f, t),
        _ => return vec![],
    };
    let mut out = Vec::new();
    match granularity {
        "hour" => {
            let fl = f.with_timezone(&chrono::Local).naive_local();
            let tl = t.with_timezone(&chrono::Local).naive_local();
            let (Some(mut cur), Some(end)) = (
                fl.date().and_hms_opt(fl.hour(), 0, 0),
                tl.date().and_hms_opt(tl.hour(), 0, 0),
            ) else { return vec![] };
            while cur <= end && out.len() < 4000 {
                out.push(cur.format("%Y-%m-%d %H").to_string());
                cur += chrono::Duration::hours(1);
            }
        }
        "day" => {
            let mut cur = f.date_naive();
            let end = t.date_naive();
            while cur <= end && out.len() < 4000 {
                out.push(cur.format("%Y-%m-%d").to_string());
                cur += chrono::Duration::days(1);
            }
        }
        "month" => {
            let mut cur = (f.year(), f.month());
            let end = (t.year(), t.month());
            while cur <= end && out.len() < 4000 {
                out.push(format!("{:04}-{:02}", cur.0, cur.1));
                cur = if cur.1 == 12 { (cur.0 + 1, 1) } else { (cur.0, cur.1 + 1) };
            }
        }
        "year" => {
            let mut cur = f.year();
            while cur <= t.year() && out.len() < 4000 {
                out.push(format!("{:04}", cur));
                cur += 1;
            }
        }
        _ => {}
    }
    out
}

/// 模型时间序列统计：按小时/日/月/年粒度聚合各模型的请求数与 Token 用量。
/// - 提供 from/to 时以时间范围为准，范围内周期连续补齐（无数据的周期补 0 点）；否则查询所有有数据的周期，按 limit 截取最近 N 个。
/// - hour 粒度按本地时区聚合（与热力图口径一致），其余粒度沿用 UTC 原始值。
/// - hour 粒度读 `request_stats_hourly`（bucket 即本地时区整小时，与 STRFTIME(datetime(...,'localtime')) 同口径）；
///   其余粒度走明细表。
pub fn model_timeseries(
    pool: &DbPool,
    granularity: &str,
    limit: usize,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<ModelTimeSeriesStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // 规范化边界（RFC3339 → UTC 秒精度）；任一存在即以范围为准，跳过 limit 截取
    let range = TimeRange::parse(from, to)?;
    let range_mode = !range.is_empty();

    let all_data: Vec<ModelTimeSeriesPoint> = if granularity == "hour" {
        model_timeseries_hourly(&conn, &range)?
    } else {
        model_timeseries_detail(&conn, granularity, from, to)?
    };

    // 收集所有出现过的周期并排序：范围模式下按 from/to 生成连续周期（填补无数据小时的跳空），
    // 生成失败时回退为"仅有数据的周期"；非范围模式取最近 limit 个。
    let all_periods: BTreeSet<String> = all_data.iter().map(|p| p.period.clone()).collect();
    let periods: Vec<String> = if range_mode {
        let filled = match (&range.from, &range.to) {
            (Some(f), Some(t)) => fill_periods(granularity, f, t),
            _ => vec![],
        };
        if filled.is_empty() {
            all_periods.iter().cloned().collect()
        } else {
            // 以连续序列为骨架，并保留序列之外仍出现过的周期（如 to 之后的部分数据，
            // 理论上已被 WHERE 过滤，这里仅作兜底，避免静默丢数据）
            let mut set: BTreeSet<String> = filled.into_iter().collect();
            set.extend(all_periods);
            set.into_iter().collect()
        }
    } else {
        all_periods
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    let period_set: HashSet<&str> = periods.iter().map(|s| s.as_str()).collect();

    // 只保留选中周期内的数据
    let data: Vec<ModelTimeSeriesPoint> = all_data
        .into_iter()
        .filter(|p| period_set.contains(p.period.as_str()))
        .collect();

    Ok(ModelTimeSeriesStats {
        granularity: granularity.to_string(),
        periods,
        data,
    })
}

/// hour 粒度的实现：数据来自 `request_stats_hourly`，只在完整小时段命中；
/// 首尾两个不完整小时回退明细（created_at 与 UTC 整秒比较），三段 UNION ALL 后
/// 按 (period, model_id) 归组——与查询明细的分桶口径逐字段一致（query_metrics 同构）。
/// bucket 列即本地时区整小时字符串，明细段的 period 用
/// STRFTIME('%Y-%m-%d %H', datetime(rs.created_at,'localtime')) 生成，两段对齐。
fn model_timeseries_hourly(
    conn: &rusqlite::Connection,
    range: &TimeRange,
) -> AppResult<Vec<ModelTimeSeriesPoint>> {
    let agg = hourly_agg_range(range.from.as_deref(), range.to.as_deref());
    // 列序（三段严格一致）：
    // 0 period / 1 model_id / 2 requests / 3 token_input / 4 token_output
    // 5 latency_ms_sum / 6 failed_requests / 7 stream_requests / 8 ttft_ms_sum / 9 ttft_samples
    // 10 cache_read / 11 cache_write / 12 cost_sum / 13 priced_requests
    let hourly_select = |extra: &str| -> String {
        format!(
            "SELECT
                h.bucket AS period,
                COALESCE(b.model_id, 'unknown') AS model_id,
                h.requests,
                h.token_input,
                h.token_output,
                h.latency_ms_sum,
                h.failed_requests,
                h.stream_requests,
                h.ttft_ms_sum,
                h.ttft_samples,
                h.cache_read_tokens,
                h.cache_write_tokens,
                h.cost_sum,
                h.priced_requests
             FROM request_stats_hourly h
             LEFT JOIN backends b ON b.id = h.backend_id
             WHERE 1=1{extra}"
        )
    };
    let detail_select = |extra: &str| -> String {
        format!(
            "SELECT
                STRFTIME('%Y-%m-%d %H', datetime(rs.created_at, 'localtime')) AS period,
                COALESCE(b.model_id, 'unknown') AS model_id,
                COUNT(*) AS requests,
                COALESCE(SUM(rs.token_input), 0) AS token_input,
                COALESCE(SUM(rs.token_output), 0) AS token_output,
                COALESCE(SUM(rs.latency_ms), 0) AS latency_ms_sum,
                SUM(CASE WHEN NOT (rs.status_code >= 200 AND rs.status_code < 300) THEN 1 ELSE 0 END) AS failed_requests,
                SUM(rs.is_stream) AS stream_requests,
                COALESCE(SUM(rs.ttft_ms), 0) AS ttft_ms_sum,
                COUNT(rs.ttft_ms) AS ttft_samples,
                COALESCE(SUM(rs.cache_read_tokens), 0) AS cache_read_tokens,
                COALESCE(SUM(rs.cache_write_tokens), 0) AS cache_write_tokens,
                SUM(rs.cost) AS cost_sum,
                COUNT(rs.cost) AS priced_requests
             FROM request_stats rs
             LEFT JOIN backends b ON b.id = rs.backend_id
             WHERE 1=1{extra}
             GROUP BY period, model_id"
        )
    };
    let (sql, params): (String, Vec<String>) =
        if let (Some(mf), Some(mt), Some(cf), Some(ct), Some(f), Some(t)) =
            (&agg.mid_from, &agg.mid_to, &agg.core_from, &agg.core_to, &agg.from, &agg.to)
        {
            (
                format!(
                    "SELECT
                        period, model_id,
                        SUM(requests) AS requests,
                        SUM(token_input) AS token_input,
                        SUM(token_output) AS token_output,
                        SUM(latency_ms_sum) AS latency_ms_sum,
                        SUM(failed_requests) AS failed_requests,
                        SUM(stream_requests) AS stream_requests,
                        SUM(ttft_ms_sum) AS ttft_ms_sum,
                        SUM(ttft_samples) AS ttft_samples,
                        SUM(cache_read_tokens) AS cache_read_tokens,
                        SUM(cache_write_tokens) AS cache_write_tokens,
                        SUM(cost_sum) AS cost_sum,
                        SUM(priced_requests) AS priced_requests
                     FROM (
                        {}
                        UNION ALL
                        {}
                        UNION ALL
                        {}
                     ) u
                     GROUP BY period, model_id",
                    hourly_select(" AND h.bucket >= ?1 AND h.bucket < ?2"),
                    detail_select(" AND rs.created_at >= ?3 AND rs.created_at < ?4"),
                    detail_select(" AND rs.created_at >= ?5 AND rs.created_at < ?6"),
                ),
                vec![mf.clone(), mt.clone(), f.clone(), cf.clone(), ct.clone(), t.clone()],
            )
        } else {
            // 开放式范围或区间不足一小时：全程明细（与 D 之前一致）
            let cond = match (&agg.from, &agg.to) {
                (Some(_), Some(_)) => " AND rs.created_at >= ?1 AND rs.created_at < ?2".to_string(),
                (Some(_), None) | (None, Some(_)) => " AND rs.created_at >= ?1".to_string(),
                (None, None) => String::new(),
            };
            let params: Vec<String> = agg.from.iter().chain(agg.to.iter()).cloned().collect();
            (
                format!(
                    "SELECT
                        period, model_id,
                        SUM(requests) AS requests,
                        SUM(token_input) AS token_input,
                        SUM(token_output) AS token_output,
                        SUM(latency_ms_sum) AS latency_ms_sum,
                        SUM(failed_requests) AS failed_requests,
                        SUM(stream_requests) AS stream_requests,
                        SUM(ttft_ms_sum) AS ttft_ms_sum,
                        SUM(ttft_samples) AS ttft_samples,
                        SUM(cache_read_tokens) AS cache_read_tokens,
                        SUM(cache_write_tokens) AS cache_write_tokens,
                        SUM(cost_sum) AS cost_sum,
                        SUM(priced_requests) AS priced_requests
                     FROM (
                        {}
                     ) u
                     GROUP BY period, model_id",
                    detail_select(&cond),
                ),
                params,
            )
        };

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|s| s.as_str())),
        |r| {
            let token_input: i64 = r.get(3)?;
            let cache_read_tokens: i64 = r.get(10)?;
            let cache_write_tokens: i64 = r.get(11)?;
            let latency_sum: f64 = r.get(5)?;
            let requests: i64 = r.get(2)?;
            let ttft_sum: f64 = r.get(8)?;
            let ttft_samples: i64 = r.get(9)?;
            let token_output: i64 = r.get(4)?;
            Ok(ModelTimeSeriesPoint {
                period: r.get::<_, String>(0)?,
                model_id: r.get::<_, String>(1)?,
                requests,
                token_input,
                token_output,
                avg_latency_ms: if requests > 0 { latency_sum / requests as f64 } else { 0.0 },
                failed_requests: r.get::<_, i64>(6)?,
                stream_requests: r.get::<_, i64>(7)?,
                avg_ttft_ms: if ttft_samples > 0 { ttft_sum / ttft_samples as f64 } else { 0.0 },
                output_speed_tps: if latency_sum - ttft_sum > 0.0 {
                    token_output as f64 * 1000.0 / (latency_sum - ttft_sum)
                } else {
                    0.0
                },
                cache_read_tokens,
                cache_write_tokens,
                // 新增输入 = 总输入 − 缓存读 − 缓存写（saturing 防异常数据）
                fresh_input_tokens: (token_input - cache_read_tokens - cache_write_tokens).max(0),
                total_cost: r.get(12)?,
                priced_requests: r.get::<_, i64>(13)?,
                ttft_samples,
            })
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// day/month/year 粒度的实现：沿用原明细 SQL（created_at 裸列比较可走索引）。
/// hour 原本也走这里，被 `model_timeseries_hourly` 替代；两者输出列序一致。
fn model_timeseries_detail(
    conn: &rusqlite::Connection,
    granularity: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<ModelTimeSeriesPoint>> {
    let time_expr = "rs.created_at"; // 非 hour 粒度保持 UTC，裸列比较走 idx_stats_created
    let fmt = match granularity {
        "day" => "%Y-%m-%d",
        "month" => "%Y-%m",
        "year" => "%Y",
        _ => {
            return Err(crate::error::AppError::BadRequest(
                "invalid granularity, expected hour/day/month/year".into(),
            ))
        }
    };
    let range = TimeRange::parse(from, to)?;

    let sql = format!(
        r#"
        SELECT
            STRFTIME(?1, {time_expr}) AS period,
            COALESCE(b.model_id, 'unknown') AS model_id,
            COUNT(*)                     AS requests,
            COALESCE(SUM(rs.token_input), 0)  AS token_input,
            COALESCE(SUM(rs.token_output), 0) AS token_output,
            COALESCE(AVG(rs.latency_ms), 0)   AS avg_latency_ms,
            SUM(CASE WHEN NOT (rs.status_code >= 200 AND rs.status_code < 300) THEN 1 ELSE 0 END) AS failed_requests,
            SUM(rs.is_stream)                   AS stream_requests,
            COALESCE(AVG(rs.ttft_ms), 0)        AS avg_ttft_ms,
            CASE WHEN SUM(rs.latency_ms) - SUM(COALESCE(rs.ttft_ms, 0)) > 0
                 THEN COALESCE(SUM(rs.token_output), 0) * 1000.0 / (SUM(rs.latency_ms) - SUM(COALESCE(rs.ttft_ms, 0)))
                 ELSE 0 END                     AS output_speed_tps,
            COALESCE(SUM(rs.cache_read_tokens), 0)  AS cache_read_tokens,
            COALESCE(SUM(rs.cache_write_tokens), 0) AS cache_write_tokens,
            SUM(rs.cost)                            AS total_cost,
            COUNT(rs.cost)                          AS priced_requests,
            COUNT(rs.ttft_ms)                       AS ttft_samples
        FROM request_stats rs
        LEFT JOIN backends b ON rs.backend_id = b.id
        WHERE 1=1
          AND (?2 IS NULL OR rs.created_at >= ?2)
          AND (?3 IS NULL OR rs.created_at <  ?3)
        GROUP BY period, model_id
        ORDER BY period, model_id
        -- SQL 层硬上限：时间桶本身最多 4000 个，再乘模型数没有上界。
        -- 无 LIMIT 时全表分组结果会一次性进内存（曾经只在 Rust 侧截断）。
        LIMIT 20000
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![fmt, range.from, range.to],
        |r| {
            let token_input: i64 = r.get(3)?;
            let cache_read_tokens: i64 = r.get(10)?;
            let cache_write_tokens: i64 = r.get(11)?;
            Ok(ModelTimeSeriesPoint {
                period: r.get::<_, String>(0)?,
                model_id: r.get::<_, String>(1)?,
                requests: r.get::<_, i64>(2)?,
                token_input,
                token_output: r.get::<_, i64>(4)?,
                avg_latency_ms: r.get::<_, f64>(5)?,
                failed_requests: r.get::<_, i64>(6)?,
                stream_requests: r.get::<_, i64>(7)?,
                avg_ttft_ms: r.get::<_, f64>(8)?,
                output_speed_tps: r.get::<_, f64>(9)?,
                cache_read_tokens,
                cache_write_tokens,
                // 与 model_summaries 同一公式：新增输入 = 总输入 − 缓存读 − 缓存写
                fresh_input_tokens: (token_input - cache_read_tokens - cache_write_tokens).max(0),
                total_cost: r.get(12)?,
                priced_requests: r.get(13)?,
                ttft_samples: r.get(14)?,
            })
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 错误分析：非 2xx 请求按 error_type 聚合（历史悬挂/缺失归入 Unknown）。
/// `model` 可选：按 `backends.model_id` 过滤（与性能矩阵同一口径），供大屏失败数下钻。
pub fn error_stats(pool: &DbPool, from: Option<&str>, to: Option<&str>, model: Option<&str>) -> AppResult<ErrorStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // 参数顺序与 SQL 片段一致：range 的 from/to 在前，model 在最后
    let mut params: Vec<String> = range.params().into_iter().map(|s| s.to_string()).collect();
    let model_clause = match model {
        Some(model) => {
            params.push(model.to_string());
            " AND COALESCE(b.model_id, 'unknown') = ?"
        }
        None => "",
    };
    // request_stats 不存 model_id，模型名挂在 backends 上；矩阵也是这么 JOIN 的
    let from_join = "FROM request_stats rs LEFT JOIN backends b ON rs.backend_id = b.id";

    let count_in_range = |extra: &str| -> i64 {
        let sql = format!(
            "SELECT COUNT(*) {from_join} WHERE 1=1{extra}{}{model_clause}",
            range.and("rs.created_at")
        );
        conn.query_row(&sql, rusqlite::params_from_iter(params.clone()), |r| r.get(0)).unwrap_or(0)
    };

    let total = count_in_range("");
    let success = count_in_range(" AND rs.status_code >= 200 AND rs.status_code < 300");

    let sql = format!(
        "SELECT COALESCE(rs.error_type, 'Unknown') AS kind, COUNT(*) AS cnt
         {from_join}
         WHERE NOT (rs.status_code >= 200 AND rs.status_code < 300)
         {}{model_clause}
         GROUP BY kind
         ORDER BY cnt DESC",
        range.and("rs.created_at")
    );
    let mut stmt = conn.prepare(&sql)?;
    let errors = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok(ErrorBreakdown { kind: r.get(0)?, count: r.get(1)? })
        })?
        .filter_map(|x| x.ok())
        .collect();

    Ok(ErrorStats { total, success, errors })
}

/// 周 × 时活跃热力图：created_at 为 UTC RFC3339，聚合时转服务器本地时区
pub fn activity_heatmap(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<ActivityHeatmap> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let sql = format!(
        "SELECT CAST(strftime('%w', datetime(created_at, 'localtime')) AS INTEGER) AS weekday,
                CAST(strftime('%H', datetime(created_at, 'localtime')) AS INTEGER) AS hour,
                COUNT(*) AS cnt
         FROM request_stats
         {}
         GROUP BY weekday, hour",
        range.where_clause("created_at")
    );
    let mut stmt = conn.prepare(&sql)?;
    let cells = stmt
        .query_map(rusqlite::params_from_iter(range.params()), |r| {
            Ok(HeatmapCell { weekday: r.get(0)?, hour: r.get(1)?, count: r.get(2)? })
        })?
        .filter_map(|x| x.ok())
        .collect::<Vec<_>>();

    let max = cells.iter().map(|c| c.count).max().unwrap_or(0);
    Ok(ActivityHeatmap { cells, max })
}

/// 延迟分位数：ORDER BY + OFFSET 取分位点（SQLite 无内置 percentile）
pub fn latency_percentiles(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<LatencyPercentiles> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let n: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM request_stats{}", range.where_clause("created_at")),
            rusqlite::params_from_iter(range.params()),
            |r| r.get(0),
        )
        .unwrap_or(0);
    if n == 0 {
        return Ok(LatencyPercentiles { p50: 0.0, p90: 0.0, p99: 0.0, max: 0.0 });
    }

    // 三次分位点曾经各自 ORDER BY 全表一次（p50/p90/p99 各一次排序 + 一次 MAX），
    // 大表上是 4 遍扫描。这里改成「一次有序扫描，在目标偏移处取值」：
    // SQLite 仍会排序，但只排一次，且边读边判、读够就停。
    let offsets: [i64; 3] = [
        ((n as f64 * 0.50) as i64).clamp(0, n - 1),
        ((n as f64 * 0.90) as i64).clamp(0, n - 1),
        ((n as f64 * 0.99) as i64).clamp(0, n - 1),
    ];
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(latency_ms, 0) FROM request_stats{} ORDER BY latency_ms",
        range.where_clause("created_at")
    ))?;
    let mut rows = stmt.query(rusqlite::params_from_iter(range.params()))?;
    let mut out = [0.0f64; 3];
    let mut max = 0.0f64;
    let mut idx: i64 = 0;
    let mut next_slot = 0usize;
    while let Some(row) = rows.next()? {
        let v: f64 = row.get(0)?;
        max = max.max(v);
        while next_slot < 3 && idx >= offsets[next_slot] {
            out[next_slot] = v;
            next_slot += 1;
        }
        idx += 1;
    }
    Ok(LatencyPercentiles { p50: out[0], p90: out[1], p99: out[2], max })
}

pub fn record(pool: &DbPool, stat: &RequestStat) -> AppResult<()> {
    let mut conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    // 与 record_batch 同口径：明细 + 预聚合小时表同一事务双写。
    // 读端（overview / proxy_stats / platform_stats / model_summaries）已切到小时表，
    // 任何不经 record_batch 的单行写路径也必须同步维护小时表，否则报表会漏数据。
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, error_type, created_at, ttft_ms, cache_read_tokens, cache_write_tokens, is_stream, api_key_id, cost, cache_savings, request_id, replayed_from, path, credits) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)")?;
        stmt.execute(rusqlite::params![stat.proxy_id, stat.route_id, stat.backend_id, stat.status_code,
            stat.latency_ms, stat.token_input, stat.token_output, stat.error_type, stat.created_at,
            stat.ttft_ms, stat.cache_read_tokens, stat.cache_write_tokens, stat.is_stream as i64,
            stat.api_key_id, stat.cost, stat.cache_savings, stat.request_id, stat.replayed_from,
            stat.path, stat.credits])?;
    }
    upsert_hourly_aggregate(&tx, std::slice::from_ref(stat))?;
    tx.commit()?;
    Ok(())
}

/// 入站请求头落库。与正文分开：正文超限不存时，请求头仍然要留下。
pub fn record_headers(pool: &DbPool, headers: &crate::models::stats::RequestHeaders) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT OR REPLACE INTO request_headers (request_id, headers, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5)",
        rusqlite::params![headers.request_id, headers.headers, headers.bytes, headers.truncated as i64, headers.created_at],
    )?;
    Ok(())
}

/// 上游原始响应正文落库（截断后的内容）。
pub fn record_response_body(pool: &DbPool, body: &ResponseBody) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT OR REPLACE INTO response_bodies (request_id, protocol, status_code, content_type, body, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![body.request_id, body.protocol, body.status_code, body.content_type,
            body.body, body.bytes, body.truncated as i64, body.created_at],
    )?;
    Ok(())
}

/// 正文落库只在采集开着时被调用（见 proxy::handler）。`INSERT OR REPLACE`：
/// 同一次请求的 request_id 唯一，重复投递覆盖而不是多出一行孤儿。
pub fn record_body(pool: &DbPool, body: &RequestBody) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT OR REPLACE INTO request_bodies (request_id, protocol, body, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![body.request_id, body.protocol, body.body, body.bytes, body.truncated as i64, body.created_at],
    )?;
    Ok(())
}

/// 按明细行 ID 取正文：重放接口的入口就是列表里的 `request_stats.id`。
/// 没有正文行时返回 None（采集没开、请求早于 V21、或正文超限没存）。
/// 返回的 `truncated` 由调用方检查——截断的前缀只用于查看，不能重放。
pub fn body_for_stat(pool: &DbPool, stat_id: i64) -> AppResult<Option<RequestBody>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT b.request_id, b.protocol, b.body, b.bytes, b.truncated, b.created_at
         FROM request_stats s JOIN request_bodies b ON b.request_id = s.request_id
         WHERE s.id = ?1",
    )?;
    let found = stmt.query_map([stat_id], |r| {
        Ok(RequestBody {
            request_id: r.get(0)?,
            protocol: r.get(1)?,
            body: r.get(2)?,
            bytes: r.get(3)?,
            truncated: r.get::<_, i64>(4)? != 0,
            created_at: r.get(5)?,
        })
    })?
    .next()
    .transpose()?;
    Ok(found)
}

/// 详情页的请求/响应读数。请求头、请求正文、响应正文三者各自判断存在与否：
/// 长请求可能只有头没有正文，响应采集也可能独立于请求采集。
pub fn payload_for_stat(pool: &DbPool, stat_id: i64) -> AppResult<crate::models::stats::RequestPayload> {
    use crate::models::stats::{RequestPayload, RequestPayloadRequest, RequestPayloadResponse};
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    let request_id: Option<String> = match conn.query_row(
        "SELECT request_id FROM request_stats WHERE id = ?1",
        [stat_id],
        |r| r.get::<_, Option<String>>(0),
    ) {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(crate::error::AppError::NotFound("Request not found".to_string()))
        }
        Err(e) => return Err(e.into()),
    };
    let Some(request_id) = request_id else {
        return Ok(RequestPayload {
            request: RequestPayloadRequest {
                headers_captured: false, body_captured: false, protocol: None,
                headers: None, body: None, bytes: 0,
                headers_truncated: false, body_truncated: false,
            },
            response: RequestPayloadResponse {
                captured: false, protocol: None, status_code: None, content_type: None,
                body: None, bytes: 0, truncated: false,
            },
            nonce: None,
            request_capture_enabled: false,
            response_capture_enabled: false,
        });
    };

    let header_row = conn.query_row(
        "SELECT headers, bytes, truncated FROM request_headers WHERE request_id = ?1",
        [&request_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)? != 0)),
    ).ok();
    let body_row = conn.query_row(
        "SELECT protocol, body, bytes, truncated FROM request_bodies WHERE request_id = ?1",
        [&request_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)? != 0)),
    ).ok();
    let response_row = conn.query_row(
        "SELECT protocol, status_code, content_type, body, bytes, truncated FROM response_bodies WHERE request_id = ?1",
        [&request_id],
        |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)? != 0,
        )),
    ).ok();

    let (headers_captured, headers, _header_bytes, header_truncated) = match header_row {
        Some((raw, bytes, truncated)) => {
            let parsed = serde_json::from_str(&raw).ok();
            (parsed.is_some(), parsed, bytes, truncated)
        }
        None => (false, None, 0, false),
    };
    let (body_captured, protocol, request_body, body_bytes, body_truncated) = match body_row {
        Some((protocol, body, bytes, truncated)) => (true, Some(protocol), Some(body), bytes, truncated),
        None => (false, None, None, 0, false),
    };

    let nonce: Option<String> = conn.query_row(
        "SELECT nonce FROM request_attempts WHERE request_id = ?1 ORDER BY attempt_no DESC LIMIT 1",
        [&request_id],
        |r| r.get::<_, Option<String>>(0),
    ).ok().flatten();

    Ok(RequestPayload {
        nonce,
        request: RequestPayloadRequest {
            headers_captured,
            body_captured,
            protocol,
            headers,
            body: request_body,
            bytes: body_bytes,
            headers_truncated: header_truncated,
            body_truncated,
        },
        response: match response_row {
            Some((protocol, status_code, content_type, body, bytes, truncated)) => RequestPayloadResponse {
                captured: true,
                protocol: Some(protocol),
                status_code: Some(status_code),
                content_type,
                body: Some(body),
                bytes,
                truncated,
            },
            None => RequestPayloadResponse {
                captured: false, protocol: None, status_code: None, content_type: None,
                body: None, bytes: 0, truncated: false,
            },
        },
        request_capture_enabled: false,
        response_capture_enabled: false,
    })
}

/// 清空全部正文的结果。`deleted` 是行数，`reclaimed_bytes` 是 VACUUM 真正还给文件系统的字节数——
/// 确认框承诺的是"删掉 N 条 · X MB"，删完就得让文件体积真的掉下来，否则用户看到"已删除"却
/// 发现库文件一点没小，会合理地认为没生效。
#[derive(serde::Serialize)]
pub struct BodyWipeResult {
    pub deleted: i64,
    pub reclaimed_bytes: i64,
    /// 有别的连接占着读锁时 VACUUM 会失败；行已经删了，但字节还在空闲页里，如实报出来。
    pub compacted: bool,
}

/// 清空全部正文：关掉采集开关后把已经存进去的提示词/响应抹掉。
/// 只删正文与请求头表，统计与轨迹留着——它们本来就不含正文。
///
/// SQLite 的 DELETE 只把页标成空闲，既不缩文件，页里的提示词字节也还在：
/// 拿到 .db 的人照样能从空闲页捞回正文。所以删完必须 VACUUM 重写库文件，
/// 再 checkpoint(TRUNCATE) 把新页折回主文件（顺序见 schema.rs 的 V17 迁移）。
pub fn delete_all_bodies(pool: &DbPool) -> AppResult<BodyWipeResult> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let size_before = std::fs::metadata(resolved_db_path_of(&conn))
        .map(|m| m.len())
        .unwrap_or(0);

    let mut deleted = conn.execute("DELETE FROM request_bodies", [])? as i64;
    deleted += conn.execute("DELETE FROM request_headers", [])? as i64;
    deleted += conn.execute("DELETE FROM response_bodies", [])? as i64;

    // 没有任何行可删时不必压实：空闲页可能是别的清理留下的，等有正文可抹时一起收。
    let compacted = if deleted > 0 {
        match conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);") {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("清空正文后 VACUUM 失败，提示词仍留在空闲页: {}", e);
                false
            }
        }
    } else {
        true
    };
    let size_after = std::fs::metadata(resolved_db_path_of(&conn))
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(BodyWipeResult {
        deleted,
        reclaimed_bytes: size_before.saturating_sub(size_after) as i64,
        compacted,
    })
}

/// 连接自身知道打开的库文件路径（`PRAGMA database_list`），比从配置再传一条路径可靠：
/// 清库动作拿到的连接池和设置页显示的路径本来就是同一个文件。
fn resolved_db_path_of(conn: &rusqlite::Connection) -> std::path::PathBuf {
    conn.query_row("SELECT file FROM pragma_database_list WHERE name = 'main'", [], |r| r.get::<_, String>(0))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// 一次尝试落一行（重试与 fallback 各一行）。轨迹与统计分开写：统计只在最终结果落一行避免重复计数，
/// 轨迹则要保留每次尝试，否则"依次试过哪些后端"无从回答。
pub fn record_attempt(pool: &DbPool, attempt: &RequestAttempt) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO request_attempts (request_id, attempt_no, backend_id, platform_id, model_id, status_code, error_type, error_message, latency_ms, started_at, forward_url, nonce)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        rusqlite::params![
            attempt.request_id, attempt.attempt_no, attempt.backend_id, attempt.platform_id,
            attempt.model_id, attempt.status_code, attempt.error_type, attempt.error_message,
            attempt.latency_ms, attempt.started_at, attempt.forward_url, attempt.nonce
        ],
    )?;
    Ok(())
}

/// 一批统计在**一个事务**里落库：五张表各一次 prepare，逐行 execute。
/// 曾经逐行 `record()` 每次都 `pool.get()` + 隐式事务（WAL 下每条一次 fsync），
/// N 并发时 SQLite 单写锁被 N 次提交打满，慢下来还会静默丢数据。
/// 调用方（StatsWriter）已经把整批数据搬到 blocking 线程池，这里是纯同步 SQL。
pub fn record_batch(
    pool: &DbPool,
    stats: &[RequestStat],
    attempts: &[RequestAttempt],
    bodies: &[RequestBody],
    headers: &[crate::models::stats::RequestHeaders],
    responses: &[ResponseBody],
) -> AppResult<()> {
    let mut conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, error_type, created_at, ttft_ms, cache_read_tokens, cache_write_tokens, is_stream, api_key_id, cost, cache_savings, request_id, replayed_from, path, credits) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        )?;
        for s in stats {
            stmt.execute(rusqlite::params![s.proxy_id, s.route_id, s.backend_id, s.status_code,
                s.latency_ms, s.token_input, s.token_output, s.error_type, s.created_at,
                s.ttft_ms, s.cache_read_tokens, s.cache_write_tokens, s.is_stream as i64,
                s.api_key_id, s.cost, s.cache_savings, s.request_id, s.replayed_from,
                s.path, s.credits])?;
        }
    }
    // 预聚合小时表：与明细同一事务双写（详见 V41 迁移注释；读端暂未切换，
    // 表只增数据不读——两阶段里第一阶段先保证数据正确可查）。
    // 桶按服务器本地时区（与 model_timeseries hour 粒度和热度图口径一致）。
    upsert_hourly_aggregate(&tx, stats)?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO request_attempts (request_id, attempt_no, backend_id, platform_id, model_id, status_code, error_type, error_message, latency_ms, started_at, forward_url, nonce)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        )?;
        for a in attempts {
            stmt.execute(rusqlite::params![
                a.request_id, a.attempt_no, a.backend_id, a.platform_id,
                a.model_id, a.status_code, a.error_type, a.error_message,
                a.latency_ms, a.started_at, a.forward_url, a.nonce
            ])?;
        }
    }
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO request_bodies (request_id, protocol, body, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        )?;
        for b in bodies {
            stmt.execute(rusqlite::params![b.request_id, b.protocol, b.body, b.bytes, b.truncated as i64, b.created_at])?;
        }
    }
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO request_headers (request_id, headers, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5)",
        )?;
        for h in headers {
            stmt.execute(rusqlite::params![h.request_id, h.headers, h.bytes, h.truncated as i64, h.created_at])?;
        }
    }
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO response_bodies (request_id, protocol, status_code, content_type, body, bytes, truncated, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        for r in responses {
            stmt.execute(rusqlite::params![r.request_id, r.protocol, r.status_code, r.content_type,
                r.body, r.bytes, r.truncated as i64, r.created_at])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 把一批明细累加进 `request_stats_hourly`（与明细同一事务，失败即整体回滚）。
/// 每行一个 UPSERT：同桶同后端只一行，`requests` 等计数累加。
/// cost 用 SUM 语义：未定价（None）不加进 cost_sum，但请求本身仍计数（priced_requests 单独记）。
fn upsert_hourly_aggregate(tx: &rusqlite::Transaction, stats: &[RequestStat]) -> AppResult<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO request_stats_hourly
             (bucket, proxy_id, route_id, backend_id,
              requests, failed_requests, stream_requests,
              token_input, token_output, cache_read_tokens, cache_write_tokens,
              latency_ms_sum, ttft_ms_sum, ttft_samples,
              cost_sum, priced_requests, cache_savings_sum, cache_savings_samples)
         VALUES (?1,?2,?3,?4, ?5,?6,?7, ?8,?9,?10,?11, ?12,?13,?14, ?15,?16,?17,?18)
         ON CONFLICT(bucket, proxy_id, route_id, backend_id) DO UPDATE SET
             requests          = requests + excluded.requests,
             failed_requests   = failed_requests + excluded.failed_requests,
             stream_requests   = stream_requests + excluded.stream_requests,
             token_input       = token_input + excluded.token_input,
             token_output      = token_output + excluded.token_output,
             cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
             cache_write_tokens = cache_write_tokens + excluded.cache_write_tokens,
             latency_ms_sum    = latency_ms_sum + excluded.latency_ms_sum,
             ttft_ms_sum       = ttft_ms_sum + excluded.ttft_ms_sum,
             ttft_samples      = ttft_samples + excluded.ttft_samples,
             cost_sum          = CASE WHEN excluded.cost_sum IS NULL
                                      THEN cost_sum ELSE COALESCE(cost_sum, 0) + excluded.cost_sum END,
             priced_requests   = priced_requests + excluded.priced_requests,
             cache_savings_sum = cache_savings_sum + excluded.cache_savings_sum,
             cache_savings_samples = cache_savings_samples + excluded.cache_savings_samples",
    )?;
    for s in stats {
        // 桶：RFC3339 UTC → 服务器本地时区，取整小时（与 model_timeseries 的 datetime(...,'localtime') 同口径）
        let bucket = match chrono::DateTime::parse_from_rfc3339(&s.created_at) {
            Ok(dt) => dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H").to_string(),
            // 格式非法的旧行：进不了任何桶直接跳过，不拖累整批写入
            Err(_) => continue,
        };
        let failed = if s.status_code >= 200 && s.status_code < 300 { 0i64 } else { 1i64 };
        stmt.execute(rusqlite::params![
            bucket, s.proxy_id, s.route_id, s.backend_id,
            1i64, failed, s.is_stream as i64,
            s.token_input.unwrap_or(0), s.token_output.unwrap_or(0),
            s.cache_read_tokens.unwrap_or(0), s.cache_write_tokens.unwrap_or(0),
            s.latency_ms, s.ttft_ms.unwrap_or(0),
            if s.ttft_ms.is_some() { 1i64 } else { 0i64 },
            s.cost, if s.cost.is_some() { 1i64 } else { 0i64 },
            s.cache_savings.unwrap_or(0.0),
            if s.cache_savings.is_some() { 1i64 } else { 0i64 },
        ])?;
    }
    Ok(())
}

const ATTEMPT_COLS: &str = "a.id, a.request_id, a.attempt_no, a.backend_id, a.platform_id, a.model_id, a.status_code, a.error_type, a.error_message, a.latency_ms, a.started_at, a.forward_url, p.name, a.nonce";
const ATTEMPT_FROM: &str = "FROM request_attempts a LEFT JOIN platforms p ON p.id = a.platform_id";

fn map_attempt(r: &rusqlite::Row) -> rusqlite::Result<RequestAttempt> {
    Ok(RequestAttempt {
        id: r.get(0)?,
        request_id: r.get(1)?,
        attempt_no: r.get(2)?,
        backend_id: r.get(3)?,
        platform_id: r.get(4)?,
        model_id: r.get(5)?,
        status_code: r.get(6)?,
        error_type: r.get(7)?,
        error_message: r.get(8)?,
        latency_ms: r.get(9)?,
        started_at: r.get(10)?,
        forward_url: r.get(11)?,
        // 平台已删除时 JOIN 不到名字，前端退回显示 ID
        platform_name: r.get(12)?,
        nonce: r.get(13)?,
    })
}

/// 某条明细行的重试轨迹。`request_stats.id` → `request_id` → 按 attempt_no 升序的尝试列表。
/// V16 之前的行没有 request_id：返回空列表而不是报错，让前端能说清"轨迹不可用"。
pub fn request_trace(pool: &DbPool, stat_id: i64) -> AppResult<RequestTrace> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let request_id: Option<String> = match conn.query_row(
        "SELECT request_id FROM request_stats WHERE id = ?1",
        rusqlite::params![stat_id],
        |r| r.get::<_, Option<String>>(0),
    ) {
        Ok(v) => v,
        // 明细行本身不存在 → 404；存在但 request_id 为 NULL（V16 之前的行）→ 空轨迹
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(crate::error::AppError::NotFound("Request not found".to_string()))
        }
        Err(e) => return Err(e.into()),
    };

    let attempts = match request_id.as_deref() {
        Some(rid) => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {} {} WHERE a.request_id = ?1 ORDER BY a.attempt_no ASC",
                ATTEMPT_COLS, ATTEMPT_FROM
            ))?;
            let rows = stmt.query_map(rusqlite::params![rid], map_attempt)?
                .filter_map(|x| x.ok())
                .collect::<Vec<RequestAttempt>>();
            rows
        }
        None => Vec::new(),
    };

    Ok(RequestTrace {
        request_stat_id: stat_id,
        request_id,
        attempts,
    })
}

/// 密钥维度用量：时间范围内每个 API Key 聚合出一行，供密钥列表直接展示，避免逐 Key 查询。
pub fn api_key_usage(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ApiKeyUsage>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let sql = format!(
        r#"
        SELECT
            api_key_id,
            COUNT(*)                                                                            AS requests,
            SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END)          AS success_requests,
            SUM(CASE WHEN NOT (status_code >= 200 AND status_code < 300) THEN 1 ELSE 0 END)    AS failed_requests,
            COALESCE(SUM(token_input), 0)                                                       AS token_input,
            COALESCE(SUM(token_output), 0)                                                      AS token_output,
            COALESCE(AVG(latency_ms), 0)                                                        AS avg_latency_ms,
            MAX(created_at)                                                                     AS last_used,
            SUM(cost)                                                                           AS total_cost,
            COUNT(cost)                                                                         AS priced_requests
        FROM request_stats
        WHERE api_key_id IS NOT NULL{}
        GROUP BY api_key_id
        "#,
        range.and("created_at")
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(range.params()), |r| {
            Ok(ApiKeyUsage {
                api_key_id: r.get(0)?,
                requests: r.get(1)?,
                success_requests: r.get(2)?,
                failed_requests: r.get(3)?,
                token_input: r.get(4)?,
                token_output: r.get(5)?,
                avg_latency_ms: r.get(6)?,
                last_used: r.get(7)?,
                // SUM(cost) 在全 NULL 时返回 NULL：与"一条都没定价"同义，前端显示 —
                total_cost: r.get(8)?,
                priced_requests: r.get(9)?,
            })
        })?
        .filter_map(|x| x.ok())
        .collect();

    Ok(rows)
}

/// 单个密钥的虚拟模型维度拆分（按请求数倒序）
pub fn api_key_model_usage(pool: &DbPool, api_key_id: &str, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ApiKeyModelUsage>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let sql = format!(
        r#"
        SELECT
            COALESCE(px.name, 'unknown')                                                        AS model_id,
            COUNT(*)                                                                            AS requests,
            SUM(CASE WHEN NOT (rs.status_code >= 200 AND rs.status_code < 300) THEN 1 ELSE 0 END) AS failed_requests,
            COALESCE(SUM(rs.token_input), 0)                                                    AS token_input,
            COALESCE(SUM(rs.token_output), 0)                                                   AS token_output,
            COALESCE(AVG(rs.latency_ms), 0)                                                     AS avg_latency_ms,
            MAX(rs.created_at)                                                                  AS last_used,
            SUM(rs.cost)                                                                        AS total_cost,
            COUNT(rs.cost)                                                                      AS priced_requests
        FROM request_stats rs
        LEFT JOIN proxies px ON rs.proxy_id = px.id
        WHERE rs.api_key_id = ?{}
        GROUP BY model_id
        ORDER BY requests DESC
        "#,
        range.and("rs.created_at")
    );

    let mut params: Vec<String> = vec![api_key_id.to_string()];
    params.extend(range.params().into_iter().map(|s| s.to_string()));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok(ApiKeyModelUsage {
                model_id: r.get(0)?,
                requests: r.get(1)?,
                failed_requests: r.get(2)?,
                token_input: r.get(3)?,
                token_output: r.get(4)?,
                avg_latency_ms: r.get(5)?,
                last_used: r.get(6)?,
                total_cost: r.get(7)?,
                priced_requests: r.get(8)?,
            })
        })?
        .filter_map(|x| x.ok())
        .collect();

    Ok(rows)
}

/// 模型维度汇总：时间范围内每个模型的全量统计（口径与 model_timeseries 一致，只是不按周期切分）。
/// 供概览页「模型性能矩阵」表格使用：一屏看尽 模型 × 请求/失败/token/延时/首字/速度。
pub fn model_summaries(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ModelSummary>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // D 读端第二阶段：hourly 三段 UNION；维度 = model_id（经 backends 关联）。
    // hourly 段与 detail 段用同一表达式（两侧都 JOIN backends，别名分别为 h/rs）
    let rows = query_dim_stats(
        &conn,
        range.from.as_deref(),
        range.to.as_deref(),
        "COALESCE(b.model_id, 'unknown')",
        "COALESCE(b.model_id, 'unknown')",
        "LEFT JOIN backends b ON b.id = h.backend_id",
        "LEFT JOIN backends b ON b.id = rs.backend_id",
        None,
    );

    let mut summaries: Vec<ModelSummary> = rows
        .into_iter()
        .map(|(model_id, row)| {
            let fresh_input = (row.token_input - row.cache_read_tokens - row.cache_write_tokens).max(0);
            ModelSummary {
                model_id,
                requests: row.requests,
                success_requests: row.success,
                failed_requests: row.requests - row.success,
                stream_requests: row.stream_requests,
                token_input: row.token_input,
                token_output: row.token_output,
                avg_latency_ms: row.avg_latency_ms,
                avg_ttft_ms: row.avg_ttft_ms,
                output_speed_tps: row.output_speed_tps,
                cache_read_tokens: row.cache_read_tokens,
                fresh_input_tokens: fresh_input,
                cache_hit_rate: {
                    let denominator = row.cache_read_tokens + fresh_input;
                    if denominator > 0 { row.cache_read_tokens as f64 / denominator as f64 * 100.0 } else { 0.0 }
                },
                total_cost: row.total_cost,
                cache_savings: row.cache_savings,
                priced_requests: row.priced_requests,
            }
        })
        .collect();

    // 排序：旧 SQL 按 requests DESC
    summaries.sort_by(|a, b| b.requests.cmp(&a.requests));
    Ok(summaries)
}

/// 追加一个"列 = ?"精确条件；值为 None 或空串时不追加。
fn add_exact(sql: &mut String, params: &mut Vec<String>, col: &str, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else { return };
    sql.push_str(&format!(" AND {col} = ?"));
    params.push(value.to_string());
}

/// 请求明细的列、表与排序：列表和导出共用一份，导出不会漏列也不会换序。
/// 尝试次数用相关子查询而不是 JOIN + GROUP BY：明细行与 `request_attempts` 是一对多，
/// JOIN 会把一条请求放大成多行，而分页的 LIMIT 数的是行。走 `idx_request_attempts_request`
/// 每行一次索引查找，一页最多 200 行，代价可控。
/// 别名取 `attempt_count`：`requests_order` 直接按这个别名排，不必把子查询再抄一遍。
const REQUESTS_SELECT: &str = r#"
SELECT
    rs.id, rs.created_at,
    COALESCE(b.model_id, 'unknown'),
    COALESCE(p.name, 'unknown'),
    COALESCE(px.name, 'unknown'),
    rs.status_code, rs.token_input, rs.token_output, rs.latency_ms, rs.ttft_ms,
    CASE WHEN rs.token_output > 0 AND rs.latency_ms - COALESCE(rs.ttft_ms, 0) > 0
         THEN rs.token_output * 1000.0 / (rs.latency_ms - COALESCE(rs.ttft_ms, 0))
         ELSE NULL END,
    rs.cache_read_tokens, rs.cache_write_tokens,
    CASE WHEN rs.token_input IS NOT NULL
         THEN MAX(rs.token_input - COALESCE(rs.cache_read_tokens, 0) - COALESCE(rs.cache_write_tokens, 0), 0)
         ELSE NULL END AS fresh_input,
    rs.is_stream, rs.error_type, ak.name, rs.cost, rs.request_id,
    CASE WHEN rs.request_id IS NULL THEN NULL
         ELSE (SELECT COUNT(*) FROM request_attempts ta WHERE ta.request_id = rs.request_id)
    END AS attempt_count,
    rs.replayed_from,
    -- 正文存没存要在这里就答出来，重放按钮才能一开始就摆对状态。用 EXISTS 而不是 JOIN：
    -- 正文表按 request_id 一对一，JOIN 不会放大行数但会把 TEXT 列拖进结果集，白读一遍。
    CASE WHEN rs.request_id IS NULL THEN 0
         ELSE EXISTS (SELECT 1 FROM request_bodies tb WHERE tb.request_id = rs.request_id AND tb.truncated = 0)
    END AS has_body,
    rs.path, rs.credits
"#;
const REQUESTS_FROM: &str = "FROM request_stats rs
    LEFT JOIN backends b ON rs.backend_id = b.id
    LEFT JOIN platforms p ON b.platform_id = p.id
    LEFT JOIN proxies px ON rs.proxy_id = px.id
    LEFT JOIN api_keys ak ON rs.api_key_id = ak.id";
/// 倒序取行：created_at 相同（同秒批量写入）时按 id 兜底
const REQUESTS_ORDER: &str = "ORDER BY rs.created_at DESC, rs.id DESC";

/// 排序列白名单：查询串里的 `sort` 只能取这些前端列 id，映射到写死的 SQL 表达式。
/// ORDER BY 不能用占位符参数化，所以防注入只能靠"不在表里就直接回落默认"。
/// 空值一律 `NULLS LAST`：没填延迟、没定价的行在两个方向都沉底，不然升序时它们会顶在最前面。
fn requests_order(filter: &RequestFilter) -> String {
    let expr = match filter.sort.as_deref() {
        Some("time") => "rs.created_at",
        Some("latency") => "rs.latency_ms",
        Some("ttft") => "rs.ttft_ms",
        Some("tokens") => "(COALESCE(rs.token_input, 0) + COALESCE(rs.token_output, 0))",
        Some("cost") => "rs.cost",
        Some("credits") => "rs.credits",
        Some("speed") => "(CASE WHEN rs.token_output > 0 AND rs.latency_ms - COALESCE(rs.ttft_ms, 0) > 0 \
             THEN rs.token_output * 1000.0 / (rs.latency_ms - COALESCE(rs.ttft_ms, 0)) ELSE NULL END)",
        Some("model") => "COALESCE(b.model_id, 'unknown')",
        Some("platform") => "COALESCE(p.name, 'unknown')",
        Some("proxy") => "COALESCE(px.name, 'unknown')",
        Some("apiKey") => "ak.name",
        Some("status") => "rs.status_code",
        Some("error") => "rs.error_type",
        Some("cacheRead") => "rs.cache_read_tokens",
        Some("cacheWrite") => "rs.cache_write_tokens",
        Some("freshInput") => "(CASE WHEN rs.token_input IS NOT NULL \
             THEN MAX(rs.token_input - COALESCE(rs.cache_read_tokens, 0) - COALESCE(rs.cache_write_tokens, 0), 0) \
             ELSE NULL END)",
        Some("stream") => "rs.is_stream",
        // 尝试次数是 `REQUESTS_SELECT` 里的相关子查询，按它的输出别名排——把子查询再抄一遍
        // 就成了两处定义，改一处就会让"看到的次数"和"排序用的次数"不一致
        Some("attempts") => "attempt_count",
        // 未知列名（包括手改过的查询串）不猜意图，回到默认顺序
        _ => return REQUESTS_ORDER.to_string(),
    };
    let dir = if filter.dir.as_deref() == Some("asc") { "ASC" } else { "DESC" };
    // 再补一条 id 兜底：同一延迟值/同一名称的行在翻页时不能跳页
    format!("ORDER BY {expr} {dir} NULLS LAST, rs.id DESC")
}

/// `RequestFilter` → WHERE 片段 + 绑定参数。
/// 列表、汇总与导出共用这一份：筛选口径只有一处定义，不会出现"页面上筛过、导出里没筛"。
fn request_where(filter: &RequestFilter) -> AppResult<(String, Vec<String>)> {
    let range = TimeRange::parse(filter.from.as_deref(), filter.to.as_deref())?;
    let mut where_sql = String::from(" WHERE 1=1");
    where_sql.push_str(&range.and("rs.created_at"));
    let mut params: Vec<String> = range.params().into_iter().map(|s| s.to_string()).collect();
    add_exact(&mut where_sql, &mut params, "COALESCE(b.model_id, 'unknown')", filter.model.as_deref());
    add_exact(&mut where_sql, &mut params, "b.platform_id", filter.platform.as_deref());
    add_exact(&mut where_sql, &mut params, "rs.proxy_id", filter.proxy.as_deref());
    add_exact(&mut where_sql, &mut params, "rs.api_key_id", filter.api_key.as_deref());
    add_exact(&mut where_sql, &mut params, "rs.error_type", filter.error_type.as_deref());
    // 延迟区间：用于"只看慢请求"这类排查，边界值含入
    if let Some(min) = filter.latency_min.filter(|value| *value >= 0) {
        where_sql.push_str(" AND rs.latency_ms >= ?");
        params.push(min.to_string());
    }
    if let Some(max) = filter.latency_max.filter(|value| *value >= 0) {
        where_sql.push_str(" AND rs.latency_ms <= ?");
        params.push(max.to_string());
    }
    match filter.status.as_deref() {
        Some("success") => where_sql.push_str(" AND rs.status_code >= 200 AND rs.status_code < 300"),
        Some("fail") => where_sql.push_str(" AND NOT (rs.status_code >= 200 AND rs.status_code < 300)"),
        _ => {}
    }
    if let Some(q) = filter.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // 转义 LIKE 的通配符，避免用户输入的 % / _ 变成模式匹配
        let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        where_sql.push_str(
            " AND (b.model_id LIKE ? ESCAPE '\\' OR p.name LIKE ? ESCAPE '\\'\
             OR px.name LIKE ? ESCAPE '\\' OR ak.name LIKE ? ESCAPE '\\')",
        );
        for _ in 0..4 {
            params.push(pattern.clone());
        }
    }
    Ok((where_sql, params))
}

/// 明细行映射；列顺序必须与 `REQUESTS_SELECT` 一致。
fn request_row(r: &rusqlite::Row) -> rusqlite::Result<RequestLogItem> {
    Ok(RequestLogItem {
        id: r.get(0)?,
        created_at: r.get(1)?,
        model_id: r.get(2)?,
        platform_name: r.get(3)?,
        proxy_name: r.get(4)?,
        status_code: r.get(5)?,
        token_input: r.get(6)?,
        token_output: r.get(7)?,
        latency_ms: r.get(8)?,
        ttft_ms: r.get(9)?,
        output_speed_tps: r.get(10)?,
        cache_read_tokens: r.get(11)?,
        cache_write_tokens: r.get(12)?,
        fresh_input_tokens: r.get(13)?,
        is_stream: r.get::<_, i64>(14)? != 0,
        error_type: r.get(15)?,
        api_key_name: r.get(16)?,
        cost: r.get(17)?,
        request_id: r.get(18)?,
        attempts: r.get(19)?,
        replayed_from: r.get(20)?,
        has_body: r.get::<_, i64>(21)? != 0,
        path: r.get(22)?,
        credits: r.get(23)?,
    })
}

/// 请求明细列表（倒序分页）：概览大屏底部日志表数据源。
/// output_speed_tps 由 SQL 表达式逐行计算：token_output / (latency − ttft)，无效时为 NULL。
/// COUNT 与取页共用同一份 WHERE，新增筛选只需要加字段，不会两边跑偏。
pub fn list_requests(pool: &DbPool, filter: &RequestFilter) -> AppResult<RequestLogPage> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let (where_sql, params) = request_where(filter)?;
    let page = filter.page.unwrap_or(1).max(1);
    let page_size = filter.page_size.unwrap_or(50).clamp(1, 200);

    // 成本/积分汇总与总数共用同一份 WHERE：筛选一变，覆盖率跟着变（这里统计的是整个筛选集，不止当前页）
    let (total, total_cost, priced_requests, total_credits): (i64, Option<f64>, i64, Option<f64>) = conn
        .query_row(
            &format!("SELECT COUNT(*), SUM(rs.cost), COUNT(rs.cost), SUM(rs.credits) {}{}", REQUESTS_FROM, where_sql),
            rusqlite::params_from_iter(params.iter()),
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;

    let mut page_params = params;
    page_params.push(page_size.to_string());
    page_params.push(((page - 1) * page_size).to_string());
    let sql = format!(
        "{} {} {} {} LIMIT ? OFFSET ?",
        REQUESTS_SELECT, REQUESTS_FROM, where_sql, requests_order(filter)
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(rusqlite::params_from_iter(page_params.iter()), request_row)?
        .filter_map(|x| x.ok())
        .collect();

    Ok(RequestLogPage {
        total,
        page,
        page_size,
        items,
        total_cost,
        priced_requests,
        total_credits,
    })
}

/// 导出默认行数：够看一段时间，又不至于把整张统计表拖进浏览器。
pub const EXPORT_DEFAULT_ROWS: i64 = 5_000;
/// 导出硬上限：再大就该用数据库备份，不是 CSV。
pub const EXPORT_MAX_ROWS: i64 = 20_000;

/// 按筛选导出请求明细，返回（行, 命中总数）。
/// 总数单独查一次，好让界面能说"命中 12,345 条、只给了前 5,000 条"，而不是静默截断。
pub fn export_requests(
    pool: &DbPool,
    filter: &RequestFilter,
    limit: i64,
) -> AppResult<(Vec<RequestLogItem>, i64)> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let (where_sql, params) = request_where(filter)?;
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) {}{}", REQUESTS_FROM, where_sql),
        rusqlite::params_from_iter(params.iter()),
        |r| r.get(0),
    )?;

    let mut row_params = params;
    row_params.push(limit.to_string());
    // 导出固定按时间倒序，不跟屏幕上的排序走：同一份筛选两次导出必须拿到同一批同一序，
    // 否则两份报表没法逐行比对。列集合与顺序都是稳定的，排序只是看的时候的事。
    let sql = format!("{} {} {} {} LIMIT ?", REQUESTS_SELECT, REQUESTS_FROM, where_sql, REQUESTS_ORDER);
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(rusqlite::params_from_iter(row_params.iter()), request_row)?
        .filter_map(|x| x.ok())
        .collect();
    Ok((items, total))
}

/// 删除指定 proxy 的所有请求统计（连带它的尝试轨迹，否则 request_attempts 只增不减）
pub fn delete_by_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "DELETE FROM request_attempts WHERE request_id IN
         (SELECT request_id FROM request_stats WHERE proxy_id = ?1 AND request_id IS NOT NULL)",
        rusqlite::params![proxy_id],
    )?;
    conn.execute(
        "DELETE FROM request_bodies WHERE request_id IN
         (SELECT request_id FROM request_stats WHERE proxy_id = ?1 AND request_id IS NOT NULL)",
        rusqlite::params![proxy_id],
    )?;
    conn.execute(
        "DELETE FROM request_headers WHERE request_id IN
         (SELECT request_id FROM request_stats WHERE proxy_id = ?1 AND request_id IS NOT NULL)",
        rusqlite::params![proxy_id],
    )?;
    conn.execute(
        "DELETE FROM response_bodies WHERE request_id IN
         (SELECT request_id FROM request_stats WHERE proxy_id = ?1 AND request_id IS NOT NULL)",
        rusqlite::params![proxy_id],
    )?;
    conn.execute("DELETE FROM request_stats WHERE proxy_id = ?1", rusqlite::params![proxy_id])?;
    Ok(())
}

/// 删除确认要展示的影响面：与 `delete_by_proxy` 删的行完全同口径，
/// 所以屏幕上的数字和删完少掉的数字不可能对不上。
pub fn proxy_history_impact(pool: &DbPool, proxy_id: &str) -> AppResult<ProxyHistoryImpact> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let requests: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1",
        rusqlite::params![proxy_id],
        |r| r.get(0),
    )?;
    let attempts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_attempts WHERE request_id IN
         (SELECT request_id FROM request_stats WHERE proxy_id = ?1 AND request_id IS NOT NULL)",
        rusqlite::params![proxy_id],
        |r| r.get(0),
    )?;
    Ok(ProxyHistoryImpact { requests, attempts })
}

/// backend 被删除时，将历史统计中的 backend_id 置空（悬挂），保留请求数与 token 统计。
/// 悬挂行在模型/平台聚合中归入 'unknown'，避免口径漂移。
pub fn orphan_backend_stats(pool: &DbPool, backend_id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE request_stats SET backend_id = NULL WHERE backend_id = ?1",
        rusqlite::params![backend_id],
    )?;
    Ok(())
}

/// 悬挂统计清理结果
#[derive(serde::Serialize)]
pub struct StatsCleanupResult {
    /// 被删除的悬挂统计行数
    pub deleted: i64,
    /// 顺带清掉的孤儿轨迹行数（对应明细行已不存在）
    pub orphan_attempts: i64,
    /// 顺带清掉的孤儿正文行数（正文采集开着时才会大于 0）
    pub orphan_bodies: i64,
    /// 顺带清掉的孤儿请求头行数
    pub orphan_headers: i64,
    /// 顺带清掉的孤儿响应正文行数
    pub orphan_responses: i64,
    /// 只是预演：数字是"照这个条件会删掉多少"，库里一行没动
    pub dry_run: bool,
}

/// 清理悬挂的历史统计行：删除 request_stats 中指向已不存在 proxy / route / backend 的行。
/// 悬挂行的 model_id / platform 聚合会显示为 'unknown'，清理后大屏数据口径恢复干净。
/// `dry_run` 只数不删——确认弹窗要先报出行数，"不可恢复"才不是空口说。
pub fn cleanup_dangling(pool: &DbPool, dry_run: bool) -> AppResult<StatsCleanupResult> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // 五条 DELETE 收进同一事务：中途失败不会留下「明细删了、正文还在」的不一致状态，
    // 也让 SQLite 只取一次写锁（逐条 autocommit 时每条都持锁一段时间）。
    let tx = if dry_run { None } else { Some(conn.unchecked_transaction()?) };
    let conn: &rusqlite::Connection = match &tx {
        Some(t) => t,
        None => &conn,
    };

    // proxy_id / route_id / backend_id 任一指向不存在的记录，即视为悬挂行
    const DANGLING: &str = r#"
        proxy_id NOT IN (SELECT id FROM proxies)
           OR route_id NOT IN (SELECT id FROM routes)
           OR backend_id NOT IN (SELECT id FROM backends)"#;
    let deleted: i64 = if dry_run {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM request_stats WHERE {}", DANGLING),
            [],
            |row| row.get(0),
        )?
    } else {
        conn.execute(
            &format!("DELETE FROM request_stats WHERE {}", DANGLING),
            [],
        )?
        .try_into()
        .unwrap_or(0)
    };

    // 明细行没了，轨迹行就是纯孤儿；一并扫掉，防止 request_attempts 单向增长。
    // 预演时按"删完悬挂行之后"的口径数：那些行此刻还在，但真要执行就会连带删掉它们。
    let orphan_attempts: i64 = if dry_run {
        conn.query_row(
            r#"
            SELECT COUNT(*) FROM request_attempts
            WHERE request_id NOT IN (
                SELECT request_id FROM request_stats
                WHERE request_id IS NOT NULL
                  AND NOT (proxy_id NOT IN (SELECT id FROM proxies)
                        OR route_id NOT IN (SELECT id FROM routes)
                        OR backend_id NOT IN (SELECT id FROM backends))
            )"#,
            [],
            |row| row.get(0),
        )?
    } else {
        conn.execute(
            "DELETE FROM request_attempts
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
        )?
        .try_into()
        .unwrap_or(0)
    };

    // 正文同理：明细行没了，正文既没法重放也不该继续占空间（它还是最敏感的那部分）。
    let orphan_bodies: i64 = if dry_run {
        conn.query_row(
            "SELECT COUNT(*) FROM request_bodies
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
            |row| row.get(0),
        )?
    } else {
        conn.execute(
            "DELETE FROM request_bodies
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
        )?
        .try_into()
        .unwrap_or(0)
    };

    let orphan_headers: i64 = if dry_run {
        conn.query_row(
            "SELECT COUNT(*) FROM request_headers
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
            |row| row.get(0),
        )?
    } else {
        conn.execute(
            "DELETE FROM request_headers
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
        )?
        .try_into()
        .unwrap_or(0)
    };

    let orphan_responses: i64 = if dry_run {
        conn.query_row(
            "SELECT COUNT(*) FROM response_bodies
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
            |row| row.get(0),
        )?
    } else {
        conn.execute(
            "DELETE FROM response_bodies
             WHERE request_id NOT IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL)",
            [],
        )?
        .try_into()
        .unwrap_or(0)
    };

    if let Some(tx) = tx {
        tx.commit()?;
    }

    if !dry_run {
        tracing::info!(
            "Stats cleanup: removed {} dangling request_stats row(s), {} orphan attempt row(s), {} orphan body row(s), {} header row(s), {} response row(s)",
            deleted,
            orphan_attempts,
            orphan_bodies,
            orphan_headers,
            orphan_responses
        );
    }
    Ok(StatsCleanupResult { deleted, orphan_attempts, orphan_bodies, orphan_headers, orphan_responses, dry_run })
}

/// 按年龄清理的结果：命中行数 + 连带轨迹行数 + 生效的边界
#[derive(serde::Serialize)]
pub struct StatsPurgeResult {
    pub rows: i64,
    pub attempts: i64,
    /// 连带删掉的正文行数（正文采集开着时才会大于 0）
    pub bodies: i64,
    /// 连带删掉的请求头行数
    pub headers: i64,
    /// 连带删掉的响应正文行数
    pub responses: i64,
    /// 早于此时间（含）的行会被删，RFC3339
    pub cutoff: String,
    pub days: i64,
    pub dry_run: bool,
}

/// 删掉 `days` 天之前的请求明细与轨迹。刻意没有定时器：自动删历史数据需要用户逐次点头，
/// 所以这里只提供"预演 → 确认 → 执行"，把删多少先说清楚。
pub fn purge_older_than(pool: &DbPool, days: i64, dry_run: bool) -> AppResult<StatsPurgeResult> {
    if days < 1 {
        return Err(crate::error::AppError::BadRequest(
            "older_than_days must be at least 1".to_string(),
        ));
    }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    // 与 normalize_bound 同一格式：定宽前缀 + 同一偏移，字符串比较即时间比较
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

    // 五条 DELETE 放进同一个事务：既避免中途失败留下「明细删了、正文没删」的
    // 不一致状态，也让 SQLite 只取一次写锁。逐条 autocommit 时，每条大 DELETE
    // 都会持写锁一段时间，StatsWriter 的批量写会被 busy_timeout 卡住甚至丢数据。
    let tx = conn.unchecked_transaction()?;

    // 先删轨迹再删明细：明细行一旦消失，"哪些轨迹属于它"就查不回来了。
    // 轨迹行按 `started_at` 存时间，但归属是那条明细行，所以按所属明细行的年龄判定。
    let attempts: i64 = if dry_run {
        tx.query_row(
            "SELECT COUNT(*) FROM request_attempts a
             WHERE EXISTS (SELECT 1 FROM request_stats s WHERE s.request_id = a.request_id AND s.created_at <= ?)",
            rusqlite::params![cutoff],
            |row| row.get(0),
        )?
    } else {
        tx.execute(
            "DELETE FROM request_attempts
             WHERE request_id IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL AND created_at <= ?)",
            rusqlite::params![cutoff],
        )?
        .try_into()
        .unwrap_or(0)
    };
    // 正文按所属明细行的年龄一起删，且必须排在删明细行之前——理由与轨迹一样：
    // 明细行一旦消失，"哪些正文属于这批过期请求"就查不回来了。它比轨迹更该跟着走：
    // 那是用户的提示词，明细都过期了还留着正文，保留期就形同虚设。
    let bodies: i64 = if dry_run {
        tx.query_row(
            "SELECT COUNT(*) FROM request_bodies b
             WHERE EXISTS (SELECT 1 FROM request_stats s WHERE s.request_id = b.request_id AND s.created_at <= ?)",
            rusqlite::params![cutoff],
            |row| row.get(0),
        )?
    } else {
        tx.execute(
            "DELETE FROM request_bodies
             WHERE request_id IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL AND created_at <= ?)",
            rusqlite::params![cutoff],
        )?
        .try_into()
        .unwrap_or(0)
    };
    let headers: i64 = if dry_run {
        tx.query_row(
            "SELECT COUNT(*) FROM request_headers h
             WHERE EXISTS (SELECT 1 FROM request_stats s WHERE s.request_id = h.request_id AND s.created_at <= ?)",
            rusqlite::params![cutoff],
            |row| row.get(0),
        )?
    } else {
        tx.execute(
            "DELETE FROM request_headers
             WHERE request_id IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL AND created_at <= ?)",
            rusqlite::params![cutoff],
        )?
        .try_into()
        .unwrap_or(0)
    };
    let responses: i64 = if dry_run {
        tx.query_row(
            "SELECT COUNT(*) FROM response_bodies r
             WHERE EXISTS (SELECT 1 FROM request_stats s WHERE s.request_id = r.request_id AND s.created_at <= ?)",
            rusqlite::params![cutoff],
            |row| row.get(0),
        )?
    } else {
        tx.execute(
            "DELETE FROM response_bodies
             WHERE request_id IN (SELECT request_id FROM request_stats WHERE request_id IS NOT NULL AND created_at <= ?)",
            rusqlite::params![cutoff],
        )?
        .try_into()
        .unwrap_or(0)
    };
    let rows: i64 = if dry_run {
        tx.query_row(
            "SELECT COUNT(*) FROM request_stats WHERE created_at <= ?",
            rusqlite::params![cutoff],
            |row| row.get(0),
        )?
    } else {
        tx.execute("DELETE FROM request_stats WHERE created_at <= ?", rusqlite::params![cutoff])?
            .try_into()
            .unwrap_or(0)
    };

    if !dry_run {
        tx.commit()?;
        tracing::info!(
            "Stats purge: removed {} request_stats row(s), {} attempt row(s), {} body row(s), {} header row(s), {} response row(s) older than {}",
            rows,
            attempts,
            bodies,
            headers,
            responses,
            cutoff
        );
    }
    Ok(StatsPurgeResult { rows, attempts, bodies, headers, responses, cutoff, days, dry_run })
}

/// 数据库体检：路径、体积、各表行数与明细时间跨度
#[derive(serde::Serialize)]
pub struct DbInfo {
    pub path: String,
    pub size_bytes: u64,
    pub request_rows: i64,
    pub attempt_rows: i64,
    pub alert_rows: i64,
    /// 已存正文的条数与字节数（采集关着时恒为 0）——设置页要能看出开这个开关花了多少空间
    pub body_rows: i64,
    pub body_bytes: i64,
    /// 已存请求头的条数与字节数
    pub header_rows: i64,
    pub header_bytes: i64,
    /// 已存响应正文的条数与字节数
    pub response_rows: i64,
    pub response_bytes: i64,
    /// 最早 / 最新的明细行时间，用于"这批历史有多长"
    pub oldest_request_at: Option<String>,
    pub newest_request_at: Option<String>,
    /// 指向已删除对象的悬挂行数——清理按钮要显示它有没有活可干
    pub dangling_rows: i64,
}

pub fn db_info(pool: &DbPool, path: &std::path::Path) -> AppResult<DbInfo> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let count = |sql: &str| -> AppResult<i64> {
        Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?)
    };
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(DbInfo {
        path: path.display().to_string(),
        size_bytes,
        request_rows: count("SELECT COUNT(*) FROM request_stats")?,
        attempt_rows: count("SELECT COUNT(*) FROM request_attempts")?,
        alert_rows: count("SELECT COUNT(*) FROM alerts")?,
        body_rows: count("SELECT COUNT(*) FROM request_bodies")?,
        // 用 bytes 列而不是 SUM(LENGTH(CAST(body AS BLOB)))：
        // 后者会把每张正文表的**全文**读一遍再算长度，大库上是一次重扫描；
        // bytes 列本来就是落库时记下的实际字节数，点查即可。
        body_bytes: conn.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM request_bodies",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        header_rows: count("SELECT COUNT(*) FROM request_headers")?,
        header_bytes: conn.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM request_headers",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        response_rows: count("SELECT COUNT(*) FROM response_bodies")?,
        response_bytes: conn.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM response_bodies",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        oldest_request_at: conn
            .query_row("SELECT MIN(created_at) FROM request_stats", [], |row| {
                row.get::<_, Option<String>>(0)
            })?,
        newest_request_at: conn
            .query_row("SELECT MAX(created_at) FROM request_stats", [], |row| {
                row.get::<_, Option<String>>(0)
            })?,
        dangling_rows: count(
            r#"SELECT COUNT(*) FROM request_stats
               WHERE proxy_id NOT IN (SELECT id FROM proxies)
                  OR route_id NOT IN (SELECT id FROM routes)
                  OR backend_id NOT IN (SELECT id FROM backends)"#,
        )?,
    })
}

pub fn overview(
    pool: &DbPool,
    total_connections_active: i64,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<OverviewStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // 主体统计：全部带 [from, to) 过滤；D 第二阶段起整小时部分走 request_stats_hourly，
    // 两个边界小时回退明细，UNION ALL 后口径与全程查明细一致（见 query_metrics）。
    let m = query_metrics(&conn, range.from.as_deref(), range.to.as_deref());
    let total_requests = m.requests;
    let success_count = m.success;
    let avg_latency = m.avg_latency_ms;
    let total_token_input = m.token_input;
    let total_token_output = m.token_output;
    let (avg_ttft, avg_speed, total_cache_read, total_cache_write, total_cost, total_cache_savings, priced_requests) =
        (m.avg_ttft_ms, m.avg_speed_tps, m.cache_read_tokens, m.cache_write_tokens, m.cost, m.cache_savings, m.priced_requests);

    // 对比周期：指定范围时对比等长的前一段区间；未指定时保持昨日口径
    match prev_range(&range) {
        Some(prev) => {
            let prev_m = query_metrics(&conn, prev.from.as_deref(), prev.to.as_deref());
            Ok(build_overview(
                &conn, total_requests, success_count, avg_latency,
                total_token_input, total_token_output,
                total_connections_active, prev_m.requests, prev_m.avg_latency_ms,
                avg_ttft, avg_speed, total_cache_read, total_cache_write,
                total_cost, total_cache_savings, priced_requests,
            ))
        }
        None => {
            // 昨日口径：与 proxy_stats 的 date('now') 保持一致
            let requests_prev_period: i64 = conn.query_row(
                "SELECT COUNT(*) FROM request_stats WHERE date(created_at) = date('now', '-1 day')",
                [], |r| r.get(0),
            ).unwrap_or(0);
            let avg_latency_prev_period: f64 = conn.query_row(
                "SELECT COALESCE(AVG(latency_ms),0) FROM request_stats WHERE date(created_at) = date('now', '-1 day')",
                [], |r| r.get(0),
            ).unwrap_or(0.0);
            Ok(build_overview(
                &conn, total_requests, success_count, avg_latency,
                total_token_input, total_token_output,
                total_connections_active, requests_prev_period, avg_latency_prev_period,
                avg_ttft, avg_speed, total_cache_read, total_cache_write,
                total_cost, total_cache_savings, priced_requests,
            ))
        }
    }
}

/// 一个时间范围内的全量聚合读数（overview 主体 + 对比周期共用）。
/// D 第二阶段：整小时桶从 `request_stats_hourly` 取，两个边界小时回退 `request_stats` 明细，
/// 三段 UNION ALL 后的结果与"全程查明细"逐字段等价（cost/cache_savings 的 NULL 语义保留：
/// 无样本数时返回 None，不是 0）。
struct Metrics {
    requests: i64,
    success: i64,
    avg_latency_ms: f64,
    token_input: i64,
    token_output: i64,
    avg_ttft_ms: f64,
    ttft_samples: i64,
    avg_speed_tps: f64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    cost: Option<f64>,
    cache_savings: Option<f64>,
    priced_requests: i64,
}

fn query_metrics(conn: &rusqlite::Connection, from: Option<&str>, to: Option<&str>) -> Metrics {
    let agg = hourly_agg_range(from, to);
    // 三段字段严格同构（列序不可乱）：
    // 0 requests / 1 failed / 2 stream / 3 token_input / 4 token_output
    // 5 cache_read / 6 cache_write / 7 latency_sum / 8 ttft_sum / 9 ttft_samples
    // 10 cost_sum(REAL NULL) / 11 priced / 12 savings_sum / 13 savings_samples
    let hourly_select = |extra: &str| -> String {
        format!(
            "SELECT requests, failed_requests, stream_requests,
                    token_input, token_output, cache_read_tokens, cache_write_tokens,
                    latency_ms_sum, ttft_ms_sum, ttft_samples,
                    cost_sum, priced_requests, cache_savings_sum, cache_savings_samples
             FROM request_stats_hourly
             WHERE 1=1{extra}"
        )
    };
    let detail_select = |extra: &str| -> String {
        format!(
            "SELECT COUNT(*) AS requests,
                    SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 0 ELSE 1 END) AS failed_requests,
                    SUM(is_stream) AS stream_requests,
                    COALESCE(SUM(token_input), 0) AS token_input, COALESCE(SUM(token_output), 0) AS token_output,
                    COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens, COALESCE(SUM(cache_write_tokens), 0) AS cache_write_tokens,
                    COALESCE(SUM(latency_ms), 0) AS latency_ms_sum, COALESCE(SUM(ttft_ms), 0) AS ttft_ms_sum, COUNT(ttft_ms) AS ttft_samples,
                    SUM(cost) AS cost_sum, COUNT(cost) AS priced_requests,
                    COALESCE(SUM(cache_savings), 0) AS cache_savings_sum, COUNT(cache_savings) AS cache_savings_samples
             FROM request_stats
             WHERE 1=1{extra}"
        )
    };
    let (sql, params): (String, Vec<String>) = if let (Some(mf), Some(mt), Some(cf), Some(ct), Some(f), Some(t)) =
        (&agg.mid_from, &agg.mid_to, &agg.core_from, &agg.core_to, &agg.from, &agg.to)
    {
        // 整小时段 [bucket=mf, mt)（hourly 表桶字符串）+ 前边界 [f, cf) + 后边界 [ct, t)
        // （明细 created_at 与 UTC 整秒比较）。参数序全局：?1=mf ?2=mt ?3=f ?4=cf ?5=ct ?6=t
        // 三段子查询必须逐列同名（UNION 取第一段列名）——外层全部按小时表列名引用。
        let sql = format!(
            "SELECT
                SUM(u.requests) AS requests,
                SUM(u.requests) - COALESCE(SUM(u.failed_requests), 0) AS success,
                CASE WHEN SUM(u.requests) > 0 THEN COALESCE(SUM(u.latency_ms_sum), 0) * 1.0 / SUM(u.requests) ELSE 0 END AS avg_latency_ms,
                COALESCE(SUM(u.token_input), 0), COALESCE(SUM(u.token_output), 0),
                COALESCE(SUM(u.cache_read_tokens), 0), COALESCE(SUM(u.cache_write_tokens), 0),
                CASE WHEN SUM(u.ttft_samples) > 0 THEN COALESCE(SUM(u.ttft_ms_sum), 0) * 1.0 / SUM(u.ttft_samples) ELSE 0 END AS avg_ttft_ms,
                COALESCE(SUM(u.ttft_samples), 0) AS ttft_samples,
                CASE WHEN SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum), 0) > 0
                     THEN COALESCE(SUM(u.token_output), 0) * 1000.0 / (SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum), 0))
                     ELSE 0 END AS avg_speed_tps,
                COALESCE(SUM(u.cache_read_tokens), 0), COALESCE(SUM(u.cache_write_tokens), 0),
                SUM(u.cost_sum) AS total_cost,
                COALESCE(SUM(u.priced_requests), 0) AS priced_requests,
                SUM(u.cache_savings_sum) AS total_savings,
                COALESCE(SUM(u.cache_savings_samples), 0) AS savings_samples
             FROM (
                {}
                UNION ALL
                {}
                UNION ALL
                {}
             ) u",
            hourly_select(" AND bucket >= ?1 AND bucket < ?2"),
            detail_select(" AND created_at >= ?3 AND created_at < ?4"),
            detail_select(" AND created_at >= ?5 AND created_at < ?6"),
        );
        (sql, vec![mf.clone(), mt.clone(), f.clone(), cf.clone(), ct.clone(), t.clone()])
    } else {
        // 开放式范围（无 from/to 任一）或区间不足一小时：全程明细，与 D 之前逐字段等价。
        // 条件按边界存在与否动态拼：无 from 则去掉 >=，无 to 则去掉 <。
        let cond = match (agg.from.as_ref(), agg.to.as_ref()) {
            (Some(_), Some(_)) => " AND created_at >= ?1 AND created_at < ?2".to_string(),
            (Some(_), None) => " AND created_at >= ?1".to_string(),
            (None, Some(_)) => " AND created_at < ?1".to_string(),
            (None, None) => String::new(),
        };
        let sql = format!(
            "SELECT
                SUM(u.requests) AS requests,
                SUM(u.requests) - COALESCE(SUM(u.failed_requests), 0) AS success,
                CASE WHEN SUM(u.requests) > 0 THEN COALESCE(SUM(u.latency_ms_sum), 0) * 1.0 / SUM(u.requests) ELSE 0 END AS avg_latency_ms,
                COALESCE(SUM(u.token_input), 0), COALESCE(SUM(u.token_output), 0),
                COALESCE(SUM(u.cache_read_tokens), 0), COALESCE(SUM(u.cache_write_tokens), 0),
                CASE WHEN SUM(u.ttft_samples) > 0 THEN COALESCE(SUM(u.ttft_ms_sum), 0) * 1.0 / SUM(u.ttft_samples) ELSE 0 END AS avg_ttft_ms,
                COALESCE(SUM(u.ttft_samples), 0) AS ttft_samples,
                CASE WHEN SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum), 0) > 0
                     THEN COALESCE(SUM(u.token_output), 0) * 1000.0 / (SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum), 0))
                     ELSE 0 END AS avg_speed_tps,
                COALESCE(SUM(u.cache_read_tokens), 0), COALESCE(SUM(u.cache_write_tokens), 0),
                SUM(u.cost_sum) AS total_cost,
                COALESCE(SUM(u.priced_requests), 0) AS priced_requests,
                SUM(u.cache_savings_sum) AS total_savings,
                COALESCE(SUM(u.cache_savings_samples), 0) AS savings_samples
             FROM (
                SELECT COUNT(*) AS requests,
                       SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 0 ELSE 1 END) AS failed_requests,
                       SUM(is_stream) AS stream_requests,
                       COALESCE(SUM(token_input), 0) AS token_input, COALESCE(SUM(token_output), 0) AS token_output,
                       COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens, COALESCE(SUM(cache_write_tokens), 0) AS cache_write_tokens,
                       COALESCE(SUM(latency_ms), 0) AS latency_ms_sum, COALESCE(SUM(ttft_ms), 0) AS ttft_ms_sum, COUNT(ttft_ms) AS ttft_samples,
                       SUM(cost) AS cost_sum, COUNT(cost) AS priced_requests,
                       COALESCE(SUM(cache_savings), 0) AS cache_savings_sum, COUNT(cache_savings) AS cache_savings_samples
                FROM request_stats
                WHERE 1=1{} 
             ) u",
            cond
        );
        let params: Vec<String> = agg.from.iter().chain(agg.to.iter()).cloned().collect();
        (sql, params)
    };

    let r = conn
        .query_row(&sql, rusqlite::params_from_iter(params.iter().map(|s| s.as_str())), |row| {
            Ok((
                row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?, row.get::<_, i64>(6)?,
                row.get::<_, f64>(7)?, row.get::<_, i64>(8)?,
                row.get::<_, f64>(9)?, row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, Option<f64>>(12)?, row.get::<_, i64>(13)?,
                row.get::<_, Option<f64>>(14)?, row.get::<_, i64>(15)?,
            ))
        })
        .unwrap_or((0, 0, 0.0, 0, 0, 0, 0, 0.0, 0, 0.0, 0, 0, None, 0, None, 0));

    let (requests, success, avg_latency_ms, token_input, token_output, cache_read, cache_write, avg_ttft_ms, ttft_samples, avg_speed_tps, _, _, total_cost, priced_requests, total_savings, savings_samples) =
        (r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13, r.14, r.15);

    Metrics {
        requests,
        success,
        avg_latency_ms,
        token_input,
        token_output,
        avg_ttft_ms,
        ttft_samples,
        avg_speed_tps,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        // 无已定价请求 → None（与明细 SUM(cost) NULL 语义一致）
        cost: if priced_requests > 0 { total_cost } else { None },
        // 无任何一条可比价差 → None（samples 还原明细 NULL 语义）
        cache_savings: if savings_samples > 0 { total_savings } else { None },
        priced_requests,
    }
}

/// 维度报表（proxy / platform / model）的三段 UNION 装配（D 读端第二阶段）。
/// 与 `query_metrics` 同构，但外层按维度再分组：整小时段走 hourly 表、两个边界小时走明细，
/// 三段每段 `GROUP BY <dim>`，UNION ALL 后外层再按维度合并，逐字段口径与全程查明细一致。
///
/// `dim_expr` 是维度表达式（模块内常量，无注入风险）；`hourly_join` / `detail_join` 分别给出
/// 从 tiny 表别名 `h` / 明细别名 `rs` 补出维度所需的 JOIN 片段（例如按 backend_id 关联 backends）。
/// 返回 (维度值, 该维度的聚合)。
struct DimAggRow {
    requests: i64,
    success: i64,
    stream_requests: i64,
    avg_latency_ms: f64,
    token_input: i64,
    token_output: i64,
    avg_ttft_ms: f64,
    output_speed_tps: f64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    total_cost: Option<f64>,
    cache_savings: Option<f64>,
    priced_requests: i64,
}

/// 维度聚合的核心实现：`from`/`to` 为已规范化（UTC 秒）边界。
/// 开放式范围或区间不足一小时时回退「全程明细」——与 D 之前行为完全一致。
/// `filter_dim` 存在时只聚合该维度值（单实体口径，如 `proxy_stats`），
/// 值绑到每段各自追加的占位符上；与 `hourly_dim`/`detail_dim` 是同一个表达式。
#[allow(clippy::too_many_arguments)]
fn query_dim_stats(
    conn: &rusqlite::Connection,
    from: Option<&str>,
    to: Option<&str>,
    hourly_dim: &str,
    detail_dim: &str,
    hourly_join: &str,
    detail_join: &str,
    filter_dim: Option<&str>,
) -> Vec<(String, DimAggRow)> {
    let agg = hourly_agg_range(from, to);
    let hourly_select = |extra: &str| -> String {
        format!(
            "SELECT {hourly_dim} AS dim,
                    requests, failed_requests, stream_requests,
                    token_input, token_output, cache_read_tokens, cache_write_tokens,
                    latency_ms_sum, ttft_ms_sum, ttft_samples,
                    cost_sum, priced_requests, cache_savings_sum, cache_savings_samples
             FROM request_stats_hourly h
             {hourly_join}
             WHERE 1=1{extra}",
            hourly_dim = hourly_dim,
            hourly_join = hourly_join,
            extra = extra
        )
    };
    let detail_select = |extra: &str| -> String {
        format!(
            "SELECT {detail_dim} AS dim,
                    COUNT(*) AS requests,
                    SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 0 ELSE 1 END) AS failed_requests,
                    SUM(is_stream) AS stream_requests,
                    COALESCE(SUM(token_input),0) AS token_input, COALESCE(SUM(token_output),0) AS token_output,
                    COALESCE(SUM(cache_read_tokens),0) AS cache_read_tokens, COALESCE(SUM(cache_write_tokens),0) AS cache_write_tokens,
                    COALESCE(SUM(latency_ms),0) AS latency_ms_sum, COALESCE(SUM(ttft_ms),0) AS ttft_ms_sum, COUNT(ttft_ms) AS ttft_samples,
                    SUM(cost) AS cost_sum, COUNT(cost) AS priced_requests,
                    COALESCE(SUM(cache_savings),0) AS cache_savings_sum, COUNT(cache_savings) AS cache_savings_samples
             FROM request_stats rs
             {detail_join}
             WHERE 1=1{extra}
             GROUP BY dim",
            detail_dim = detail_dim,
            detail_join = detail_join,
            extra = extra
        )
    };
    // 外层列序（query_map 按位取）：0 dim / 1 requests / 2 success / 3 avg_latency_ms
    // 4 token_input / 5 token_output / 6 avg_ttft_ms / 7 output_speed_tps / 8 cache_read
    // 9 cache_write / 10 total_cost / 11 priced_requests / 12 cache_savings
    let outer = |from_clauses: &[String]| -> String {
        format!(
            "SELECT
                u.dim,
                SUM(u.requests) AS requests,
                SUM(u.requests) - COALESCE(SUM(u.failed_requests),0) AS success,
                COALESCE(SUM(u.stream_requests),0) AS stream_requests,
                CASE WHEN SUM(u.requests) > 0
                     THEN COALESCE(SUM(u.latency_ms_sum),0) * 1.0 / SUM(u.requests)
                     ELSE 0 END AS avg_latency_ms,
                COALESCE(SUM(u.token_input),0), COALESCE(SUM(u.token_output),0),
                CASE WHEN SUM(u.ttft_samples) > 0
                     THEN COALESCE(SUM(u.ttft_ms_sum),0) * 1.0 / SUM(u.ttft_samples)
                     ELSE 0 END AS avg_ttft_ms,
                CASE WHEN SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum),0) > 0
                     THEN COALESCE(SUM(u.token_output),0) * 1000.0 / (SUM(u.latency_ms_sum) - COALESCE(SUM(u.ttft_ms_sum),0))
                     ELSE 0 END AS output_speed_tps,
                COALESCE(SUM(u.cache_read_tokens),0), COALESCE(SUM(u.cache_write_tokens),0),
                SUM(u.cost_sum) AS total_cost,
                COALESCE(SUM(u.priced_requests),0) AS priced_requests,
                CASE WHEN SUM(u.cache_savings_samples) > 0
                     THEN SUM(u.cache_savings_sum) ELSE NULL END AS cache_savings
             FROM (
                {}
             ) u
             GROUP BY u.dim",
            from_clauses.join("\n                UNION ALL\n                ")
        )
    };

    let sql: String;
    let mut params: Vec<String>;
    // filter 值按段各自绑定：三段模式占位符从 7 起（1-6 是区间边界），单段模式接着区间条件；
    // 过滤条件复用各段的 dim 表达式（hourly_dim / detail_dim），参数绑的就是调用方给的维度值
    if let (Some(mf), Some(mt), Some(cf), Some(ct), Some(f), Some(t)) =
        (&agg.mid_from, &agg.mid_to, &agg.core_from, &agg.core_to, &agg.from, &agg.to)
    {
        let filter_sql = filter_dim.map(|_| format!(" AND {hourly_dim} = ?7"));
        let filter_sql2 = filter_dim.map(|_| format!(" AND {detail_dim} = ?8"));
        let filter_sql3 = filter_dim.map(|_| format!(" AND {detail_dim} = ?9"));
        sql = outer(&[
            {
                let mut s = hourly_select(" AND bucket >= ?1 AND bucket < ?2");
                if let Some(fs) = &filter_sql { s.push_str(fs); }
                s
            },
            {
                let mut s = detail_select(" AND created_at >= ?3 AND created_at < ?4");
                if let Some(fs) = &filter_sql2 { s.push_str(fs); }
                s
            },
            {
                let mut s = detail_select(" AND created_at >= ?5 AND created_at < ?6");
                if let Some(fs) = &filter_sql3 { s.push_str(fs); }
                s
            },
        ]);
        params = vec![mf.clone(), mt.clone(), f.clone(), cf.clone(), ct.clone(), t.clone()];
        if let Some(d) = filter_dim { params.extend([d.to_string(), d.to_string(), d.to_string()]); }
    } else {
        let (cond, n_params) = match (&agg.from, &agg.to) {
            (Some(_), Some(_)) => (" AND created_at >= ?1 AND created_at < ?2".to_string(), 2),
            (Some(_), None) | (None, Some(_)) => (" AND created_at >= ?1".to_string(), 1),
            (None, None) => (String::new(), 0),
        };
        let filter_sql = filter_dim.map(|_| format!(" AND {detail_dim} = ?{}", n_params + 1));
        let mut dsel = detail_select(&cond);
        if let Some(fs) = &filter_sql { dsel.push_str(fs); }
        sql = outer(&[dsel]);
        params = agg.from.iter().chain(agg.to.iter()).cloned().collect();
        if let Some(d) = filter_dim { params.push(d.to_string()); }
    }

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("query_dim_stats prepare failed: {e}");
            return Vec::new();
        }
    };
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|s| s.as_str())), |r| {
            Ok((
                r.get::<_, String>(0)?,
                DimAggRow {
                    requests: r.get(1)?,
                    success: r.get(2)?,
                    stream_requests: r.get(3)?,
                    avg_latency_ms: r.get(4)?,
                    token_input: r.get(5)?,
                    token_output: r.get(6)?,
                    avg_ttft_ms: r.get(7)?,
                    output_speed_tps: r.get(8)?,
                    cache_read_tokens: r.get(9)?,
                    cache_write_tokens: r.get(10)?,
                    total_cost: r.get(11)?,
                    priced_requests: r.get(12)?,
                    cache_savings: r.get(13)?,
                },
            ))
        });
    let mut out = Vec::new();
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            out.push(row);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn build_overview(
    conn: &rusqlite::Connection,
    total_requests: i64,
    success_count: i64,
    avg_latency: f64,
    total_token_input: i64,
    total_token_output: i64,
    total_connections_active: i64,
    requests_prev_period: i64,
    avg_latency_prev_period: f64,
    avg_ttft: f64,
    avg_speed: f64,
    total_cache_read: i64,
    total_cache_write: i64,
    total_cost: Option<f64>,
    total_cache_savings: Option<f64>,
    priced_requests: i64,
) -> OverviewStats {
    let total_proxies: i64 = conn.query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0)).unwrap_or(0);
    let active_proxies = total_proxies; // all proxies are active (no status field)
    let total_platforms: i64 = conn.query_row("SELECT COUNT(*) FROM platforms", [], |r| r.get(0)).unwrap_or(0);
    let active_platforms = total_platforms; // all platforms are active
    let total_models: i64 = conn.query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0)).unwrap_or(0);
    let (peak_priced_models, peak_active_models, peak_next_at) = peak_state(conn, chrono::Local::now());

    OverviewStats {
        total_requests, success_rate: if total_requests > 0 { success_count as f64 / total_requests as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: avg_latency, active_proxies, total_proxies, active_platforms, total_platforms, total_models,
        total_token_input, total_token_output,
        total_connections_active, requests_prev_period, avg_latency_prev_period,
        avg_ttft_ms: avg_ttft, avg_output_speed_tps: avg_speed,
        total_cache_read_tokens: total_cache_read,
        // 新增输入 = 总输入 − 缓存读 − 缓存写（saturating 防御异常数据出现负数）
        fresh_input_tokens: (total_token_input - total_cache_read - total_cache_write).max(0),
        // CC Switch 口径：命中 / (命中 + 新增输入)，即"可缓存 prompt 里多少比例真的命中"
        cache_hit_rate: {
            let denominator = total_cache_read + (total_token_input - total_cache_read - total_cache_write).max(0);
            if denominator > 0 { total_cache_read as f64 / denominator as f64 * 100.0 } else { 0.0 }
        },
        total_cost,
        total_cache_savings,
        priced_requests,
        peak_priced_models,
        peak_active_models,
        peak_next_at,
    }
}

/// 当前高峰计费状态：配置了高峰价的启用模型数、正在高峰窗口里的模型数、下一次切换时刻。
/// 只看启用且基础价完整的模型——半截价格的模型本来就不会产生成本。
fn peak_state(
    conn: &rusqlite::Connection,
    at: chrono::DateTime<chrono::Local>,
) -> (i64, i64, Option<String>) {
    let Ok(mut stmt) = conn.prepare(
        // status 既可能是旧行的裸值 Active，也可能是新行 serde 写出的 "Active"（带引号）
        "SELECT peak_pricing FROM models \
         WHERE peak_pricing IS NOT NULL AND status IN ('Active', '\"Active\"') \
           AND input_price IS NOT NULL AND output_price IS NOT NULL",
    ) else {
        return (0, 0, None);
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return (0, 0, None);
    };
    let mut configured = 0i64;
    let mut active = 0i64;
    let mut next: Option<chrono::DateTime<chrono::Local>> = None;
    for raw in rows.flatten() {
        let Ok(peak) = serde_json::from_str::<crate::models::pricing::PeakPricing>(&raw) else {
            continue;
        };
        if peak.windows.is_empty() || peak.peak.is_none() {
            continue;
        }
        configured += 1;
        if peak.is_active(at) {
            active += 1;
        }
        if let Some(change) = peak.next_change(at) {
            next = Some(match next {
                Some(current) => current.min(change),
                None => change,
            });
        }
    }
    (configured, active, next.map(|t| t.format("%H:%M").to_string()))
}

/// 计算对比用的上一周期：有范围时为等长的前一段 [from-dur, from)；无范围返回 None（调用方回退昨日口径）
fn prev_range(range: &TimeRange) -> Option<TimeRange> {
    let (from, to) = (range.from.as_ref()?, range.to.as_ref()?);
    let f = chrono::DateTime::parse_from_rfc3339(from).ok()?;
    let t = chrono::DateTime::parse_from_rfc3339(to).ok()?;
    let dur = t.signed_duration_since(f);
    if dur <= chrono::Duration::zero() {
        return None;
    }
    let pf = f - dur;
    Some(TimeRange {
        from: Some(pf.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, false)),
        to: Some(from.clone()),
    })
}

pub fn proxy_stats(pool: &DbPool, proxy_id: &str, from: Option<&str>, to: Option<&str>) -> AppResult<ProxyStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // Get proxy name
    let proxy_name: String = conn.query_row(
        "SELECT name FROM proxies WHERE id = ?1",
        rusqlite::params![proxy_id],
        |r| r.get(0)
    ).unwrap_or_default();

    // D 读端第二阶段：单实体也走 hourly 三段 UNION（口径与 bulk 完全一致，只是各段加维度过滤）。
    // 悬挂行（网关已删）不会命中 proxies 表，aggregate 为空 → 下方按 0 口径兜底。
    let rows = query_dim_stats(
        &conn,
        range.from.as_deref(),
        range.to.as_deref(),
        "h.proxy_id",
        "rs.proxy_id",
        "",
        "",
        Some(proxy_id),
    );
    let row = rows.into_iter().next().map(|(_, r)| r);

    // 今日请求数：与原口径一致（固定"今日"，不受时段筛选影响）
    let requests_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1 AND date(created_at) = date('now')",
        rusqlite::params![proxy_id],
        |r| r.get(0)
    ).unwrap_or(0);

    Ok(ProxyStats {
        proxy_id: proxy_id.to_string(),
        proxy_name,
        total_requests: row.as_ref().map_or(0, |r| r.requests),
        success_rate: row.as_ref().map_or(0.0, |r| if r.requests > 0 { r.success as f64 / r.requests as f64 * 100.0 } else { 0.0 }),
        avg_latency_ms: row.as_ref().map_or(0.0, |r| r.avg_latency_ms),
        requests_today,
        total_token_input: row.as_ref().map_or(0, |r| r.token_input),
        total_token_output: row.as_ref().map_or(0, |r| r.token_output),
        total_cost: row.as_ref().and_then(|r| r.total_cost),
        priced_requests: row.as_ref().map_or(0, |r| r.priced_requests),
    })
}

/// 全部虚拟模型的时段用量：两条 SQL，替代按 proxy 逐个查询（每个 proxy 原本 6 条）
pub fn proxy_stats_all(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ProxyStats>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // 现有 proxies 全名单（含没有任何请求的，报表必须给 0 行；名查 id 补上 name）
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, name FROM proxies")?;
        let mapped = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in mapped.flatten() {
            names.insert(row.0, row.1);
        }
    }

    // D 读端第二阶段：hourly 三段 UNION；维度 = proxy（hourly 表自带 proxy_id）。
    // 只查有数据的维度；外层兜底补无数据 proxy 的 0 行。
    // hourly 段别名 h、detail 段别名 rs，dim 表达式必须各自成立——这是两套表达式的原因
    let rows = query_dim_stats(
        &conn,
        range.from.as_deref(),
        range.to.as_deref(),
        "h.proxy_id",
        "rs.proxy_id",
        "",
        "",
        None,
    );

    // 只保留现存网关维度：悬挂 `proxy_id`（网关已删）的请求仍计入它所处平台的统计，
    // 但不属于任何网关——旧批量 SQL 从 proxies 表驱动，天然排除，这里显式过滤保持一致
    let mut stats: Vec<ProxyStats> = rows
        .into_iter()
        .filter(|(proxy_id, _)| names.contains_key(proxy_id))
        .map(|(proxy_id, row)| {
            let name = names.get(&proxy_id).cloned().unwrap_or_default();
            let mut s = ProxyStats {
                proxy_name: name,
                total_requests: row.requests,
                success_rate: if row.requests > 0 { row.success as f64 / row.requests as f64 * 100.0 } else { 0.0 },
                avg_latency_ms: row.avg_latency_ms,
                requests_today: 0, // 下方统一补
                total_token_input: row.token_input,
                total_token_output: row.token_output,
                total_cost: row.total_cost,
                priced_requests: row.priced_requests,
                proxy_id: String::new(),
            };
            s.proxy_id = proxy_id;
            s
        })
        .collect();

    // 零流量的 proxy 也要有自己的行：报表口径「没请求 = 0」，不是「没有这一行」
    let seen: std::collections::HashSet<String> = stats.iter().map(|s| s.proxy_id.clone()).collect();
    for (id, name) in &names {
        if !seen.contains(id) {
            stats.push(ProxyStats {
                proxy_id: id.clone(),
                proxy_name: name.clone(),
                total_requests: 0,
                success_rate: 0.0,
                avg_latency_ms: 0.0,
                requests_today: 0,
                total_token_input: 0,
                total_token_output: 0,
                total_cost: None,
                priced_requests: 0,
            });
        }
    }

    // 今日请求数与原口径一致：固定"今日"，不受时段筛选影响
    let mut today = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT proxy_id, COUNT(*) FROM request_stats WHERE proxy_id IS NOT NULL AND date(created_at) = date('now') GROUP BY proxy_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows.flatten() {
            today.insert(row.0, row.1);
        }
    }
    for item in stats.iter_mut() {
        item.requests_today = today.get(&item.proxy_id).copied().unwrap_or(0);
    }

    Ok(stats)
}

/// 全部平台时段用量：一条 SQL，替代按 platform 逐个查询（每个平台原本 5 条）。
/// backend 被删除后的悬挂统计行按 `platform_stats` 的口径归入 unknown，这里天然不参与分组。
pub fn platform_stats_all(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<PlatformStats>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // D 读端第二阶段：hourly 三段 UNION；维度 = platform（hourly 有 backend_id → backends 拿 platform_id）。
    // 两侧都 JOIN backends，仅别名不同（h/rs），dim 表达式同文
    let rows = query_dim_stats(
        &conn,
        range.from.as_deref(),
        range.to.as_deref(),
        "b.platform_id",
        "b.platform_id",
        "LEFT JOIN backends b ON b.id = h.backend_id",
        "LEFT JOIN backends b ON b.id = rs.backend_id",
        None,
    );

    // platform 全名单（含零流量的，报表给 0 行）；悬挂 backend 行（platform_id 为 NULL）不归属任何平台
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, name FROM platforms")?;
        let mapped = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in mapped.flatten() {
            names.insert(row.0, row.1);
        }
    }

    let mut stats: Vec<PlatformStats> = Vec::new();
    for (platform_id, row) in rows {
        if platform_id.is_empty() {
            // 悬挂 backend 的行在全部平台视角里不归属；与旧 LEFT JOIN 语义一致
            continue;
        }
        stats.push(PlatformStats {
            platform_id: platform_id.clone(),
            platform_name: names.get(&platform_id).cloned().unwrap_or_default(),
            total_requests: row.requests,
            success_rate: if row.requests > 0 { row.success as f64 / row.requests as f64 * 100.0 } else { 0.0 },
            avg_latency_ms: row.avg_latency_ms,
            total_token_input: row.token_input,
            total_token_output: row.token_output,
            total_cost: row.total_cost,
            priced_requests: row.priced_requests,
        });
    }

    let seen: std::collections::HashSet<String> = stats.iter().map(|s| s.platform_id.clone()).collect();
    for (id, name) in &names {
        if !seen.contains(id) {
            stats.push(PlatformStats {
                platform_id: id.clone(),
                platform_name: name.clone(),
                total_requests: 0,
                success_rate: 0.0,
                avg_latency_ms: 0.0,
                total_token_input: 0,
                total_token_output: 0,
                total_cost: None,
                priced_requests: 0,
            });
        }
    }

    Ok(stats)
}

pub fn platform_stats(pool: &DbPool, platform_id: &str, from: Option<&str>, to: Option<&str>) -> AppResult<PlatformStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // Get platform name
    let platform_name: String = conn.query_row(
        "SELECT name FROM platforms WHERE id = ?1",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or_default();

    // D 读端第二阶段：单实体也走 hourly 三段 UNION（口径与 bulk 完全一致，各段加维度过滤）。
    // 悬挂 backend 行（platform_id 为 NULL）不归属任何平台——旧实现里显式归 "unknown"，
    // bulk 视角则跳过 NULL，这里配合 dim = ?N 只在非悬挂行上命中，两边数字一致。
    let rows = query_dim_stats(
        &conn,
        range.from.as_deref(),
        range.to.as_deref(),
        "b.platform_id",
        "b.platform_id",
        "LEFT JOIN backends b ON b.id = h.backend_id",
        "LEFT JOIN backends b ON b.id = rs.backend_id",
        Some(platform_id),
    );
    let row = rows.into_iter().next().map(|(_, r)| r);

    Ok(PlatformStats {
        platform_id: platform_id.to_string(),
        platform_name,
        total_requests: row.as_ref().map_or(0, |r| r.requests),
        success_rate: row.as_ref().map_or(0.0, |r| if r.requests > 0 { r.success as f64 / r.requests as f64 * 100.0 } else { 0.0 }),
        avg_latency_ms: row.as_ref().map_or(0.0, |r| r.avg_latency_ms),
        total_token_input: row.as_ref().map_or(0, |r| r.token_input),
        total_token_output: row.as_ref().map_or(0, |r| r.token_output),
        total_cost: row.as_ref().and_then(|r| r.total_cost),
        priced_requests: row.as_ref().map_or(0, |r| r.priced_requests),
    })
}

pub fn backend_stats(pool: &DbPool, backend_id: &str) -> AppResult<BackendStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats WHERE backend_id = ?1", rusqlite::params![backend_id], |r| r.get(0)).unwrap_or(0);
    let success: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats WHERE backend_id = ?1 AND status_code >= 200 AND status_code < 300", rusqlite::params![backend_id], |r| r.get(0)).unwrap_or(0);
    let latency: f64 = conn.query_row("SELECT COALESCE(AVG(latency_ms),0) FROM request_stats WHERE backend_id = ?1", rusqlite::params![backend_id], |r| r.get(0)).unwrap_or(0.0);

    // 取 platform / model 名称（悬挂 backend 时兜底为 unknown）
    let (platform_name, model_name): (String, String) = conn
        .query_row(
            "SELECT p.name, b.model_id FROM backends b LEFT JOIN platforms p ON b.platform_id = p.id WHERE b.id = ?1",
            rusqlite::params![backend_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or_else(|_| (String::from("unknown"), String::from("unknown")));

    Ok(BackendStats {
        backend_id: backend_id.to_string(),
        platform_name,
        model_name,
        total_requests: total,
        success_rate: if total > 0 { success as f64 / total as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: latency,
        active_connections: 0,
    })
}

/// 记一次平台连通性检测。由测试连接接口在探测完成后调用（只在平台已保存时），
/// 失败原因只存短文本，不含凭据。
pub fn record_platform_health(
    pool: &DbPool,
    platform_id: &str,
    ok: bool,
    latency_ms: i64,
    model_count: i64,
    error: Option<&str>,
) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO platform_health (platform_id, ok, latency_ms, model_count, error, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            platform_id,
            if ok { 1i64 } else { 0i64 },
            latency_ms,
            model_count,
            error.map(|s| s.chars().take(300).collect::<String>()),
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

const HEALTH_COLS: &str = "id, platform_id, ok, latency_ms, model_count, error, created_at";

fn map_health(r: &rusqlite::Row) -> rusqlite::Result<PlatformHealth> {
    Ok(PlatformHealth {
        id: r.get(0)?,
        platform_id: r.get(1)?,
        ok: r.get::<_, i64>(2)? != 0,
        latency_ms: r.get(3)?,
        model_count: r.get(4)?,
        error: r.get(5)?,
        created_at: r.get(6)?,
    })
}

/// 单个平台最近 N 次检测（倒序）；详情页历史列表用
pub fn platform_health_history(pool: &DbPool, platform_id: &str, limit: i64) -> AppResult<Vec<PlatformHealth>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM platform_health WHERE platform_id = ?1 ORDER BY id DESC LIMIT ?2",
        HEALTH_COLS
    ))?;
    let rows = stmt.query_map(rusqlite::params![platform_id, limit], map_health)?;
    Ok(rows.filter_map(|x| x.ok()).collect())
}

/// 时段内每平台健康汇总：检测/失败次数按范围聚合，"最后一次"取范围内最新一行。
/// 两次查询：聚合 + 按 last_id 批量取明细，避免按平台逐个查。
pub fn platform_health_all(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<PlatformHealthSummary>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let agg_sql = format!(
        "SELECT platform_id, COUNT(*), SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), MAX(id) FROM platform_health WHERE 1=1{} GROUP BY platform_id ORDER BY platform_id",
        range.and("created_at")
    );
    let mut stmt = conn.prepare(&agg_sql)?;
    let agg: Vec<(String, i64, i64, i64)> = stmt
        .query_map(rusqlite::params_from_iter(range.params()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), r.get(3)?))
        })?
        .filter_map(|x| x.ok())
        .collect();
    if agg.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; agg.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM platform_health WHERE id IN ({})",
        HEALTH_COLS, placeholders
    ))?;
    let latest: std::collections::HashMap<String, PlatformHealth> = stmt
        .query_map(rusqlite::params_from_iter(agg.iter().map(|a| a.3)), map_health)?
        .filter_map(|x| x.ok())
        .map(|row| (row.platform_id.clone(), row))
        .collect();

    Ok(agg.into_iter().map(|(platform_id, checks, failures, _)| {
        let last = latest.get(&platform_id);
        PlatformHealthSummary {
            platform_id,
            checks,
            failures,
            last_ok: last.map(|h| h.ok),
            last_latency_ms: last.map(|h| h.latency_ms),
            last_model_count: last.map(|h| h.model_count),
            last_error: last.and_then(|h| h.error.clone()),
            last_checked_at: last.map(|h| h.created_at.clone()),
        }
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("stats-test-{}.db", uuid::Uuid::new_v4()));
        (crate::db::init_pool(&path).expect("init test db"), path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn stat(api_key_id: &str, cost: Option<f64>) -> RequestStat {
        RequestStat {
            id: 0,
            proxy_id: "p1".into(), route_id: "r1".into(), backend_id: "b1".into(),
            status_code: 200, latency_ms: 100,
            token_input: Some(1000), token_output: Some(500),
            error_type: None, created_at: "2026-01-01T00:00:00.000Z".into(),
            ttft_ms: None, cache_read_tokens: None, cache_write_tokens: None,
            is_stream: false, api_key_id: Some(api_key_id.into()), cost,
            cache_savings: None,
            request_id: None, replayed_from: None,
            credits: None, path: None,
        }
    }

    /// 预聚合小时表：同一桶同一后端累加，跨桶分行；失败数、流式数、成本 NULL 语义与明细一致。
    #[test]
    fn hourly_aggregate_accumulates_same_bucket_and_respects_cost_null() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");

            // 三个请求：两条同一小时（一成一败、一有价一无价）、一条不同小时
            let mut ok_priced = stat("k", Some(0.01));
            ok_priced.latency_ms = 50;
            ok_priced.token_input = Some(100);
            ok_priced.is_stream = true;
            let mut fail_unpriced = stat("k", None);
            fail_unpriced.status_code = 502;
            fail_unpriced.error_type = Some("ServerError".into());
            fail_unpriced.latency_ms = 200;
            let mut next_hour = stat("k", Some(0.02));
            next_hour.created_at = "2026-01-01T02:00:00.000Z".into();

            super::record_batch(&pool, &[ok_priced, fail_unpriced, next_hour], &[], &[], &[], &[])
                .expect("batch with upsert");

            let rows: Vec<(String, i64, i64, i64, i64, i64, i64, i64, Option<f64>, f64)> = conn
                .prepare("SELECT bucket, requests, failed_requests, stream_requests, token_input, latency_ms_sum, ttft_samples, priced_requests, cost_sum, cache_savings_sum FROM request_stats_hourly ORDER BY bucket")
                .expect("query")
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))
                .expect("map")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect");

            assert_eq!(rows.len(), 2, "两个不同小时桶");
            let (b1, b2) = (&rows[0], &rows[1]);
            // 第一条小时桶：成功（无失败）+ 失败（无成功）→ requests=2, failed=1, stream=1
            assert_eq!((b1.1, b1.2, b1.3), (2, 1, 1), "同桶两条累加：2 请求 1 失败 1 流式");
            // 有价 0.01 + 无价 None → cost_sum=0.01, priced_requests=1
            assert_eq!((b1.8, b1.9), (Some(0.01), 0.0), "cost_sum 只累加有价行");
            // token 只来自有价那条（fail_unpriced 继承默认 1000）
            assert_eq!(b1.4, 1100, "100 + 1000");
            // latency 两条都算
            assert_eq!(b1.5, 250, "50 + 200");
            // ttft_samples：两条都无 ttft
            assert_eq!(b1.6, 0);
            // 第二条小时桶：单条成功有价
            assert_eq!((b2.1, b2.2, b2.8, b2.9), (1, 0, Some(0.02), 0.0));
        }
        cleanup_db(&path);
    }

    /// 预聚合表与明细同事务：明细失败时聚合也不该落（事务原子性由 record_batch 保证）。
    /// 这里验证「非法 created_at 的行被跳过而不致整批失败」——聚合是尽力而为，明细照写。
    #[test]
    fn hourly_aggregate_skips_malformed_timestamps_without_breaking_the_batch() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            let mut bad = stat("k", Some(0.01));
            bad.created_at = "not-a-timestamp".into();
            let mut good = stat("k", Some(0.01));

            super::record_batch(&pool, &[bad, good], &[], &[], &[], &[])
                .expect("batch with one malformed timestamp");

            // 明细两条都在
            let detail: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats", [], |r| r.get(0)).unwrap();
            assert_eq!(detail, 2, "明细不受聚合影响");
            // 聚合只落了一条（good 那条），bad 被跳过
            let agg: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats_hourly", [], |r| r.get(0)).unwrap();
            assert_eq!(agg, 1, "非法时间戳的行不进聚合，但整批不失败");
        }
        cleanup_db(&path);
    }

    /// 整小时段切分（D 读端核心）：按本地时区的整小时边界切开，边界小时保持原秒级边界。
    /// 只有在一个完整小时及以上时才给出核心段 —— 不足一小时 / 开放式范围不得用小时表。
    #[test]
    fn hourly_agg_range_splits_only_on_complete_hours() {
        use super::hourly_agg_range;

        // 桶是服务器本地时区的 "YYYY-MM-DD HH"，测试机时区不定，只断言不变量：
        // 跨多个小时的区间一定给出非空核心段，且核心段落在两个边界之间（单调）。
        let agg = hourly_agg_range(Some("2026-01-01T10:30:00+00:00"), Some("2026-01-01T12:30:00+00:00"));
        let (Some(mf), Some(mt)) = (&agg.mid_from, &agg.mid_to) else {
            panic!("跨小时区间必须有核心段：{agg:?}");
        };
        assert_eq!(mf.len(), 13, "桶必须形如 YYYY-MM-DD HH");
        assert_eq!(mt.len(), 13);
        assert!(mf < mt, "核心段必须严格非空");

        // 不足一小时 [10:30, 11:30) → 无核心段，整体回退明细
        let under = hourly_agg_range(Some("2026-01-01T10:30:00+00:00"), Some("2026-01-01T11:30:00+00:00"));
        assert_eq!(under.mid_from, None);
        assert_eq!(under.mid_to, None);

        // 开放式范围（缺 from / 缺 to）→ 无核心段
        let open = hourly_agg_range(None, Some("2026-01-01T12:00:00+00:00"));
        assert_eq!(open.mid_from, None);
        assert_eq!(open.mid_to, None);

        // 边界原样透传（调用方已规范化，口径是 UTC 秒）
        assert_eq!(agg.from.as_deref(), Some("2026-01-01T10:30:00+00:00"));
        assert_eq!(agg.to.as_deref(), Some("2026-01-01T12:30:00+00:00"));
    }

    /// 读端三段 UNION 与「全程查明细」逐字段等价：跨小时边界、失败/流式/成本/NULL 语义，
    /// 以及缺失数据（表空）时的降级路径都不能改变读出口。
    #[test]
    fn query_metrics_matches_pure_detail_query_across_hour_bounds() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            use super::query_metrics;

            // 三小时边界：10:59、11:00（整点入下一小时桶）、11:59、12:00、12:01
            // 时区：测试机本地时区即数据库 datetime(...,'localtime') 的口径；
            // 这里用带 +00:00 的 UTC 字面量，桶按服务器本地时区切。
            let times = [
                "2026-01-01T10:59:30Z",  // 前边界桶
                "2026-01-01T11:00:00Z",
                "2026-01-01T11:59:59Z",
                "2026-01-01T12:00:00Z",
                "2026-01-01T12:01:00Z",  // 后边界桶，须严格小于 to（12:01:30Z）
            ];
            let mut rows = Vec::new();
            for (i, ts) in times.iter().enumerate() {
                let mut s = stat(&format!("k{i}"), Some(0.01 + i as f64 * 0.001));
                s.created_at = (*ts).to_string();
                if i == 1 {
                    s.status_code = 502; // 失败行
                    s.error_type = Some("ServerError".into());
                }
                if i == 3 {
                    s.cache_savings = Some(0.005); // 价差行
                }
                s.latency_ms = 100 + i as i64;
                s.ttft_ms = Some(10 + i as i64);
                s.is_stream = i % 2 == 1;
                rows.push(s);
            }
            super::record_batch(&pool, &rows, &[], &[], &[], &[]).expect("batch with upsert");

            // 跨多个整小时：核心段在内小时表走，两端小时明细走
            let agg = hourly_agg_range(
                Some("2026-01-01T10:59:30Z"),
                Some("2026-01-01T12:01:30Z"),
            );
            assert!(agg.mid_from.is_some(), "跨小时区间必须有核心段");
            let got = query_metrics(&conn, Some("2026-01-01T10:59:30Z"), Some("2026-01-01T12:01:30Z"));

            // 对照：全程查明细（等价旧口径）
            let direct = conn
                .query_row(
                    "SELECT COUNT(*) AS n FROM request_stats WHERE created_at >= ?1 AND created_at < ?2",
                    rusqlite::params!["2026-01-01T10:59:30Z", "2026-01-01T12:01:30Z"],
                    |r| r.get::<_, i64>(0),
                )
                .expect("count");

            assert_eq!(got.requests, direct, "请求总数与明细一致");
            // 已知行：5 行全在区间内
            assert_eq!(got.requests, 5);
            assert_eq!(got.success, 4, "502 那行不算成功");
            assert_eq!(got.token_input, 5 * 1000, "每行 token_input=1000");
            assert_eq!(got.cache_read_tokens, 0);
            assert_eq!(got.cache_write_tokens, 0);
            // 失败行 502 也算 latency 与 ttft 样本
            assert_eq!(got.priced_requests, 5, "5 行全有价");
            let cost = got.cost.expect("有价必须给总数");
            assert!((cost - (5.0 * 0.01 + 0.001 * 10.0)).abs() < 1e-9, "cost 之和");
            // 价差行有一条
            let savings = got.cache_savings.expect("有价差必须给总数");
            assert!((savings - 0.005).abs() < 1e-12, "cache_savings 之和");

            // 空库（直接删掉明细、从小时表删）→ 降级路径也无 NULL 崩溃
            conn.execute("DELETE FROM request_stats", []).expect("clear detail");
            conn.execute("DELETE FROM request_stats_hourly", []).expect("clear hourly");
            let empty = query_metrics(&conn, Some("2026-01-01T10:59:30Z"), Some("2026-01-01T12:01:30Z"));
            assert_eq!(empty.requests, 0);
            assert_eq!(empty.cost, None, "空库成本必须是 None 不是 0");
            assert_eq!(empty.cache_savings, None);
        }
        cleanup_db(&path);
    }

    /// 单小时区间（不足整数小时）也必须走明细口径 —— 这是 `hourly_agg_range` 返回 None 的落点。
    #[test]
    fn query_metrics_sub_hour_range_falls_back_to_detail() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            use super::query_metrics;

            let mut a = stat("k-a", None);
            a.created_at = "2026-01-01T10:10:00Z".into();
            let mut b = stat("k-b", None);
            b.created_at = "2026-01-01T10:50:00Z".into();
            super::record_batch(&pool, &[a, b], &[], &[], &[], &[]).expect("batch");

            let got = query_metrics(&conn, Some("2026-01-01T10:00:00Z"), Some("2026-01-01T11:00:00Z"));
            assert_eq!(got.requests, 2);
            assert_eq!(got.cost, None, "两行都未定价 → None");
            assert_eq!(got.cache_savings, None);
        }
        cleanup_db(&path);
    }

    /// 时序点要带齐单模型分时图需要的聚合：新增输入、缓存写、成本覆盖、首字样本来数。
    /// 成本全 NULL 的点必须是 None（不是 0）；ttft 全 NULL 的点 samples 必须是 0。
    #[test]
    fn model_timeseries_carries_cache_cost_and_ttft_samples() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO platforms (id,name,type,base_url,api_key,created_at,updated_at) VALUES ('pl','p','openai','http://x','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            ).expect("platform");
            conn.execute("INSERT INTO proxies (id,name,created_at) VALUES ('px','px','2026-01-01T00:00:00Z')", []).expect("proxy");
            conn.execute("INSERT INTO routes (id,proxy_id) VALUES ('rt','px')", []).expect("route");
            conn.execute(
                "INSERT INTO backends (id,route_id,platform_id,model_id,weight,priority) VALUES ('b-a','rt','pl','model-a',1,0),('b-b','rt','pl','model-b',1,0)",
                [],
            ).expect("backends");
        }

        let mut cached = stat("k", Some(0.01));
        cached.backend_id = "b-a".into();
        cached.created_at = "2026-01-01T10:15:00Z".into();
        cached.ttft_ms = Some(100);
        cached.is_stream = true;
        cached.cache_read_tokens = Some(600);
        cached.cache_write_tokens = Some(100);
        record(&pool, &cached).expect("cached row");

        let mut plain = stat("k", None);
        plain.backend_id = "b-a".into();
        plain.created_at = "2026-01-01T10:45:00Z".into();
        plain.token_input = Some(500);
        plain.token_output = Some(100);
        record(&pool, &plain).expect("unpriced row");

        let mut uncached_stream = stat("k", None);
        uncached_stream.backend_id = "b-b".into();
        uncached_stream.created_at = "2026-01-01T10:30:00Z".into();
        uncached_stream.token_input = Some(300);
        uncached_stream.token_output = Some(50);
        uncached_stream.ttft_ms = Some(200);
        uncached_stream.is_stream = true;
        // 异常数据：缓存读大于总输入，fresh 必须 saturating 到 0 而不是负数
        uncached_stream.cache_read_tokens = Some(400);
        record(&pool, &uncached_stream).expect("uncached row");

        let series = model_timeseries(
            &pool,
            "hour",
            24,
            Some("2026-01-01T00:00:00Z"),
            Some("2026-01-02T00:00:00Z"),
        )
        .expect("timeseries");

        let a = series.data.iter().find(|p| p.model_id == "model-a").expect("model-a point");
        assert_eq!(a.requests, 2);
        assert_eq!(a.token_input, 1500);
        assert_eq!(a.cache_read_tokens, 600);
        assert_eq!(a.cache_write_tokens, 100);
        assert_eq!(a.fresh_input_tokens, 800, "1500 - 600 - 100");
        assert_eq!(a.priced_requests, 1, "只有第一行有成本");
        assert!((a.total_cost.expect("已定价请求") - 0.01).abs() < 1e-12);
        assert_eq!(a.ttft_samples, 1, "只有流式那一行有首字");
        assert_eq!(a.avg_ttft_ms, 100.0);
        assert_eq!(a.stream_requests, 1);

        let b = series.data.iter().find(|p| p.model_id == "model-b").expect("model-b point");
        assert_eq!(b.requests, 1);
        assert_eq!(b.cache_read_tokens, 400);
        assert_eq!(b.fresh_input_tokens, 0, "缓存读大于总输入时 saturating 到 0");
        assert_eq!(b.total_cost, None, "全未定价是 NULL，不是 0");
        assert_eq!(b.priced_requests, 0);
        assert_eq!(b.ttft_samples, 1);
        assert_eq!(b.avg_ttft_ms, 200.0);

        // 单模型分时图与矩阵必须同口径：桶求和/加权必须逐项等于同一范围的 model_summaries。
        let summaries = model_summaries(
            &pool,
            Some("2026-01-01T00:00:00Z"),
            Some("2026-01-02T00:00:00Z"),
        )
        .expect("summaries");
        for summary in &summaries {
            let points: Vec<_> = series.data.iter().filter(|p| p.model_id == summary.model_id).collect();
            assert_eq!(points.iter().map(|p| p.requests).sum::<i64>(), summary.requests);
            assert_eq!(points.iter().map(|p| p.token_input).sum::<i64>(), summary.token_input);
            assert_eq!(points.iter().map(|p| p.token_output).sum::<i64>(), summary.token_output);
            assert_eq!(points.iter().map(|p| p.cache_read_tokens).sum::<i64>(), summary.cache_read_tokens);
            assert_eq!(points.iter().map(|p| p.fresh_input_tokens).sum::<i64>(), summary.fresh_input_tokens);
            assert_eq!(points.iter().map(|p| p.priced_requests).sum::<i64>(), summary.priced_requests);
            let cost: f64 = points.iter().filter_map(|p| p.total_cost).sum();
            match summary.total_cost {
                Some(total) => assert!((cost - total).abs() < 1e-12, "成本合计不一致：{cost} vs {total}"),
                None => assert_eq!(summary.priced_requests, 0),
            }
            let weighted = points.iter().map(|p| p.avg_latency_ms * p.requests as f64).sum::<f64>()
                / summary.requests as f64;
            assert!(
                (weighted - summary.avg_latency_ms).abs() < 1e-9,
                "加权耗时不一致：{weighted} vs {}",
                summary.avg_latency_ms
            );
        }

        cleanup_db(&path);
    }

    /// 大屏矩阵的失败数下钻：按 backends.model_id 过滤，且 total/success 同口径
    #[test]
    fn error_stats_filters_by_backend_model() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO platforms (id,name,type,base_url,api_key,created_at,updated_at) VALUES ('pl','p','openai','http://x','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            ).expect("platform");
            conn.execute("INSERT INTO proxies (id,name,created_at) VALUES ('px','px','2026-01-01T00:00:00Z')", []).expect("proxy");
            conn.execute("INSERT INTO routes (id,proxy_id) VALUES ('rt','px')", []).expect("route");
            conn.execute(
                "INSERT INTO backends (id,route_id,platform_id,model_id,weight,priority) VALUES ('b-a','rt','pl','model-a',1,0),('b-b','rt','pl','model-b',1,0)",
                [],
            ).expect("backends");
        }

        let mut rate_limited = stat("k", None);
        rate_limited.backend_id = "b-a".into();
        rate_limited.status_code = 429;
        rate_limited.error_type = Some("RateLimit".into());
        record(&pool, &rate_limited).expect("record a");

        let mut server_error = stat("k", None);
        server_error.backend_id = "b-a".into();
        server_error.status_code = 500;
        server_error.error_type = Some("ServerError".into());
        record(&pool, &server_error).expect("record a2");

        let mut timeout = stat("k", None);
        timeout.backend_id = "b-b".into();
        timeout.status_code = 504;
        timeout.error_type = Some("Timeout".into());
        record(&pool, &timeout).expect("record b");

        let only_a = error_stats(&pool, None, None, Some("model-a")).expect("model-a");
        assert_eq!(only_a.total, 2);
        assert_eq!(only_a.success, 0);
        let mut kinds: Vec<(&str, i64)> = only_a.errors.iter().map(|e| (e.kind.as_str(), e.count)).collect();
        kinds.sort();
        assert_eq!(kinds, vec![("RateLimit", 1), ("ServerError", 1)]);

        let all = error_stats(&pool, None, None, None).expect("all");
        assert_eq!(all.total, 3);
        assert_eq!(all.errors.iter().map(|e| e.count).sum::<i64>(), 3);

        cleanup_db(&path);
    }

    /// 上界截断到秒会把这一秒内写入的行全部排除：`created_at` 带纳秒，字典序里
    /// `...04.204518400+00:00` 大于 `...04+00:00`。流量页实时尾巴上最新那一两条就是这么消失的。
    #[test]
    fn range_bound_covers_the_whole_second_it_lands_in() {
        assert_eq!(normalize_bound("2026-10-21T16:20:04.768Z", "to").unwrap(), "2026-10-21T16:20:05+00:00");
        assert_eq!(normalize_bound("2026-10-21T16:20:04Z", "to").unwrap(), "2026-10-21T16:20:04+00:00");
        assert_eq!(normalize_bound("2026-10-21T16:20:04.768+02:00", "from").unwrap(), "2026-10-21T14:20:05+00:00");

        let (pool, path) = test_db();
        let mut row = stat("k-bound", None);
        row.created_at = "2026-10-21T16:20:04.204518400+00:00".into();
        record(&pool, &row).expect("record");

        let range = TimeRange::parse(Some("2026-10-21T16:19:00Z"), Some("2026-10-21T16:20:04.768Z")).expect("parse");
        let sql = format!("SELECT COUNT(*) FROM request_stats WHERE 1=1{}", range.and("created_at"));
        let params = range.params();
        let found: i64 = pool
            .get()
            .expect("conn")
            .query_row(&sql, rusqlite::params![params[0], params[1]], |r| r.get(0))
            .expect("count");
        cleanup_db(&path);
        assert_eq!(found, 1, "写入这一秒内的行必须落在 [from, to) 里");
    }

    /// 未定价的行落 NULL 而不是 0：聚合若把 NULL 当 0，"成本 0" 和"没定价"就分不开了
    #[test]
    fn unpriced_rows_stay_null_across_aggregates() {
        let (pool, path) = test_db();
        record(&pool, &stat("k-priced", Some(0.0045))).expect("record priced");
        record(&pool, &stat("k-unpriced", None)).expect("record unpriced");

        // 明细页：整份筛选集的合计 + 覆盖率分子
        let page = list_requests(&pool, &RequestFilter::default()).expect("list");
        assert_eq!(page.total, 2);
        assert_eq!(page.priced_requests, 1);
        let total_cost = page.total_cost.expect("有一条已定价就该有合计");
        assert!((total_cost - 0.0045).abs() < 1e-12, "实际 {total_cost}");
        let mut costs: Vec<Option<f64>> = page.items.iter().map(|i| i.cost).collect();
        costs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        assert_eq!(costs[1], None, "未定价那行必须是 NULL，不是 0");

        // 模型汇总同样带出覆盖率分子
        let summaries = model_summaries(&pool, None, None).expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].priced_requests, 1);
        assert_eq!(summaries[0].requests, 2);
        assert!((summaries[0].total_cost.expect("sum") - 0.0045).abs() < 1e-12);

        // 全未定价的那个 Key：合计是 None（前端显示 —），覆盖率分子是 0
        let usage = api_key_usage(&pool, None, None).expect("key usage");
        let unpriced = usage.iter().find(|u| u.api_key_id == "k-unpriced").expect("row");
        assert_eq!(unpriced.total_cost, None);
        assert_eq!(unpriced.priced_requests, 0);
        assert_eq!(unpriced.requests, 1);

        // 单 Key 的模型维度拆分：合计与分子都要跟着走，否则弹窗里的成本列会骗人
        let priced_models = api_key_model_usage(&pool, "k-priced", None, None).expect("priced model usage");
        assert_eq!(priced_models.len(), 1);
        assert_eq!(priced_models[0].priced_requests, 1);
        assert!((priced_models[0].total_cost.expect("sum") - 0.0045).abs() < 1e-12);
        let unpriced_models = api_key_model_usage(&pool, "k-unpriced", None, None).expect("unpriced model usage");
        assert_eq!(unpriced_models[0].total_cost, None);
        assert_eq!(unpriced_models[0].priced_requests, 0);

        cleanup_db(&path);
    }

    /// 缓存价差是请求时冻结的金额：正负都按行相加，未定价行保持 NULL（不是 0）
    #[test]
    fn cache_savings_are_summed_and_stay_null_when_unknown() {
        let (pool, path) = test_db();
        let mut saved = stat("k", Some(0.01));
        saved.cache_savings = Some(0.002);
        saved.cache_read_tokens = Some(1000);
        record(&pool, &saved).expect("saved");
        let mut surcharge = stat("k", Some(0.02));
        surcharge.cache_savings = Some(-0.001);
        record(&pool, &surcharge).expect("surcharge");
        record(&pool, &stat("k", None)).expect("unknown");

        let ov = overview(&pool, 0, None, None).expect("overview");
        assert!((ov.total_cache_savings.expect("sum") - 0.001).abs() < 1e-12);
        assert_eq!(ov.total_cost, Some(0.03));

        let summaries = model_summaries(&pool, None, None).expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert!((summaries[0].cache_savings.expect("model sum") - 0.001).abs() < 1e-12);

        cleanup_db(&path);
    }

    /// 高峰状态只统计启用且基础价完整的模型；窗口覆盖全天时当前一定处于高峰
    #[test]
    fn overview_reports_peak_pricing_state() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO platforms (id,name,type,base_url,api_key,created_at,updated_at) VALUES ('pl','p','openai','http://x','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            ).expect("platform");
            let peak = r#"{"windows":[{"start":"00:00","end":"12:00"},{"start":"12:00","end":"00:00"}],"peak":{"input":2.0}}"#;
            // 新行的 status 是 serde 写出的带引号形式，旧行才是裸值：两种都要认
            conn.execute(
                "INSERT INTO models (id,platform_id,model_id,display_name,input_price,output_price,peak_pricing,status) \
                 VALUES ('m1','pl','m1','m1',1.0,2.0,?1,'\"Active\"'), \
                        ('m2','pl','m2','m2',1.0,2.0,?1,'\"Disabled\"'), \
                        ('m3','pl','m3','m3',NULL,2.0,?1,'\"Active\"')",
                rusqlite::params![peak],
            ).expect("models");
        }

        let ov = overview(&pool, 0, None, None).expect("overview");
        assert_eq!(ov.peak_priced_models, 1, "停用与半截价格的模型不算");
        assert_eq!(ov.peak_active_models, 1);
        assert!(ov.peak_next_at.is_some(), "全天窗口也应有下一次切换边界");

        cleanup_db(&path);
    }

    /// 表头排序：白名单里的列才生效，空值两个方向都沉底，未知列名回落默认顺序。
    #[test]
    fn list_requests_sorts_only_by_whitelisted_columns() {
        fn row(latency: i64, cost: Option<f64>, tokens: (i64, i64), created_at: &str) -> RequestStat {
            RequestStat {
                id: 0,
                proxy_id: "p1".into(), route_id: "r1".into(), backend_id: "b1".into(),
                status_code: 200, latency_ms: latency,
                token_input: Some(tokens.0), token_output: Some(tokens.1),
                error_type: None, created_at: created_at.into(),
                ttft_ms: None, cache_read_tokens: None, cache_write_tokens: None,
                is_stream: false, api_key_id: Some("k1".into()), cost,
                cache_savings: None,
                request_id: None, replayed_from: None,
                credits: None, path: None,
            }
        }
        let (pool, path) = test_db();
        record(&pool, &row(100, Some(0.002), (10, 5), "2026-01-01T00:00:01.000Z")).expect("fast");
        record(&pool, &row(5000, None, (1, 1), "2026-01-01T00:00:02.000Z")).expect("slow unpriced");
        record(&pool, &row(900, Some(0.010), (100, 200), "2026-01-01T00:00:03.000Z")).expect("mid");

        let latencies = |sort: &str, dir: Option<&str>| -> Vec<i64> {
            let filter = RequestFilter {
                sort: Some(sort.into()),
                dir: dir.map(str::to_string),
                ..Default::default()
            };
            list_requests(&pool, &filter).expect("list").items.iter().map(|i| i.latency_ms).collect()
        };
        assert_eq!(latencies("latency", None), vec![5000, 900, 100], "默认降序");
        assert_eq!(latencies("latency", Some("asc")), vec![100, 900, 5000], "点第二次翻向");

        // 成本：未定价的 NULL 在两个方向都排最后，升序时不会顶在最前面冒充"最便宜"
        let costs = |dir: Option<&str>| -> Vec<Option<f64>> {
            let filter = RequestFilter { sort: Some("cost".into()), dir: dir.map(str::to_string), ..Default::default() };
            list_requests(&pool, &filter).expect("list").items.iter().map(|i| i.cost).collect()
        };
        assert_eq!(costs(None), vec![Some(0.010), Some(0.002), None]);
        assert_eq!(costs(Some("asc")), vec![Some(0.002), Some(0.010), None]);

        // token 列排的是输入 + 输出之和，不是其中一侧
        assert_eq!(latencies("tokens", None), vec![900, 100, 5000]);

        // 不在白名单里的值（含拼出来的注入串）一律回到默认的时间倒序
        let injected = RequestFilter {
            sort: Some("rs.cost; DROP TABLE request_stats".into()),
            dir: Some("asc; --".into()),
            ..Default::default()
        };
        let page = list_requests(&pool, &injected).expect("injected sort");
        assert_eq!(page.items.iter().map(|i| i.latency_ms).collect::<Vec<_>>(), vec![900, 5000, 100]);
        assert_eq!(page.total, 3, "表还在，行也没少");

        cleanup_db(&path);
    }

    /// 导出与列表共用同一条 WHERE：筛过的才导出，且截断必须报出命中总数。
    #[test]
    fn export_honours_the_filter_and_reports_truncation() {
        let (pool, path) = test_db();
        for i in 0..5 {
            let mut row = stat(if i < 3 { "k-a" } else { "k-b" }, Some(0.001));
            row.latency_ms = 100 + i * 1000;
            record(&pool, &row).expect("record");
        }

        let (all, total) = export_requests(&pool, &RequestFilter::default(), 10).expect("export all");
        assert_eq!(total, 5);
        assert_eq!(all.len(), 5);
        // 倒序与列表一致：导出文件的头几行就是屏幕上看到的头几行
        assert_eq!(all[0].latency_ms, 4100);

        let (slow, slow_total) = export_requests(
            &pool,
            &RequestFilter {
                api_key: Some("k-b".into()),
                latency_min: Some(2000),
                ..Default::default()
            },
            10,
        )
        .expect("export filtered");
        assert_eq!(slow_total, 2, "k-a 里也有一条 ≥2000，但 Key 条件先筛掉了");
        assert_eq!(slow.len(), 2);

        let (capped, total) = export_requests(&pool, &RequestFilter::default(), 2).expect("export capped");
        assert_eq!(capped.len(), 2);
        assert_eq!(total, 5, "总数是命中数而不是给出行数——界面靠它说明只导出了前几条");

        cleanup_db(&path);
    }

    fn attempt(request_id: &str, no: i64, status: Option<i64>) -> RequestAttempt {
        RequestAttempt {
            id: 0,
            request_id: request_id.into(),
            attempt_no: no,
            backend_id: Some(format!("b{no}")),
            platform_id: None,
            model_id: Some("glm-4".into()),
            status_code: status,
            error_type: if status.is_some_and(|s| s >= 400) { Some("UpstreamError".into()) } else { None },
            error_message: status.filter(|s| *s >= 400).map(|s| format!("upstream {s}")),
            latency_ms: Some(no * 10),
            started_at: "2026-01-01T00:00:00.000Z".into(),
            forward_url: None,
            nonce: None,
            platform_name: None,
        }
    }

    /// 轨迹要能还原"依次试过哪些后端"：倒序写入也得按 attempt_no 升序读出，
    /// 连接失败那种没有状态码的行不能被判成成功
    #[test]
    fn request_trace_lists_attempts_in_order() {
        let (pool, path) = test_db();
        // 旧行（V16 之前写入，没有 request_id）与新行并存
        record(&pool, &stat("k0", None)).expect("record legacy");
        let mut traced = stat("k1", None);
        traced.request_id = Some("req-1".into());
        record(&pool, &traced).expect("record traced");

        // 第 1 次连接失败（无状态码），第 2 次上游 503，第 3 次成功
        record_attempt(&pool, &attempt("req-1", 3, Some(200))).expect("a3");
        record_attempt(&pool, &attempt("req-1", 1, None)).expect("a1");
        record_attempt(&pool, &attempt("req-1", 2, Some(503))).expect("a2");

        let traced_id = list_requests(&pool, &RequestFilter::default()).expect("list")
            .items.into_iter().find(|i| i.request_id.is_some()).expect("traced row").id;
        let trace = request_trace(&pool, traced_id).expect("trace");
        assert_eq!(trace.request_id.as_deref(), Some("req-1"));
        let nums: Vec<i64> = trace.attempts.iter().map(|a| a.attempt_no).collect();
        assert_eq!(nums, vec![1, 2, 3]);
        assert_eq!(trace.attempts[0].status_code, None, "连接失败那次不该有状态码");
        assert_eq!(trace.attempts[2].status_code, Some(200));

        // request_id 为 NULL 的旧行：空轨迹而不是编造链路
        let legacy_id = list_requests(&pool, &RequestFilter::default()).expect("list")
            .items.into_iter().find(|i| i.request_id.is_none()).expect("legacy row").id;
        let legacy = request_trace(&pool, legacy_id).expect("legacy trace");
        assert_eq!(legacy.request_id, None);
        assert!(legacy.attempts.is_empty());

        // 明细行本身不存在 → 404
        assert!(matches!(
            request_trace(&pool, 999_999),
            Err(crate::error::AppError::NotFound(_))
        ));

        // 明细行被清掉时轨迹行跟着走，否则 request_attempts 只增不减
        // 先预演：报出的行数要和真删的一致，且库里一行不少
        let preview = cleanup_dangling(&pool, true).expect("preview cleanup");
        assert_eq!(preview.deleted, 2, "预演数出的悬挂行");
        assert_eq!(preview.orphan_attempts, 3);
        assert!(preview.dry_run);
        assert_eq!(list_requests(&pool, &RequestFilter::default()).expect("still there").total, 2);
        let removed = cleanup_dangling(&pool, false).expect("cleanup dangling");
        assert_eq!(removed.deleted, 2, "proxy/route/backend 都不存在 → 两条悬挂");
        assert_eq!(removed.orphan_attempts, 3);
        assert!(matches!(
            request_trace(&pool, traced_id),
            Err(crate::error::AppError::NotFound(_))
        ));

        cleanup_db(&path);
    }

    /// 尝试次数进列表：有 `request_id` 的行数出真实次数，V16 之前的旧行是 NULL 而不是 0
    /// （0 次尝试的请求根本不存在），按次数排序时旧行两个方向都沉底。
    #[test]
    fn list_requests_carries_the_attempt_count() {
        let (pool, path) = test_db();
        record(&pool, &stat("k0", None)).expect("legacy row without request_id");
        let mut once = stat("k1", None);
        once.request_id = Some("req-once".into());
        record(&pool, &once).expect("一次成功");
        record_attempt(&pool, &attempt("req-once", 1, Some(200))).expect("a1");
        let mut retried = stat("k1", None);
        retried.request_id = Some("req-retried".into());
        retried.latency_ms = 700;
        record(&pool, &retried).expect("重试后成功");
        record_attempt(&pool, &attempt("req-retried", 1, Some(503))).expect("a1");
        record_attempt(&pool, &attempt("req-retried", 2, Some(200))).expect("a2");
        // 明细行落了库、尝试行没落库（写入前进程退出）：如实报 0，不编造成 1
        let mut lost = stat("k1", None);
        lost.request_id = Some("req-lost".into());
        record(&pool, &lost).expect("尝试明细丢失");

        let counts = |dir: Option<&str>| -> Vec<Option<i64>> {
            let filter = RequestFilter {
                sort: Some("attempts".into()),
                dir: dir.map(str::to_string),
                ..Default::default()
            };
            list_requests(&pool, &filter).expect("list").items.iter().map(|i| i.attempts).collect()
        };
        assert_eq!(counts(None), vec![Some(2), Some(1), Some(0), None], "降序：重试过的排最前");
        assert_eq!(counts(Some("asc")), vec![Some(0), Some(1), Some(2), None], "旧行 NULL 沉底，不冒充 0 次");

        // 导出与列表共用 SELECT 与映射：CSV 里的次数和屏幕上的是同一个数
        let (rows, _) = export_requests(&pool, &RequestFilter::default(), 10).expect("export");
        assert_eq!(rows.iter().find(|i| i.request_id.is_none()).expect("legacy row").attempts, None);
        let retried = rows.iter().find(|i| i.request_id.as_deref() == Some("req-retried")).expect("retried row");
        assert_eq!(retried.attempts, Some(2));

        cleanup_db(&path);
    }

    /// 按年龄清理：先预演报数，再执行；轨迹行跟着所属明细行走，不留在库里当孤儿
    #[test]
    fn purge_by_age_previews_then_removes_rows_with_their_attempts() {
        let (pool, path) = test_db();
        // 老行：fixture 的 created_at 是 2026-01-01，远超 90 天
        let mut old = stat("k1", None);
        old.request_id = Some("req-old".into());
        record(&pool, &old).expect("old row");
        record_attempt(&pool, &attempt("req-old", 1, Some(500))).expect("a1");
        record_attempt(&pool, &attempt("req-old", 2, Some(200))).expect("a2");
        // 新行：3 天前，带一次尝试
        let mut fresh = stat("k1", None);
        fresh.created_at = (chrono::Utc::now() - chrono::Duration::days(3))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        fresh.request_id = Some("req-fresh".into());
        record(&pool, &fresh).expect("fresh row");
        record_attempt(&pool, &attempt("req-fresh", 1, None)).expect("a3");

        let preview = purge_older_than(&pool, 90, true).expect("preview purge");
        assert_eq!(preview.rows, 1, "只有老行落在 90 天之外");
        assert_eq!(preview.attempts, 2, "预演的轨迹行数按删除后的口径：老行名下那两条");
        assert!(preview.dry_run);
        assert_eq!(list_requests(&pool, &RequestFilter::default()).expect("list").total, 2, "预演不该删掉任何行");

        let done = purge_older_than(&pool, 90, false).expect("purge");
        assert_eq!(done.rows, 1);
        assert_eq!(done.attempts, 2);
        assert_eq!(done.days, 90);
        let left = list_requests(&pool, &RequestFilter::default()).expect("list");
        assert_eq!(left.total, 1);
        let trace = request_trace(&pool, left.items[0].id).expect("fresh trace");
        assert_eq!(trace.attempts.len(), 1, "新行的尝试轨迹不受影响");

        // 天数下限：0 天等于"删掉全部历史"，不该由一个空输入触发
        assert!(matches!(
            purge_older_than(&pool, 0, true),
            Err(crate::error::AppError::BadRequest(_))
        ));

        let info = db_info(&pool, &path).expect("db info");
        assert_eq!(info.request_rows, 1);
        assert_eq!(info.attempt_rows, 1);
        assert!(info.size_bytes > 0, "文件大小给设置页显示");
        assert_eq!(info.oldest_request_at.as_deref(), Some(fresh.created_at.as_str()));
        assert_eq!(info.newest_request_at.as_deref(), Some(fresh.created_at.as_str()));
        assert_eq!(info.dangling_rows, 1, "fixture 里没有 p1/r1/b1，剩下的就是悬挂行");

        cleanup_db(&path);
    }

    /// 正文单独一张表，但生命周期跟着明细行走：列表能看出哪行有正文、重放按明细行 ID
    /// 取回当初那份请求，过期清理与悬挂清理都把正文一起带走——明细行都没了，
    /// 留在库里的提示词只会让保留期形同虚设。
    #[test]
    fn stored_bodies_follow_their_request_rows() {
        let (pool, path) = test_db();
        let prompt = r#"{"model":"gpt-x","messages":[{"role":"user","content":"hi"}]}"#;
        let mut with_body = stat("k1", None);
        with_body.request_id = Some("req-body".into());
        record(&pool, &with_body).expect("row with body");
        record_body(&pool, &RequestBody {
            request_id: "req-body".into(),
            protocol: "openai".into(),
            body: prompt.into(),
            bytes: prompt.len() as i64,
            truncated: false,
            created_at: with_body.created_at.clone(),
        }).expect("record body");
        // 采集没开 / 早于 V21：明细行照记，只是没有正文可重放
        let mut without_body = stat("k1", None);
        without_body.request_id = Some("req-nobody".into());
        record(&pool, &without_body).expect("row without body");

        let page = list_requests(&pool, &RequestFilter::default()).expect("list");
        let item = page.items.iter().find(|i| i.request_id.as_deref() == Some("req-body")).expect("body row");
        assert!(item.has_body, "存过正文的行要在列表里露出重放入口");
        let nobody = page.items.iter().find(|i| i.request_id.as_deref() == Some("req-nobody")).expect("no-body row");
        assert!(!nobody.has_body, "没有正文行的重放入口不该出现");
        assert_eq!(item.replayed_from, None, "原始请求不是重放出来的");

        let stored = body_for_stat(&pool, item.id).expect("fetch body").expect("有正文");
        assert_eq!(stored.body, prompt, "重放要用的是调用方当初那份原文，不是转好的上游请求");
        assert_eq!(stored.protocol, "openai");
        assert!(body_for_stat(&pool, nobody.id).expect("fetch body").is_none(), "没存过就是没有");

        // 重放出来的那条：replayed_from 指回来源明细行，界面上才分得清哪条是重放
        let mut replayed = stat("k1", None);
        replayed.request_id = Some("req-replay".into());
        replayed.replayed_from = Some(item.id);
        record(&pool, &replayed).expect("replayed row");
        let again = list_requests(&pool, &RequestFilter::default()).expect("list again");
        let replay_row = again.items.iter().find(|i| i.request_id.as_deref() == Some("req-replay")).expect("replay row");
        assert_eq!(replay_row.replayed_from, Some(item.id));
        assert!(!replay_row.has_body, "重放自身不另存一份正文");

        let info = db_info(&pool, &path).expect("db info");
        assert_eq!(info.body_rows, 1);
        assert_eq!(info.body_bytes, prompt.len() as i64);

        // 关掉开关后的"抹掉正文"：正文表清空，明细行一条不少——它们本来就不含正文，
        // 删掉它们只会让报表凭空缺一块
        assert_eq!(delete_all_bodies(&pool).expect("delete bodies").deleted, 1);
        let after_wipe = db_info(&pool, &path).expect("db info after wipe");
        assert_eq!(after_wipe.body_rows, 0);
        assert_eq!(after_wipe.request_rows, 3, "清空正文不该连带删掉明细行");
        record_body(&pool, &RequestBody {
            request_id: "req-body".into(),
            protocol: "openai".into(),
            body: prompt.into(),
            bytes: prompt.len() as i64,
            truncated: false,
            created_at: with_body.created_at.clone(),
        }).expect("record body again");

        // 按年龄清理：正文按所属明细行的年龄一起删，预演先报数
        let preview = purge_older_than(&pool, 90, true).expect("preview purge");
        assert_eq!(preview.bodies, 1, "fixture 的 created_at 是 2026-01-01，正文落在 90 天之外");
        assert_eq!(body_for_stat(&pool, item.id).expect("still there").expect("预演不动库").body, prompt);
        let done = purge_older_than(&pool, 90, false).expect("purge");
        assert_eq!(done.bodies, 1);
        assert!(body_for_stat(&pool, item.id).expect("after purge").is_none(), "明细行删了，正文不该留在库里");

        // 悬挂清理：明细行被删掉后，正文也当孤儿扫掉
        record_body(&pool, &RequestBody {
            request_id: "req-gone".into(),
            protocol: "openai".into(),
            body: prompt.into(),
            bytes: prompt.len() as i64,
            truncated: false,
            created_at: with_body.created_at.clone(),
        }).expect("record orphan body");
        let cleaned = cleanup_dangling(&pool, false).expect("cleanup");
        assert_eq!(cleaned.orphan_bodies, 1, "没有明细行对应的正文要清掉");
        assert_eq!(db_info(&pool, &path).expect("db info after cleanup").body_rows, 0);

        cleanup_db(&path);
    }

    /// 截断的请求正文仍要在详情里看得到，但不能出现在重放入口（前缀不是当初那份请求）
    #[test]
    fn truncated_request_body_is_visible_but_not_replayable() {
        let (pool, path) = test_db();
        let prefix = r#"{"model":"gpt-x","messages":[{"role":"user","content":"partial"#;
        let mut row = stat("k1", None);
        row.request_id = Some("req-trunc".into());
        record(&pool, &row).expect("record");
        let stat_id: i64 = pool.get().unwrap().query_row(
            "SELECT id FROM request_stats WHERE request_id = 'req-trunc'",
            [], |r| r.get(0),
        ).unwrap();
        record_body(&pool, &RequestBody {
            request_id: "req-trunc".into(),
            protocol: "openai".into(),
            body: prefix.into(),
            bytes: 700_000,
            truncated: true,
            created_at: row.created_at.clone(),
        }).expect("record truncated body");

        let payload = payload_for_stat(&pool, stat_id).expect("payload");
        assert!(payload.request.body_captured, "截断也要算已采集，否则 UI 会退回未采集");
        assert!(payload.request.body_truncated);
        assert_eq!(payload.request.body.as_deref(), Some(prefix));
        assert_eq!(payload.request.bytes, 700_000, "bytes 是原始长度，不是前缀长度");

        let stored = body_for_stat(&pool, stat_id).expect("body").expect("prefix exists");
        assert!(stored.truncated, "重放前必须能看出这是截断的");

        let page = list_requests(&pool, &RequestFilter::default()).expect("list");
        let item = page.items.iter().find(|i| i.request_id.as_deref() == Some("req-trunc")).expect("row");
        assert!(!item.has_body, "截断正文不能露出重放入口");

        cleanup_db(&path);
    }

    /// 删除确认里的条数必须等于真正删掉的行数：同一个 proxy 过滤条件，别的模型的行不受影响
    #[test]
    fn history_impact_counts_exactly_what_delete_by_proxy_removes() {
        let (pool, path) = test_db();
        // p1：一条有轨迹（2 次尝试）+ 一条 V16 之前的旧行（request_id 为 NULL，没有轨迹）
        let mut traced = stat("k1", None);
        traced.request_id = Some("req-1".into());
        record(&pool, &traced).expect("record traced");
        record(&pool, &stat("k1", None)).expect("record legacy");
        record_attempt(&pool, &attempt("req-1", 1, None)).expect("a1");
        record_attempt(&pool, &attempt("req-1", 2, Some(200))).expect("a2");
        // p2：也有历史和一个轨迹行，删 p1 不该动它
        let mut other = stat("k2", None);
        other.proxy_id = "p2".into();
        other.request_id = Some("req-2".into());
        record(&pool, &other).expect("record other");
        record_attempt(&pool, &attempt("req-2", 1, Some(200))).expect("other attempt");

        let impact = proxy_history_impact(&pool, "p1").expect("impact p1");
        assert_eq!(impact.requests, 2);
        assert_eq!(impact.attempts, 2, "旧行没有轨迹，不该被算进来");

        delete_by_proxy(&pool, "p1").expect("delete p1");
        let after = proxy_history_impact(&pool, "p1").expect("impact after");
        assert_eq!(after.requests, 0);
        assert_eq!(after.attempts, 0, "轨迹跟着走，否则 request_attempts 只增不减");

        let other_after = proxy_history_impact(&pool, "p2").expect("impact p2");
        assert_eq!((other_after.requests, other_after.attempts), (1, 1));

        // 不存在的模型：0 条而不是报错，确认框照样能说话
        let none = proxy_history_impact(&pool, "no-such-proxy").expect("impact missing");
        assert_eq!((none.requests, none.attempts), (0, 0));

        cleanup_db(&path);
    }

    /// 请求头/响应正文与请求明细同生共死：详情能读出来，清空与按年龄清理都要覆盖它们。
    #[test]
    fn payload_rows_are_read_and_deleted_with_the_request() {
        let (pool, path) = test_db();
        let mut row = stat("k1", None);
        row.request_id = Some("req-payload".into());
        row.created_at = "2020-01-01T00:00:00+00:00".into();
        record(&pool, &row).expect("record");

        let stat_id: i64 = pool.get().unwrap().query_row(
            "SELECT id FROM request_stats WHERE request_id = 'req-payload'",
            [],
            |r| r.get(0),
        ).unwrap();
        record_body(&pool, &RequestBody {
            request_id: "req-payload".into(), protocol: "openai".into(),
            body: "{\"model\":\"m\"}".into(), bytes: 15, truncated: false,
            created_at: "2020-01-01T00:00:00+00:00".into(),
        }).expect("record body");
        record_headers(&pool, &RequestHeaders {
            request_id: "req-payload".into(),
            headers: "{\"content-type\":\"application/json\"}".into(),
            bytes: 34, truncated: false,
            created_at: "2020-01-01T00:00:00+00:00".into(),
        }).expect("record headers");
        record_response_body(&pool, &ResponseBody {
            request_id: "req-payload".into(), protocol: "openai".into(), status_code: 200,
            content_type: Some("application/json".into()), body: "{\"ok\":true}".into(),
            bytes: 11, truncated: false,
            created_at: "2020-01-01T00:00:00+00:00".into(),
        }).expect("record response");

        let payload = payload_for_stat(&pool, stat_id).expect("payload");
        assert!(payload.request.headers_captured);
        assert_eq!(payload.request.headers.as_ref().unwrap()["content-type"], "application/json");
        assert!(payload.request.body_captured);
        assert_eq!(payload.request.body.as_deref(), Some("{\"model\":\"m\"}"));
        assert!(payload.response.captured);
        assert_eq!(payload.response.status_code, Some(200));
        assert_eq!(payload.response.body.as_deref(), Some("{\"ok\":true}"));

        // 清空正文：三张表都算，统计行保留
        assert_eq!(delete_all_bodies(&pool).unwrap().deleted, 3);
        let wiped = payload_for_stat(&pool, stat_id).expect("payload after wipe");
        assert!(!wiped.request.headers_captured && !wiped.request.body_captured && !wiped.response.captured);

        // 再放一份，按年龄清理时跟着明细一起走
        record_headers(&pool, &RequestHeaders {
            request_id: "req-payload".into(), headers: "{}".into(), bytes: 2, truncated: false,
            created_at: "2020-01-01T00:00:00+00:00".into(),
        }).unwrap();
        record_response_body(&pool, &ResponseBody {
            request_id: "req-payload".into(), protocol: "openai".into(), status_code: 200,
            content_type: None, body: "done".into(), bytes: 4, truncated: false,
            created_at: "2020-01-01T00:00:00+00:00".into(),
        }).unwrap();
        let purged = purge_older_than(&pool, 1, false).expect("purge");
        assert_eq!(purged.rows, 1);
        assert_eq!(purged.headers, 1);
        assert_eq!(purged.responses, 1);

        cleanup_db(&path);
    }

    /// 「清空已存内容」必须让库文件真的变小：DELETE 只标空闲页，提示词字节还在盘上，
    /// 用户看到体积不变就会认为没清掉。这条把"删完即压实"钉住。
    #[test]
    fn wiping_bodies_reclaims_disk_space() {
        let (pool, path) = test_db();
        let payload = "P".repeat(64 * 1024);
        for i in 0..40 {
            let request_id = format!("req-bulk-{i}");
            record_body(&pool, &RequestBody {
                request_id: request_id.clone(), protocol: "openai".into(),
                body: payload.clone(), bytes: payload.len() as i64, truncated: false,
                created_at: "2026-01-01T00:00:00+00:00".into(),
            }).unwrap();
        }
        let conn = pool.get().unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(conn);
        let size_before = std::fs::metadata(&path).unwrap().len();
        assert!(size_before > 64 * 1024, "fixture 应该先写入可观的正文");

        let result = delete_all_bodies(&pool).unwrap();
        assert_eq!(result.deleted, 40);
        assert!(result.compacted, "单连接测试库不该有读锁挡住 VACUUM");
        let size_after = std::fs::metadata(&path).unwrap().len();
        assert!(size_after < size_before / 2, "库文件应明显缩小: {size_before} -> {size_after}");
        assert_eq!(result.reclaimed_bytes, (size_before - size_after) as i64);

        cleanup_db(&path);
    }

    /// 大屏改用批量取数（2026-11）后的口径钉子：**批量与逐个必须逐字段相等**。
    /// 两边是两条独立 SQL——批量从实体表 LEFT JOIN 事实表，逐个从事实表反查实体——
    /// 任何口径漂移都会直接变成屏上数字变了，而且只会在使用者眼皮底下变。
    /// 覆盖：同平台多后端、有/无定价、token 缺失、失败请求、零流量实体、
    /// 悬挂 `backend_id`、以及 `proxy_id` 指向已删网关的悬挂行。
    #[test]
    fn bulk_stats_match_the_per_entity_queries_field_by_field() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO platforms (id,name,type,base_url,api_key,created_at,updated_at)
                 VALUES ('pl-a','平台A','openai','http://a','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z'),
                        ('pl-b','平台B','openai','http://b','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z'),
                        ('pl-idle','平台C','openai','http://c','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            ).expect("platforms");
            conn.execute(
                "INSERT INTO proxies (id,name,created_at) VALUES ('px-a','网关A','2026-01-01T00:00:00Z'),
                                                 ('px-b','网关B','2026-01-01T00:00:00Z'),
                                                 ('px-idle','网关C','2026-01-01T00:00:00Z')",
                [],
            ).expect("proxies");
            conn.execute("INSERT INTO routes (id,proxy_id) VALUES ('rt-a','px-a'),('rt-b','px-b')", []).expect("routes");
            conn.execute(
                "INSERT INTO backends (id,route_id,platform_id,model_id,weight,priority)
                 VALUES ('b-a1','rt-a','pl-a','m',1,0),('b-a2','rt-a','pl-a','m',1,0),('b-b1','rt-b','pl-b','m',1,0)",
                [],
            ).expect("backends");
        }

        // (proxy, backend, status, latency, 输入, 输出, 成本)
        let rows: Vec<(&str, &str, i32, i64, Option<i64>, Option<i64>, Option<f64>)> = vec![
            ("px-a", "b-a1", 200, 100, Some(1000), Some(500), Some(0.02)),
            ("px-a", "b-a1", 200, 300, Some(200), Some(100), None),
            ("px-a", "b-a2", 500, 900, None, None, None),
            ("px-b", "b-b1", 200, 50, Some(4000), Some(2000), Some(0.5)),
            ("px-b", "b-b1", 429, 10, Some(10), Some(0), None),
            // 悬挂 proxy_id：后端还在，网关已删 → 算平台的，不算任何网关
            ("px-ghost", "b-a1", 200, 120, Some(7), Some(7), Some(0.01)),
            // 悬挂 backend_id：网关还在，后端已删 → 算网关的，归 unknown 平台
            ("px-a", "b-ghost", 200, 80, Some(9), Some(9), Some(0.03)),
        ];
        for (index, (proxy_id, backend_id, status, latency, tin, tout, cost)) in rows.into_iter().enumerate() {
            let mut row = stat("k1", cost);
            row.proxy_id = proxy_id.into();
            row.route_id = if proxy_id == "px-b" { "rt-b".into() } else { "rt-a".into() };
            row.backend_id = backend_id.into();
            row.status_code = status;
            row.latency_ms = latency;
            row.token_input = tin;
            row.token_output = tout;
            row.created_at = format!("2026-01-01T{:02}:00:00Z", 10 + index);
            record(&pool, &row).expect("stat row");
        }

        let from = "2026-01-01T00:00:00Z";
        let to = "2026-02-01T00:00:00Z";

        let bulk_proxy = proxy_stats_all(&pool, Some(from), Some(to)).expect("bulk proxy");
        assert_eq!(bulk_proxy.len(), 3, "零流量的网关也要有自己的行，否则界面上是「没统计」而不是 0");
        for bulk in &bulk_proxy {
            let single = proxy_stats(&pool, &bulk.proxy_id, Some(from), Some(to)).expect("single proxy");
            assert_eq!(bulk.proxy_name, single.proxy_name, "网关名");
            assert_eq!(bulk.total_requests, single.total_requests, "{} 请求数", bulk.proxy_id);
            assert_eq!(bulk.requests_today, single.requests_today, "{} 今日", bulk.proxy_id);
            assert_eq!(bulk.total_token_input, single.total_token_input, "{} 输入", bulk.proxy_id);
            assert_eq!(bulk.total_token_output, single.total_token_output, "{} 输出", bulk.proxy_id);
            assert_eq!(bulk.priced_requests, single.priced_requests, "{} 已定价", bulk.proxy_id);
            assert_eq!(bulk.total_cost, single.total_cost, "{} 成本", bulk.proxy_id);
            assert!(
                (bulk.success_rate - single.success_rate).abs() < 1e-9,
                "{} 成功率 {:?} ≠ {:?}", bulk.proxy_id, bulk.success_rate, single.success_rate
            );
            assert!(
                (bulk.avg_latency_ms - single.avg_latency_ms).abs() < 1e-9,
                "{} 延迟 {:?} ≠ {:?}", bulk.proxy_id, bulk.avg_latency_ms, single.avg_latency_ms
            );
        }

        let bulk_platform = platform_stats_all(&pool, Some(from), Some(to)).expect("bulk platform");
        assert_eq!(bulk_platform.len(), 3, "零流量/没有后端的平台也要有自己的行");
        for bulk in &bulk_platform {
            let single = platform_stats(&pool, &bulk.platform_id, Some(from), Some(to)).expect("single platform");
            assert_eq!(bulk.platform_name, single.platform_name, "平台名");
            assert_eq!(bulk.total_requests, single.total_requests, "{} 请求数", bulk.platform_id);
            assert_eq!(bulk.total_token_input, single.total_token_input, "{} 输入", bulk.platform_id);
            assert_eq!(bulk.total_token_output, single.total_token_output, "{} 输出", bulk.platform_id);
            assert_eq!(bulk.priced_requests, single.priced_requests, "{} 已定价", bulk.platform_id);
            assert_eq!(bulk.total_cost, single.total_cost, "{} 成本", bulk.platform_id);
            assert!(
                (bulk.success_rate - single.success_rate).abs() < 1e-9,
                "{} 成功率 {:?} ≠ {:?}", bulk.platform_id, bulk.success_rate, single.success_rate
            );
            assert!(
                (bulk.avg_latency_ms - single.avg_latency_ms).abs() < 1e-9,
                "{} 延迟 {:?} ≠ {:?}", bulk.platform_id, bulk.avg_latency_ms, single.avg_latency_ms
            );
        }

        // 钉住这份数据自己的形状：否则上面的「相等」可能是两个 0 比出来的假绿
        let px_a = bulk_proxy.iter().find(|p| p.proxy_id == "px-a").expect("px-a");
        assert_eq!(px_a.total_requests, 4, "b-a1×2 + b-a2×1 + 悬挂后端×1");
        assert_eq!(px_a.total_cost, Some(0.05), "两条已定价：0.02 + 0.03");
        assert_eq!(px_a.priced_requests, 2);
        let pl_a = bulk_platform.iter().find(|p| p.platform_id == "pl-a").expect("pl-a");
        assert_eq!(pl_a.total_requests, 4, "b-a1×3（含 px-ghost 那笔）+ b-a2×1");
        assert_eq!(pl_a.priced_requests, 2);
        assert_eq!(pl_a.total_cost, Some(0.03), "0.02 + 0.01；b-ghost 那笔 0.03 归 unknown 平台，不进 pl-a");
        let pl_idle = bulk_platform.iter().find(|p| p.platform_id == "pl-idle").expect("pl-idle");
        assert_eq!((pl_idle.total_requests, pl_idle.total_cost, pl_idle.success_rate), (0, None, 0.0));

        cleanup_db(&path);
    }

    /// D 读端三段 UNION 的随机对等性：随机时间戳摆布在多个小时桶与边界上，
    /// `query_metrics` 的输出与「只查明细手工聚合同样字段」必须逐字段全等——
    /// 这是验证核心段 + 边界段不重不漏的最直接证据（任一段边界错位就会差请求数或成本）。
    #[test]
    fn query_metrics_equals_pure_detail_aggregation_on_random_spread() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            use super::query_metrics;

            // 用固定种子保证可复现；时间跨 3 个整点小时（UTC 视角），本地时区可能偏移，
            // 但无论偏移到哪个桶，三段 UNION 的总和都必须等于明细全图。
            let mut seed = 0x5DEECE66Du64;
            let mut next = move || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 33) as u32
            };
            let mut rows = Vec::new();
            for i in 0..120 {
                let secs: u32 = next() % 3 * 3600 + next() % 3600; // 分布在 00:00–03:00 UTC
                let ts = format!("2026-01-01T{:02}:{:02}:{:02}Z", secs / 3600, (secs % 3600) / 60, secs % 60);
                let mut s = stat(&format!("k{i}"), if i % 3 == 0 { Some(0.001 * f64::from(i)) } else { None });
                s.created_at = ts;
                s.status_code = if i % 7 == 0 { 500 } else { 200 };
                s.latency_ms = i64::from(next() % 3000);
                s.ttft_ms = if i % 5 == 0 { Some(i64::from(next() % 500)) } else { None };
                s.is_stream = i % 2 == 0;
                if i % 11 == 0 {
                    s.cache_savings = Some(0.0001 * f64::from(i));
                }
                rows.push(s);
            }
            super::record_batch(&pool, &rows, &[], &[], &[], &[]).expect("batch");

            // 范围：覆盖 00:00Z 到 03:00Z 前 30 秒（跨整点小时，确保核心段 + 两个边界段都激活）
            let got = query_metrics(&conn, Some("2026-01-01T00:00:00Z"), Some("2026-01-01T03:00:30Z"));

            // 对照：纯明细聚合，逐字段同口径
            let direct: (i64, i64, i64, i64, i64, f64, i64, f64, Option<f64>, i64, Option<f64>, i64) = conn
                .query_row(
                    "SELECT
                        COUNT(*),
                        SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 0 ELSE 1 END),
                        SUM(is_stream),
                        COALESCE(SUM(token_input),0), COALESCE(SUM(token_output),0),
                        COALESCE(AVG(latency_ms),0),
                        SUM(CASE WHEN ttft_ms IS NOT NULL THEN 1 ELSE 0 END),
                        COALESCE(AVG(ttft_ms),0),
                        SUM(cost), COUNT(cost),
                        SUM(cache_savings), COUNT(cache_savings)
                     FROM request_stats
                     WHERE created_at >= ?1 AND created_at < ?2",
                    rusqlite::params!["2026-01-01T00:00:00Z", "2026-01-01T03:00:30Z"],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                        r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?)),
                )
                .expect("direct");

            // 手工重算 speed：SUMQ / SUMG 与逐行 ROUND 不同，不能拿 SQL 里那个 SUM(逐行除法) 当对照
            // —— 直接按口径手工算：总输出 / 总生成时长
            let (direct_requests, direct_failed, _direct_stream, direct_tin, direct_tout, direct_avg_lat,
                direct_ttft_samples, direct_avg_ttft, direct_cost_sum, direct_priced, direct_savings_sum,
                direct_savings_samples) = direct;
            let speed_denominator: i64 = conn.query_row(
                "SELECT COALESCE(SUM(latency_ms),0) - COALESCE(SUM(ttft_ms),0) FROM request_stats
                 WHERE created_at >= ?1 AND created_at < ?2",
                rusqlite::params!["2026-01-01T00:00:00Z", "2026-01-01T03:00:30Z"],
                |r| r.get::<_, i64>(0),
            ).expect("speed denom");

            assert_eq!(got.requests, direct_requests, "请求数");
            assert_eq!(got.success, direct_requests - direct_failed, "成功数");
            assert_eq!(got.token_input, direct_tin, "输入 token");
            assert_eq!(got.token_output, direct_tout, "输出 token");
            assert_eq!(got.cache_read_tokens, 0);
            assert_eq!(got.cache_write_tokens, 0);
            assert!((got.avg_latency_ms - direct_avg_lat).abs() < 1e-9, "平均延迟");
            assert_eq!(got.priced_requests, direct_priced, "已定价");
            // 浮点对照用容差：hourly 分桶累加与明细一次 SUM 的加法顺序不同，末位可能差 1 ulp
            let both_some = |a: Option<f64>, b: Option<f64>| a.zip(b).map(|(x, y)| (x - y).abs() < 1e-9).unwrap_or(false);
            assert!(
                both_some(got.cost, if direct_priced > 0 { direct_cost_sum } else { None }),
                "成本（NULL 语义）：got {:?} ≠ 明细 {:?}",
                got.cost,
                if direct_priced > 0 { direct_cost_sum } else { None }
            );
            assert!(
                both_some(got.cache_savings, if direct_savings_samples > 0 { direct_savings_sum } else { None }),
                "价差（NULL 语义）：got {:?} ≠ 明细 {:?}",
                got.cache_savings,
                if direct_savings_samples > 0 { direct_savings_sum } else { None }
            );
            // ttft 平均：样本数相等 + 总和要求相等
            let got_ttft_total = got.avg_ttft_ms * got.ttft_samples as f64;
            let direct_ttft_total = direct_avg_ttft * direct_ttft_samples as f64;
            assert_eq!(got.ttft_samples, direct_ttft_samples, "ttft 样本数");
            assert!((got_ttft_total - direct_ttft_total).abs() < 1e-9, "ttft 总和");
            // speed：总输出 ×1000 / 总生成时长（SQL 里同口径）
            let expected_speed = if speed_denominator > 0 {
                direct_tout as f64 * 1000.0 / speed_denominator as f64
            } else {
                0.0
            };
            assert!((got.avg_speed_tps - expected_speed).abs() < 1e-9, "生成速度");

            // 空范围（from >= to）→ 无核心段，整体明细回退，返回 0 而非崩溃
            let empty_range = query_metrics(&conn, Some("2026-01-01T00:00:00Z"), Some("2026-01-01T00:00:00Z"));
            assert_eq!(empty_range.requests, 0);
            assert_eq!(empty_range.cost, None);
        }
        cleanup_db(&path);
    }

    /// hour 粒度读 hourly 表后必须与明细逐字段一致——这是墙的默认视图，
    /// 口径漂移会直接变成屏上数字变了。用随机散布的跨整点数据同时激活
    /// hourly 核心段边界（bucket 比较）与明细段的 UTC created_at 比较，
    /// 再钉住 fresh_input / cost / savings 的 NULL 语义。
    #[test]
    fn model_timeseries_hourly_equals_pure_detail_on_random_spread() {
        let (pool, path) = test_db();
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO platforms (id,name,type,base_url,api_key,created_at,updated_at) VALUES ('pl','p','openai','http://x','k','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            ).expect("platform");
            conn.execute(
                "INSERT INTO proxies (id,name,created_at) VALUES ('px','px','2026-01-01T00:00:00Z')",
                [],
            ).expect("proxy");
            conn.execute("INSERT INTO routes (id,proxy_id) VALUES ('rt','px')", []).expect("route");
            conn.execute(
                "INSERT INTO backends (id,route_id,platform_id,model_id,weight,priority) VALUES
                 ('b-a','rt','pl','model-a',1,0),('b-b','rt','pl','model-b',1,0)",
                [],
            ).expect("backends");

            // 随机散布：00:00Z ~ 03:59Z，跨四个整点；部分行定价、部分行无 token/ttft
            let mut rows = Vec::new();
            let mut seed = 42u64;
            let mut next = move || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 33) % 100
            };
            for i in 0..60 {
                let hour = i % 4;
                let minute = next() % 60;
                let sec = next() % 60;
                let model = if next() % 2 == 0 { "b-a" } else { "b-b" };
                let priced = next() % 3 == 0;
                let stream = next() % 2 == 0;
                let mut row = stat(&format!("k{i}"), if priced { Some(0.01 + 0.001 * i as f64) } else { None });
                row.proxy_id = "px".into();
                row.route_id = "rt".into();
                row.backend_id = model.into();
                row.status_code = if next() % 5 == 0 { 429 } else { 200 };
                row.latency_ms = (next() as i64) * 10 + 1;
                row.token_input = if next() % 4 == 0 { None } else { Some((next() * 100) as i64) };
                row.token_output = if next() % 4 == 0 { None } else { Some((next() * 50) as i64) };
                row.is_stream = stream;
                row.ttft_ms = if stream && next() % 2 == 0 { Some((next() * 2 + 1) as i64) } else { None };
                row.cache_read_tokens = if next() % 3 == 0 { Some((next() * 40) as i64) } else { None };
                row.cache_write_tokens = if next() % 5 == 0 { Some((next() * 20) as i64) } else { None };
                row.created_at = format!("2026-01-01T{hour:02}:{minute:02}:{sec:02}Z");
                rows.push(row);
            }
            super::record_batch(&pool, &rows, &[], &[], &[], &[]).expect("batch");

            let from = "2026-01-01T00:00:00Z";
            let to = "2026-01-02T00:00:00Z";
            let hourly = model_timeseries(&pool, "hour", 24, Some(from), Some(to)).expect("hourly");
            // 对照：纯明细聚合。同批数据按 day 粒度跑（明细表），hour 桶加总后逐字段相等
            let daily = model_timeseries(&pool, "day", 0, Some(from), Some(to)).expect("daily");
            let by_model_day: std::collections::HashMap<&str, &ModelTimeSeriesPoint> =
                daily.data.iter().map(|p| (p.model_id.as_str(), p)).collect();
            assert_eq!(by_model_day.len(), 2, "两个模型都应有速写");
            // (requests, tin, tout, caches, failed, stream, ttft_samples, latency_total, ttft_total)
            let mut acc: std::collections::HashMap<&str, (i64,i64,i64,i64,i64,i64,i64,i64,f64,f64)> = Default::default();
            for p in &hourly.data {
                let e = acc.entry(p.model_id.as_str()).or_default();
                e.0 += p.requests;
                e.1 += p.token_input;
                e.2 += p.token_output;
                e.3 += p.cache_read_tokens;
                e.4 += p.cache_write_tokens;
                e.5 += p.failed_requests;
                e.6 += p.stream_requests;
                e.7 += p.ttft_samples;
                e.8 += p.avg_latency_ms * p.requests as f64;
                e.9 += p.avg_ttft_ms * p.ttft_samples as f64;
            }
            for (model_id, &(r, tin, tout, cr, cw, fail, stream, ttfts, lat_tot, ttft_tot)) in &acc {
                let d = by_model_day.get(model_id).expect("model in daily");
                assert_eq!(d.requests, r, "{model_id} 请求数");
                assert_eq!(d.token_input, tin, "{model_id} 输入");
                assert_eq!(d.token_output, tout, "{model_id} 输出");
                assert_eq!(d.cache_read_tokens, cr, "{model_id} 缓存读");
                assert_eq!(d.cache_write_tokens, cw, "{model_id} 缓存写");
                assert_eq!(d.failed_requests, fail, "{model_id} 失败");
                assert_eq!(d.stream_requests, stream, "{model_id} 流式");
                assert_eq!(d.ttft_samples, ttfts, "{model_id} ttft 样本");
                assert!(
                    (d.avg_latency_ms * d.requests as f64 - lat_tot).abs() < 1e-9,
                    "{model_id} 延迟总和"
                );
                assert!(
                    (d.avg_ttft_ms * d.ttft_samples as f64 - ttft_tot).abs() < 1e-9,
                    "{model_id} ttft 总和"
                );
                // fresh_input：两套实现同公式，这里钉公式本身
                let expected_fresh = (tin - cr - cw).max(0);
                let fresh_acc: i64 = hourly.data.iter()
                    .filter(|p| p.model_id == *model_id)
                    .map(|p| p.fresh_input_tokens).sum();
                assert_eq!(fresh_acc, expected_fresh, "{model_id} fresh 合计");
            }
            // cost 逐点加总（NULL 语义已在 daily/hourly 内各自成立，对比总和一致即可）
            let cost_daily: f64 = daily.data.iter().filter_map(|p| p.total_cost).sum();
            let cost_hourly: f64 = hourly.data.iter().filter_map(|p| p.total_cost).sum();
            assert!((cost_daily - cost_hourly).abs() < 1e-9, "成本 {cost_daily} vs {cost_hourly}");
        }
        cleanup_db(&path);
    }
}
