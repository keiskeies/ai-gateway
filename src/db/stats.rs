use crate::error::AppResult;
use crate::models::stats::*;
use crate::db::DbPool;
use std::collections::{BTreeSet, HashSet};

/// 规范化时间范围参数：解析 RFC3339 后统一转为 UTC 秒精度字符串（`...SS+00:00`），
/// 保证与存量 `...SS.fff+00:00` 的 created_at 做字典序比较语义正确（秒截断，比较边界对齐秒）。
fn normalize_bound(s: &str, field: &str) -> AppResult<String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
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

/// 生成时间范围内的连续周期 key（格式与 SQL 里 STRFTIME 一致），填补无数据周期造成的折线跳空。
/// - hour 粒度按服务器本地时区（与聚合表达式 datetime(...,'localtime') 口径一致）；
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
pub fn model_timeseries(
    pool: &DbPool,
    granularity: &str,
    limit: usize,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<ModelTimeSeriesStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // created_at 列：hour 粒度转本地时区，其余保持 UTC（裸列比较可走 idx_stats_created 索引）
    let time_expr = match granularity {
        "hour" => "datetime(rs.created_at, 'localtime')",
        _ => "rs.created_at",
    };
    let fmt = match granularity {
        "hour" => "%Y-%m-%d %H",
        "day" => "%Y-%m-%d",
        "month" => "%Y-%m",
        "year" => "%Y",
        _ => {
            return Err(crate::error::AppError::BadRequest(
                "invalid granularity, expected hour/day/month/year".into(),
            ))
        }
    };

    // 规范化边界（RFC3339 → UTC 秒精度）；任一存在即以范围为准，跳过 limit 截取
    let range = TimeRange::parse(from, to)?;
    let range_mode = !range.is_empty();

    // 拼接 time_expr（静态片段 + 粒度白名单，无注入风险）
    // 性能指标口径：
    // - avg_ttft_ms：仅统计有 ttft 的行（AVG 自动跳过 NULL）
    // - output_speed_tps：生成阶段口径 = SUM(token_output)*1000 / (SUM(latency) - SUM(ttft))，
    //   非流式行 ttft 为空、按 0 计（即全耗时视为生成阶段）
    // - failed_requests / stream_requests：失败数与流式数
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
            COALESCE(SUM(rs.cache_read_tokens), 0) AS cache_read_tokens
        FROM request_stats rs
        LEFT JOIN backends b ON rs.backend_id = b.id
        WHERE 1=1
          AND (?2 IS NULL OR rs.created_at >= ?2)
          AND (?3 IS NULL OR rs.created_at <  ?3)
        GROUP BY period, model_id
        ORDER BY period, model_id
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![fmt, range.from, range.to],
        |r| {
            Ok(ModelTimeSeriesPoint {
                period: r.get::<_, String>(0)?,
                model_id: r.get::<_, String>(1)?,
                requests: r.get::<_, i64>(2)?,
                token_input: r.get::<_, i64>(3)?,
                token_output: r.get::<_, i64>(4)?,
                avg_latency_ms: r.get::<_, f64>(5)?,
                failed_requests: r.get::<_, i64>(6)?,
                stream_requests: r.get::<_, i64>(7)?,
                avg_ttft_ms: r.get::<_, f64>(8)?,
                output_speed_tps: r.get::<_, f64>(9)?,
                cache_read_tokens: r.get::<_, i64>(10)?,
            })
        },
    )?;

    let mut all_data = Vec::new();
    for row in rows {
        all_data.push(row?);
    }

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

