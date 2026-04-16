use crate::error::AppResult;
use crate::models::stats::*;
use crate::db::DbPool;

pub fn record(pool: &DbPool, stat: &RequestStat) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, error_type, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![stat.proxy_id, stat.route_id, stat.backend_id, stat.status_code,
            stat.latency_ms, stat.token_input, stat.token_output, stat.error_type, stat.created_at])?;
    Ok(())
}

pub fn overview(pool: &DbPool) -> AppResult<OverviewStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let total_requests: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats", [], |r| r.get(0)).unwrap_or(0);
    let success_count: i64 = conn.query_row("SELECT COUNT(*) FROM request_stats WHERE status_code >= 200 AND status_code < 300", [], |r| r.get(0)).unwrap_or(0);
    let avg_latency: f64 = conn.query_row("SELECT COALESCE(AVG(latency_ms),0) FROM request_stats", [], |r| r.get(0)).unwrap_or(0.0);
    let active_proxies: i64 = conn.query_row("SELECT COUNT(*) FROM proxies WHERE status='\"Running\"'", [], |r| r.get(0)).unwrap_or(0);
    let total_proxies: i64 = conn.query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0)).unwrap_or(0);
    let active_platforms: i64 = conn.query_row("SELECT COUNT(*) FROM platforms WHERE status='\"Active\"'", [], |r| r.get(0)).unwrap_or(0);
    let total_platforms: i64 = conn.query_row("SELECT COUNT(*) FROM platforms", [], |r| r.get(0)).unwrap_or(0);
    let total_models: i64 = conn.query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0)).unwrap_or(0);

    Ok(OverviewStats {
        total_requests, success_rate: if total_requests > 0 { success_count as f64 / total_requests as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: avg_latency, active_proxies, total_proxies, active_platforms, total_platforms, total_models,
    })
}
