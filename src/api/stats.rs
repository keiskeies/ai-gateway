use std::sync::Arc;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::RequestFilter;
use crate::proxy::handler::ProxyState;

/// 概览页统计通用时间范围（RFC3339，UTC；[from, to) 半开区间）
#[derive(Clone, Default, Deserialize)]
pub struct RangeQuery {
    /// 起始时间（含）
    pub from: Option<String>,
    /// 结束时间（不含）
    pub to: Option<String>,
}

pub async fn overview(
    db: web::Data<DbPool>,
    state: web::Data<Arc<ProxyState>>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    // 并发连接来自内存中的 selector 计数器，无需触库
    let active_connections = state.selector.active_connections_total();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::overview(&db, active_connections, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn proxy_stats(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::proxy_stats(&db, &proxy_id, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 删除虚拟模型前的影响披露：该模型名下会被一并销毁的历史行数
pub async fn proxy_history_impact(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let impact = web::block(move || crate::db::stats::proxy_history_impact(&db, &proxy_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(impact))
}

/// 全部虚拟模型用量（一次请求），替代按 proxy 逐个查询
pub async fn proxy_stats_all(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::proxy_stats_all(&db, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 全部平台用量（一次请求），替代按 platform 逐个查询
pub async fn platform_stats_all(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::platform_stats_all(&db, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn platform_stats(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::platform_stats(&db, &platform_id, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 全部平台健康汇总（时段内检测/失败次数 + 最后一次结果），平台列表 CHECK 列用
pub async fn platform_health_all(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::platform_health_all(&db, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 单个平台最近 50 次连通性检测（倒序）
pub async fn platform_health(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::platform_health_history(&db, &platform_id, 50))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn backend_stats(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let backend_id = path.into_inner();
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::backend_stats(&db, &backend_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

#[derive(Deserialize)]
pub struct ModelTimeSeriesQuery {
    /// 粒度: hour / day / month / year
    pub granularity: Option<String>,
    /// 周期数量（未指定 from/to 时生效）
    pub limit: Option<usize>,
    /// 起始时间（RFC3339，UTC），含
    pub from: Option<String>,
    /// 结束时间（RFC3339，UTC），不含
    pub to: Option<String>,
}

pub async fn model_timeseries(
    db: web::Data<DbPool>,
    query: web::Query<ModelTimeSeriesQuery>,
) -> AppResult<HttpResponse> {
    let granularity = query.granularity.as_deref().unwrap_or("day").to_string();
    let limit = query.limit.unwrap_or_else(|| match granularity.as_str() {
        "hour" => 24,
        "month" => 12,
        "year" => 5,
        _ => 30,
    });
    let from = query.from.clone();
    let to = query.to.clone();
    let db = db.into_inner();
    let stats = web::block(move || {
        crate::db::stats::model_timeseries(&db, &granularity, limit, from.as_deref(), to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

#[derive(Debug, Deserialize)]
pub struct ErrorQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// 可选：只看某个后端模型的错误，供大屏矩阵的失败数下钻
    pub model: Option<String>,
}

pub async fn error_stats(db: web::Data<DbPool>, query: web::Query<ErrorQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::error_stats(&db, q.from.as_deref(), q.to.as_deref(), q.model.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn activity_heatmap(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::activity_heatmap(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn latency_percentiles(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::latency_percentiles(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 模型维度汇总（时间范围内每模型全量统计，概览页模型性能矩阵用）
pub async fn model_summaries(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::model_summaries(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 请求明细列表（倒序分页）；筛选条件见 `RequestFilter`
pub async fn request_logs(db: web::Data<DbPool>, query: web::Query<RequestFilter>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let filter = query.into_inner();
    let result = web::block(move || crate::db::stats::list_requests(&db, &filter))
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

/// CSV 文本单元格：加引号、内部引号翻倍。以 `= + - @` 或制表符开头的值再补一个撇号——
/// 模型名、平台名、Key 名都是用户可写的，导出到表格软件里不该被当成公式执行。
pub(crate) fn csv_text(value: &str) -> String {
    let guarded = match value.as_bytes().first() {
        Some(b'=') | Some(b'+') | Some(b'-') | Some(b'@') | Some(b'\t') => format!("'{value}"),
        _ => value.to_string(),
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// 数字单元格：None 留空（导出后是空单元格，不是 0，也不是 "NULL"）
pub(crate) fn csv_num<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// 按当前筛选导出请求明细 CSV（服务端生成，受行数上限约束）。
/// 与列表接口共用 `RequestFilter` 与同一条 WHERE，所以"导出的是屏幕上这批"是成立的。
/// 响应头带命中总数与实际行数，前端才能说明截断，而不是默默少给。
pub async fn export_traffic(
    db: web::Data<DbPool>,
    query: web::Query<RequestFilter>,
) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let filter = query.into_inner();
    let limit = filter
        .limit
        .unwrap_or(crate::db::stats::EXPORT_DEFAULT_ROWS)
        .clamp(1, crate::db::stats::EXPORT_MAX_ROWS);
    let (rows, total) = web::block(move || crate::db::stats::export_requests(&db, &filter, limit))
        .await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))??;

    let mut body = String::from("\u{feff}"); // BOM：没有它 Excel 会把中文名称列读成乱码
    body.push_str(
        "created_at,model,platform,virtual_model,api_key,status_code,error_type,stream,\
         token_input,token_output,fresh_input_tokens,cache_read_tokens,cache_write_tokens,\
         latency_ms,ttft_ms,output_tokens_per_sec,cost,credits,path,attempts\n",
    );
    for row in &rows {
        let cells = [
            csv_text(&row.created_at),
            csv_text(&row.model_id),
            csv_text(&row.platform_name),
            csv_text(&row.proxy_name),
            csv_text(row.api_key_name.as_deref().unwrap_or("")),
            row.status_code.to_string(),
            csv_text(row.error_type.as_deref().unwrap_or("")),
            if row.is_stream { "stream".to_string() } else { "json".to_string() },
            csv_num(row.token_input),
            csv_num(row.token_output),
            csv_num(row.fresh_input_tokens),
            csv_num(row.cache_read_tokens),
            csv_num(row.cache_write_tokens),
            row.latency_ms.to_string(),
            csv_num(row.ttft_ms),
            csv_num(row.output_speed_tps.map(|v| v.round())),
            csv_num(row.cost),
            csv_num(row.credits),
            csv_text(row.path.as_deref().unwrap_or("")),
            // 空单元格 = 这条行早于 V16，次数无从查起（不是 0 次）
            csv_num(row.attempts),
        ];
        body.push_str(&cells.join(","));
        body.push('\n');
    }

    let name = format!("aigw-traffic-{}.csv", chrono::Utc::now().format("%Y%m%d-%H%M"));
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header(("Content-Disposition", format!("attachment; filename=\"{name}\"")))
        .insert_header(("X-Export-Rows", rows.len().to_string()))
        .insert_header(("X-Export-Total", total.to_string()))
        .body(body))
}

/// 密钥维度用量汇总（时间范围内每个 API Key 一行）
pub async fn api_key_usage(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::api_key_usage(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 单次请求的尝试轨迹（重试 / fallback 依次试过哪些后端）。
/// `path` 是 `request_stats.id`（明细行的 id）；V16 之前的旧行没有 request_id，返回空轨迹而不是编造链路
pub async fn request_trace(
    db: web::Data<DbPool>,
    path: web::Path<i64>,
) -> AppResult<HttpResponse> {
    let stat_id = path.into_inner();
    let db = db.into_inner();
    let trace = web::block(move || crate::db::stats::request_trace(&db, stat_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(trace))
}

/// 请求/响应详情：请求头、请求正文、上游原始响应正文。
/// 三者各自判断是否存在；长请求可以只有头、没有正文，响应采集也可以独立开启。
pub async fn request_payload(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    path: web::Path<i64>,
) -> AppResult<HttpResponse> {
    let stat_id = path.into_inner();
    let (request_capture_enabled, response_capture_enabled) = {
        let cfg = config.read();
        (
            cfg.defaults.capture_request_bodies,
            cfg.defaults.capture_response_bodies,
        )
    };
    let db = db.into_inner();
    let mut payload = web::block(move || crate::db::stats::payload_for_stat(&db, stat_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    // 采集开关是当前配置，不是这条请求写入时的快照；前端用它区分"开关没开"与"这条没记"
    payload.request_capture_enabled = request_capture_enabled;
    payload.response_capture_enabled = response_capture_enabled;
    Ok(HttpResponse::Ok().json(payload))
}

/// 单个密钥的虚拟模型维度用量
pub async fn api_key_model_usage(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    let key_id = path.into_inner();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::api_key_model_usage(&db, &key_id, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 清理悬挂统计行的请求体。不带 body 时按老口径直接删除，带 `dry_run` 时只报数。
#[derive(Deserialize)]
pub struct CleanupBody {
    /// 预演：返回"照这个条件会删掉多少"，库里一行不动
    #[serde(default)]
    pub dry_run: bool,
}

/// 清理悬挂的历史统计行（指向已删除 proxy / route / backend 的 request_stats 记录）
pub async fn cleanup_stats(
    db: web::Data<DbPool>,
    body: Option<web::Json<CleanupBody>>,
) -> AppResult<HttpResponse> {
    let dry_run = body.map(|b| b.0.dry_run).unwrap_or(false);
    let db = db.into_inner();
    let result = web::block(move || crate::db::stats::cleanup_dangling(&db, dry_run))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

#[derive(Deserialize)]
pub struct PurgeBody {
    /// 删掉 N 天之前的请求明细与轨迹
    pub older_than_days: i64,
    #[serde(default)]
    pub dry_run: bool,
}

/// 按年龄清理请求明细。没有定时器自动跑这件事：删历史数据必须每次由人点头，
/// 所以前端先 `dry_run` 拿行数、确认弹窗显示完再真正执行。
pub async fn purge_stats(
    db: web::Data<DbPool>,
    body: web::Json<PurgeBody>,
) -> AppResult<HttpResponse> {
    let body = body.into_inner();
    let db = db.into_inner();
    let result = web::block(move || {
        crate::db::stats::purge_older_than(&db, body.older_than_days, body.dry_run)
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

/// 数据库体检：路径、体积、行数与明细时间跨度（设置页"数据"分组）
pub async fn db_info(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let path = config.read().resolved_db_path();
    let result = web::block(move || crate::db::stats::db_info(&db, &path))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

/// 重放一条请求：把存过的原始正文重新送进网关，响应原样回给调用方（流式仍是流式）。
///
/// 没有正文就直说，不做"按今天的配置凑一个请求"——那样发出去的不是当初那一笔，
/// 调试结论会跟着错。409 而不是 404：这条明细行确实存在，缺的是正文。
pub async fn replay_request(
    db: web::Data<DbPool>,
    state: web::Data<Arc<ProxyState>>,
    path: web::Path<i64>,
) -> AppResult<HttpResponse> {
    let stat_id = path.into_inner();
    let db = db.into_inner();
    let stored = web::block(move || crate::db::stats::body_for_stat(&db, stat_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    let Some(stored) = stored else {
        return Err(crate::error::AppError::Conflict(format!(
            "request {} has no stored body; body capture is off or the row predates V21",
            stat_id
        )));
    };
    if stored.truncated {
        return Err(crate::error::AppError::Conflict(format!(
            "request {} only has a truncated body ({} bytes original); it can be viewed but not replayed",
            stat_id, stored.bytes
        )));
    }
    let response = crate::proxy::handler::replay_request(&state, stat_id, &stored).await?;
    Ok(response)
}

/// 清空全部已存正文（关掉采集开关后把已经落库的提示词抹掉）。
/// 统计与轨迹不动——它们本来就不含正文，删掉它们只会让报表凭空缺一块。
pub async fn delete_bodies(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let deleted = web::block(move || crate::db::stats::delete_all_bodies(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": deleted })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_quote_text_and_neutralise_formulas() {
        assert_eq!(csv_text("plain"), "\"plain\"");
        assert_eq!(csv_text("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_text("a,b"), "\"a,b\"");
        // 模型名、平台名、Key 名都是用户可写的：以 = + - @ 开头的值落到表格里是一段公式
        assert_eq!(csv_text("=1+1"), "\"'=1+1\"");
        assert_eq!(csv_text("@cmd"), "\"'@cmd\"");
        assert_eq!(csv_text("-40"), "\"'-40\"");
        assert_eq!(csv_num(None::<i64>), "", "未定价导出空单元格，不是 0 也不是 NULL");
        assert_eq!(csv_num(Some(12_i64)), "12");
    }
}