/// 错误分析：非 2xx 请求按 error_type 聚合（历史悬挂/缺失归入 Unknown）
pub fn error_stats(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<ErrorStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let count_in_range = |extra: &str| -> i64 {
        let sql = format!("SELECT COUNT(*) FROM request_stats WHERE 1=1{extra}{}", range.and("created_at"));
        conn.query_row(&sql, rusqlite::params_from_iter(range.params()), |r| r.get(0)).unwrap_or(0)
    };

    let total = count_in_range("");
    let success = count_in_range(" AND status_code >= 200 AND status_code < 300");

    let sql = format!(
        "SELECT COALESCE(error_type, 'Unknown') AS kind, COUNT(*) AS cnt
         FROM request_stats
         WHERE NOT (status_code >= 200 AND status_code < 300)
         {}
         GROUP BY kind
         ORDER BY cnt DESC",
        range.and("created_at")
    );
    let mut stmt = conn.prepare(&sql)?;
    let errors = stmt
        .query_map(rusqlite::params_from_iter(range.params()), |r| {
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

    let pct = |frac: f64| -> f64 {
        let off = ((n as f64 * frac) as i64).clamp(0, n - 1);
        conn.query_row(
            &format!(
                "SELECT COALESCE(latency_ms, 0) FROM request_stats{} ORDER BY latency_ms LIMIT 1 OFFSET ?",
                range.where_clause("created_at")
            ),
            rusqlite::params_from_iter(
                range
                    .params()
                    .into_iter()
                    .map(|s| s.to_string())
                    .chain(std::iter::once(off.to_string())),
            ),
            |r| r.get(0),
        )
        .unwrap_or(0.0)
    };

    let max: f64 = conn
        .query_row(
            &format!("SELECT COALESCE(MAX(latency_ms),0) FROM request_stats{}", range.where_clause("created_at")),
            rusqlite::params_from_iter(range.params()),
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    Ok(LatencyPercentiles { p50: pct(0.50), p90: pct(0.90), p99: pct(0.99), max })
}

pub fn record(pool: &DbPool, stat: &RequestStat) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, error_type, created_at, ttft_ms, cache_read_tokens, cache_write_tokens, is_stream) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        rusqlite::params![stat.proxy_id, stat.route_id, stat.backend_id, stat.status_code,
            stat.latency_ms, stat.token_input, stat.token_output, stat.error_type, stat.created_at,
            stat.ttft_ms, stat.cache_read_tokens, stat.cache_write_tokens, stat.is_stream as i64])?;
    Ok(())
}

/// 模型维度汇总：时间范围内每个模型的全量统计（口径与 model_timeseries 一致，只是不按周期切分）。
/// 供概览页「模型性能矩阵」表格使用：一屏看尽 模型 × 请求/失败/token/延时/首字/速度。
pub fn model_summaries(pool: &DbPool, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ModelSummary>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    let sql = r#"
        SELECT
            COALESCE(b.model_id, 'unknown') AS model_id,
            COUNT(*)                        AS requests,
            SUM(CASE WHEN rs.status_code >= 200 AND rs.status_code < 300 THEN 1 ELSE 0 END) AS success_requests,
            SUM(CASE WHEN NOT (rs.status_code >= 200 AND rs.status_code < 300) THEN 1 ELSE 0 END) AS failed_requests,
            SUM(rs.is_stream)               AS stream_requests,
            COALESCE(SUM(rs.token_input), 0)  AS token_input,
            COALESCE(SUM(rs.token_output), 0) AS token_output,
            COALESCE(AVG(rs.latency_ms), 0)   AS avg_latency_ms,
            COALESCE(AVG(rs.ttft_ms), 0)      AS avg_ttft_ms,
            CASE WHEN SUM(rs.latency_ms) - SUM(COALESCE(rs.ttft_ms, 0)) > 0
                 THEN COALESCE(SUM(rs.token_output), 0) * 1000.0 / (SUM(rs.latency_ms) - SUM(COALESCE(rs.ttft_ms, 0)))
                 ELSE 0 END                   AS output_speed_tps,
            COALESCE(SUM(rs.cache_read_tokens), 0) AS cache_read_tokens,
            COALESCE(SUM(rs.cache_write_tokens), 0) AS cache_write_tokens
        FROM request_stats rs
        LEFT JOIN backends b ON rs.backend_id = b.id
        WHERE 1=1
          AND (?1 IS NULL OR rs.created_at >= ?1)
          AND (?2 IS NULL OR rs.created_at <  ?2)
        GROUP BY model_id
        ORDER BY requests DESC
    "#;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![range.from, range.to], |r| {
        let token_input: i64 = r.get(5)?;
        let cache_read_tokens: i64 = r.get(10)?;
        let cache_write_tokens: i64 = r.get(11)?;
        // 新增输入 = 总输入 − 缓存读 − 缓存写（saturating 防御异常数据）
        let fresh_input = (token_input - cache_read_tokens - cache_write_tokens).max(0);
        Ok(ModelSummary {
            model_id: r.get(0)?,
            requests: r.get(1)?,
            success_requests: r.get(2)?,
            failed_requests: r.get(3)?,
            stream_requests: r.get(4)?,
            token_input,
            token_output: r.get(6)?,
            avg_latency_ms: r.get(7)?,
            avg_ttft_ms: r.get(8)?,
            output_speed_tps: r.get(9)?,
            cache_read_tokens,
            fresh_input_tokens: fresh_input,
            // CC Switch 口径：命中 / (命中 + 新增输入)
            cache_hit_rate: {
                let denominator = cache_read_tokens + fresh_input;
                if denominator > 0 { cache_read_tokens as f64 / denominator as f64 * 100.0 } else { 0.0 }
            },
        })
    })?;
    Ok(rows.filter_map(|x| x.ok()).collect())
}

