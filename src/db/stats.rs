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
    let total_proxies: i64 = conn.query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0)).unwrap_or(0);
    let active_proxies = total_proxies; // all proxies are active (no status field)
    let total_platforms: i64 = conn.query_row("SELECT COUNT(*) FROM platforms", [], |r| r.get(0)).unwrap_or(0);
    let active_platforms = total_platforms; // all platforms are active
    let total_models: i64 = conn.query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0)).unwrap_or(0);
    let total_token_input: i64 = conn.query_row("SELECT COALESCE(SUM(token_input),0) FROM request_stats WHERE token_input IS NOT NULL", [], |r| r.get(0)).unwrap_or(0);
    let total_token_output: i64 = conn.query_row("SELECT COALESCE(SUM(token_output),0) FROM request_stats WHERE token_output IS NOT NULL", [], |r| r.get(0)).unwrap_or(0);

    Ok(OverviewStats {
        total_requests, success_rate: if total_requests > 0 { success_count as f64 / total_requests as f64 * 100.0 } else { 0.0 },
        avg_latency_ms: avg_latency, active_proxies, total_proxies, active_platforms, total_platforms, total_models,
        total_token_input, total_token_output,
    })
}

pub fn proxy_stats(pool: &DbPool, proxy_id: &str) -> AppResult<ProxyStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    
    // Get proxy name
    let proxy_name: String = conn.query_row(
        "SELECT name FROM proxies WHERE id = ?1", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or_default();

    let total_requests: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or(0);
    
    let success_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1 AND status_code >= 200 AND status_code < 300", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or(0);
    
    let avg_latency: f64 = conn.query_row(
        "SELECT COALESCE(AVG(latency_ms),0) FROM request_stats WHERE proxy_id = ?1", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or(0.0);
    
    let requests_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats WHERE proxy_id = ?1 AND date(created_at) = date('now')", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or(0);

    let total_token_input: i64 = conn.query_row(
        "SELECT COALESCE(SUM(token_input),0) FROM request_stats WHERE proxy_id = ?1 AND token_input IS NOT NULL", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
    ).unwrap_or(0);
    
    let total_token_output: i64 = conn.query_row(
        "SELECT COALESCE(SUM(token_output),0) FROM request_stats WHERE proxy_id = ?1 AND token_output IS NOT NULL", 
        rusqlite::params![proxy_id], 
        |r| r.get(0)
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

pub fn platform_stats(pool: &DbPool, platform_id: &str) -> AppResult<PlatformStats> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // Get platform name
    let platform_name: String = conn.query_row(
        "SELECT name FROM platforms WHERE id = ?1",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or_default();

    // Join request_stats -> backends -> platforms to get stats per platform
    let total_requests: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats rs
         JOIN backends b ON rs.backend_id = b.id
         WHERE b.platform_id = ?1",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or(0);

    let success_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_stats rs
         JOIN backends b ON rs.backend_id = b.id
         WHERE b.platform_id = ?1 AND rs.status_code >= 200 AND rs.status_code < 300",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or(0);

    let avg_latency: f64 = conn.query_row(
        "SELECT COALESCE(AVG(rs.latency_ms),0) FROM request_stats rs
         JOIN backends b ON rs.backend_id = b.id
         WHERE b.platform_id = ?1",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or(0.0);

    let total_token_input: i64 = conn.query_row(
        "SELECT COALESCE(SUM(rs.token_input),0) FROM request_stats rs
         JOIN backends b ON rs.backend_id = b.id
         WHERE b.platform_id = ?1 AND rs.token_input IS NOT NULL",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or(0);

    let total_token_output: i64 = conn.query_row(
        "SELECT COALESCE(SUM(rs.token_output),0) FROM request_stats rs
         JOIN backends b ON rs.backend_id = b.id
         WHERE b.platform_id = ?1 AND rs.token_output IS NOT NULL",
        rusqlite::params![platform_id],
        |r| r.get(0)
    ).unwrap_or(0);

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