/// 请求明细列表（倒序分页）：概览大屏底部日志表数据源。
/// output_speed_tps 由 SQL 表达式逐行计算：token_output / (latency − ttft)，无效时为 NULL。
/// model 精确匹配后端模型 id；status 取 success / fail / None（all）。
pub fn list_requests(
    pool: &DbPool,
    from: Option<&str>,
    to: Option<&str>,
    model: Option<&str>,
    status: Option<&str>,
    page: i64,
    page_size: i64,
) -> AppResult<RequestLogPage> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // 动态 WHERE：时间范围 + 模型 + 状态（success=2xx / fail=非2xx）
    let mut where_sql = String::from(" WHERE 1=1");
    where_sql.push_str(&range.and("rs.created_at"));
    let mut params: Vec<String> = range.params().into_iter().map(|s| s.to_string()).collect();
    if let Some(m) = model {
        where_sql.push_str(" AND COALESCE(b.model_id, 'unknown') = ?");
        params.push(m.to_string());
    }
    match status {
        Some("success") => where_sql.push_str(" AND rs.status_code >= 200 AND rs.status_code < 300"),
        Some("fail") => where_sql.push_str(" AND NOT (rs.status_code >= 200 AND rs.status_code < 300)"),
        _ => {}
    }

    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM request_stats rs LEFT JOIN backends b ON rs.backend_id = b.id{}", where_sql),
            rusqlite::params_from_iter(params.iter()),
            |r| r.get(0),
        )?;

    // 倒序取页：created_at 相同（同秒批量写入）时按 id 兜底
    params.push(page_size.to_string());
    params.push(((page - 1) * page_size).to_string());
    let sql = format!(
        r#"
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
            rs.is_stream, rs.error_type
        FROM request_stats rs
        LEFT JOIN backends b ON rs.backend_id = b.id
        LEFT JOIN platforms p ON b.platform_id = p.id
        LEFT JOIN proxies px ON rs.proxy_id = px.id
        {}
        ORDER BY rs.created_at DESC, rs.id DESC
        LIMIT ? OFFSET ?
        "#,
        where_sql
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
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
            })
        })?
        .filter_map(|x| x.ok())
        .collect();

    Ok(RequestLogPage {
        total,
        page,
        page_size,
        items,
    })
}

/// 删除指定 proxy 的所有请求统计
pub fn delete_by_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM request_stats WHERE proxy_id = ?1", rusqlite::params![proxy_id])?;
    Ok(())
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
}

/// 清理悬挂的历史统计行：删除 request_stats 中指向已不存在 proxy / route / backend 的行。
/// 悬挂行的 model_id / platform 聚合会显示为 'unknown'，清理后大屏数据口径恢复干净。
pub fn cleanup_dangling(pool: &DbPool) -> AppResult<StatsCleanupResult> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // proxy_id / route_id / backend_id 任一指向不存在的记录，即视为悬挂行
    let deleted: i64 = conn
        .execute(
            r#"
        DELETE FROM request_stats
        WHERE proxy_id NOT IN (SELECT id FROM proxies)
           OR route_id NOT IN (SELECT id FROM routes)
           OR backend_id NOT IN (SELECT id FROM backends)
        "#,
            [],
        )?
        .try_into()
        .unwrap_or(0);

    tracing::info!("Stats cleanup: removed {} dangling request_stats row(s)", deleted);
    Ok(StatsCleanupResult { deleted })
}

pub fn overview(
    pool: &DbPool,
    total_connections_active: i64,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<OverviewStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let range = TimeRange::parse(from, to)?;

    // 主体统计：全部带 [from, to) 过滤
    let count = |extra: &str| -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM request_stats WHERE 1=1{extra}{}", range.and("created_at")),
            rusqlite::params_from_iter(range.params()),
            |r| r.get(0),
        ).unwrap_or(0)
    };
    let total_requests = count("");
    let success_count = count(" AND status_code >= 200 AND status_code < 300");
    let avg_latency: f64 = conn.query_row(
        &format!("SELECT COALESCE(AVG(latency_ms),0) FROM request_stats{}", range.where_clause("created_at")),
        rusqlite::params_from_iter(range.params()),
        |r| r.get(0),
    ).unwrap_or(0.0);
    let total_token_input: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(token_input),0) FROM request_stats WHERE token_input IS NOT NULL{}", range.and("created_at")),
        rusqlite::params_from_iter(range.params()),
        |r| r.get(0),
    ).unwrap_or(0);
    let total_token_output: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(token_output),0) FROM request_stats WHERE token_output IS NOT NULL{}", range.and("created_at")),
        rusqlite::params_from_iter(range.params()),
        |r| r.get(0),
    ).unwrap_or(0);
    // 全局首字 / 速度 / 缓存命中（KPI 副行展示；口径与时序聚合一致）
    let (avg_ttft, avg_speed, total_cache_read, total_cache_write): (f64, f64, i64, i64) = conn.query_row(
        &format!(
            r#"SELECT
                COALESCE(AVG(ttft_ms), 0) AS avg_ttft,
                CASE WHEN SUM(latency_ms) - SUM(COALESCE(ttft_ms, 0)) > 0
                     THEN COALESCE(SUM(token_output), 0) * 1000.0 / (SUM(latency_ms) - SUM(COALESCE(ttft_ms, 0)))
                     ELSE 0 END AS avg_speed,
                COALESCE(SUM(cache_read_tokens), 0) AS total_cache_read,
                COALESCE(SUM(cache_write_tokens), 0) AS total_cache_write
             FROM request_stats{}"#,
            range.where_clause("created_at")
        ),
        rusqlite::params_from_iter(range.params()),
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).unwrap_or((0.0, 0.0, 0, 0));

    // 对比周期：指定范围时对比等长的前一段区间；未指定时保持昨日口径
    match prev_range(&range) {
        Some(prev) => {
            let requests_prev_period: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM request_stats{}", prev.where_clause("created_at")),
                rusqlite::params_from_iter(prev.params()),
                |r| r.get(0),
            ).unwrap_or(0);
            let avg_latency_prev_period: f64 = conn.query_row(
                &format!("SELECT COALESCE(AVG(latency_ms),0) FROM request_stats{}", prev.where_clause("created_at")),
                rusqlite::params_from_iter(prev.params()),
                |r| r.get(0),
            ).unwrap_or(0.0);
            Ok(build_overview(
                &conn, total_requests, success_count, avg_latency,
                total_token_input, total_token_output,
                total_connections_active, requests_prev_period, avg_latency_prev_period,
                avg_ttft, avg_speed, total_cache_read, total_cache_write,
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
            ))
        }
    }
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
) -> OverviewStats {
    let total_proxies: i64 = conn.query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0)).unwrap_or(0);
    let active_proxies = total_proxies; // all proxies are active (no status field)
    let total_platforms: i64 = conn.query_row("SELECT COUNT(*) FROM platforms", [], |r| r.get(0)).unwrap_or(0);
    let active_platforms = total_platforms; // all platforms are active
    let total_models: i64 = conn.query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0)).unwrap_or(0);

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
    }
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

    let count = |extra: &str| -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1{extra}{}", range.and("created_at")),
            rusqlite::params_from_iter(
                std::iter::once(proxy_id.to_string())
                    .chain(range.params().into_iter().map(|s| s.to_string())),
            ),
            |r| r.get(0),
        ).unwrap_or(0)
    };
    let total_requests = count("");
    let success_count = count(" AND status_code >= 200 AND status_code < 300");

    let avg_latency: f64 = conn.query_row(
        &format!("SELECT COALESCE(AVG(latency_ms),0) FROM request_stats WHERE proxy_id = ?1{}", range.and("created_at")),
        rusqlite::params_from_iter(
            std::iter::once(proxy_id.to_string()).chain(range.params().into_iter().map(|s| s.to_string())),
        ),
        |r| r.get(0),
    ).unwrap_or(0.0);

    // 今日请求数：与原口径一致（固定"今日"，不受时段筛选影响）
    let requests_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1 AND date(created_at) = date('now')",
        rusqlite::params![proxy_id],
        |r| r.get(0)
    ).unwrap_or(0);

    let total_token_input: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(token_input),0) FROM request_stats WHERE proxy_id = ?1 AND token_input IS NOT NULL{}", range.and("created_at")),
        rusqlite::params_from_iter(
            std::iter::once(proxy_id.to_string()).chain(range.params().into_iter().map(|s| s.to_string())),
        ),
        |r| r.get(0),
    ).unwrap_or(0);

    let total_token_output: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(token_output),0) FROM request_stats WHERE proxy_id = ?1 AND token_output IS NOT NULL{}", range.and("created_at")),
        rusqlite::params_from_iter(
            std::iter::once(proxy_id.to_string()).chain(range.params().into_iter().map(|s| s.to_string())),
        ),
        |r| r.get(0),
    ).unwrap_or(0);

    Ok(ProxyStats {
        proxy_id: proxy_id.to_string(),
        proxy_name,
        total_requests,
        success_rate: if total_requests > 0 { success_count as f64 / total_requests as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: avg_latency,
        requests_today,
        total_token_input,
        total_token_output,
    })
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

    // 悬挂统计兜底：backend 被删除后 request_stats 仍可能留着旧 backend_id。
    // 用 LEFT JOIN 关联 platforms；匹配不上的行归入 "unknown"（platform_id 为空），避免重复/遗漏。
    // 注意：悬挂行只能按 backend_id 聚合，无法再判定其 platform，因此归入 unknown。
    let base = "COALESCE(b.platform_id, 'unknown') = ?1";
    let query_val = |select: &str, extra: &str| -> f64 {
        conn.query_row(
            &format!(
                "SELECT {select} FROM request_stats rs
                 LEFT JOIN backends b ON rs.backend_id = b.id
                 WHERE {base}{extra}{}",
                range.and("rs.created_at")
            ),
            rusqlite::params_from_iter(
                std::iter::once(platform_id.to_string())
                    .chain(range.params().into_iter().map(|s| s.to_string())),
            ),
            |r| r.get(0),
        ).unwrap_or(0.0)
    };

    let total_requests = query_val("COUNT(*)", "") as i64;
    let success_count = query_val("COUNT(*)", " AND rs.status_code >= 200 AND rs.status_code < 300") as i64;
    let avg_latency = query_val("COALESCE(AVG(rs.latency_ms),0)", "");
    let total_token_input = query_val("COALESCE(SUM(rs.token_input),0)", " AND rs.token_input IS NOT NULL") as i64;
    let total_token_output = query_val("COALESCE(SUM(rs.token_output),0)", " AND rs.token_output IS NOT NULL") as i64;

    Ok(PlatformStats {
        platform_id: platform_id.to_string(),
        platform_name,
        total_requests,
        success_rate: if total_requests > 0 { success_count as f64 / total_requests as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: avg_latency,
        total_token_input,
        total_token_output,
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
