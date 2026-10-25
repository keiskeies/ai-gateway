pub mod platform;
pub mod alert;
pub mod backup;
pub mod model;
pub mod proxy;
pub mod route;
pub mod stats;
pub mod settings;
pub mod api_key;
pub mod auth;
pub mod config_transfer;
pub mod events;
pub mod client_config;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        .service(
            web::scope("/api")
                // 登录鉴权：除 status/setup/login 外的 /api 路由都需要有效会话
                .wrap(actix_web::middleware::from_fn(auth::require_auth))
                .route("/auth/status", web::get().to(auth::status))
                .route("/auth/setup", web::post().to(auth::setup))
                .route("/auth/login", web::post().to(auth::login))
                .route("/auth/logout", web::post().to(auth::logout))
                .route("/auth/password", web::put().to(auth::change_password))
                // 实时事件流（SSE）：控制台唯一长连接，其余接口都是一问一答
                .route("/events", web::get().to(events::stream))
                // 平台
                .route("/platforms", web::get().to(platform::list))
                .route("/platforms", web::post().to(platform::create))
                .route("/platforms/presets", web::get().to(platform::list_presets))
                .route("/platforms/test", web::post().to(platform::test_config))
                .route("/platforms/{id}", web::get().to(platform::get))
                .route("/platforms/{id}", web::put().to(platform::update))
                .route("/platforms/{id}", web::delete().to(platform::delete))
                .route("/platforms/{id}/remote-models", web::get().to(platform::fetch_remote_models))
                .route("/platforms/{id}/models/sync", web::post().to(platform::sync_models))
                .route("/platforms/{id}/models/import", web::post().to(platform::import_models))
                .route("/platforms/{id}/models/batch", web::put().to(platform::batch_update_models))
                .route("/platforms/{id}/models/prices", web::get().to(platform::export_prices))
                .route("/platforms/{id}/models/prices", web::post().to(platform::import_prices))
                // 模型
                .route("/models", web::get().to(model::list))
                .route("/models", web::post().to(model::create))
                .route("/models/{id}", web::get().to(model::get))
                .route("/models/{id}", web::put().to(model::update))
                .route("/models/{id}", web::delete().to(model::delete))
                .route("/models/{id}/test", web::post().to(model::test_connection))
                // 代理
                .route("/proxies", web::get().to(proxy::list))
                .route("/proxies", web::post().to(proxy::create))
                .route("/proxies/{id}", web::get().to(proxy::get))
                .route("/proxies/{id}", web::put().to(proxy::update))
                .route("/proxies/{id}", web::delete().to(proxy::delete))
                // 路由
                .route("/routes", web::get().to(route::list_all))
                .route("/proxies/{proxy_id}/routes", web::get().to(route::list))
                .route("/proxies/{proxy_id}/routes", web::post().to(route::create))
                .route("/routes/{id}", web::put().to(route::update))
                .route("/routes/{id}", web::delete().to(route::delete))
                .route("/routes/{route_id}/backends", web::get().to(route::list_backends))
                .route("/routes/{route_id}/backends", web::post().to(route::add_backend))
                .route("/backends/{id}", web::put().to(route::update_backend))
                .route("/backends/{id}", web::delete().to(route::delete_backend))
                // API Keys
                .route("/api-keys", web::get().to(api_key::list))
                .route("/api-keys", web::post().to(api_key::create))
                .route("/api-keys/batch", web::post().to(api_key::batch))
                .route("/api-keys/{id}", web::put().to(api_key::update))
                .route("/api-keys/{id}/rotate", web::post().to(api_key::rotate))
                .route("/api-keys/{id}", web::delete().to(api_key::delete))
                // 统计
        .route("/stats/overview", web::get().to(stats::overview))
        .route("/stats/proxies", web::get().to(stats::proxy_stats_all))
        .route("/stats/proxy/{proxy_id}", web::get().to(stats::proxy_stats))
        .route("/stats/proxy/{proxy_id}/impact", web::get().to(stats::proxy_history_impact))
        .route("/stats/platforms", web::get().to(stats::platform_stats_all))
        .route("/stats/platforms/health", web::get().to(stats::platform_health_all))
        .route("/stats/platform/{platform_id}", web::get().to(stats::platform_stats))
        .route("/stats/platform/{platform_id}/health", web::get().to(stats::platform_health))
        .route("/stats/model-timeseries", web::get().to(stats::model_timeseries))
        .route("/stats/backend/{backend_id}", web::get().to(stats::backend_stats))
        .route("/stats/errors", web::get().to(stats::error_stats))
        .route("/stats/heatmap", web::get().to(stats::activity_heatmap))
        .route("/stats/latency", web::get().to(stats::latency_percentiles))
        .route("/stats/model-summaries", web::get().to(stats::model_summaries))
        .route("/stats/requests", web::get().to(stats::request_logs))
        .route("/stats/requests/export", web::get().to(stats::export_traffic))
        .route("/stats/requests/{id}/trace", web::get().to(stats::request_trace))
        .route("/stats/requests/{id}/replay", web::post().to(stats::replay_request))
        .route("/stats/bodies", web::delete().to(stats::delete_bodies))
        .route("/stats/api-keys", web::get().to(stats::api_key_usage))
        .route("/stats/api-keys/{id}", web::get().to(stats::api_key_model_usage))
        .route("/stats/cleanup", web::post().to(stats::cleanup_stats))
        .route("/stats/purge", web::post().to(stats::purge_stats))
        .route("/stats/db", web::get().to(stats::db_info))
                // 通知中心：读时扫描 + 已读状态
        .route("/alerts", web::get().to(alert::list))
        .route("/alerts/read-all", web::post().to(alert::mark_all_read))
        .route("/alerts/{key}/read", web::post().to(alert::mark_read))
                // 整库快照：拍 / 列表 / 下载 / 删 / 原地还原
        .route("/backups", web::get().to(backup::list))
        .route("/backups", web::post().to(backup::create))
        .route("/backups/{name}", web::get().to(backup::download))
        .route("/backups/{name}", web::delete().to(backup::delete))
        .route("/backups/{name}/restore", web::post().to(backup::restore))
                // 设置
                .route("/settings", web::get().to(settings::get_config))
                .route("/settings", web::put().to(settings::update_config))
                // 配置迁移：导出 / 预演 / 落库
                .route("/config/export", web::get().to(config_transfer::export))
                .route("/config/import/plan", web::post().to(config_transfer::plan))
                .route("/config/import", web::post().to(config_transfer::apply))
                // 客户端接入：检测 / 预览 / 应用 / 撤销 / 恢复备份
                .route("/clients", web::get().to(client_config::list))
                .route("/clients/{id}/preview", web::post().to(client_config::preview))
                .route("/clients/{id}/apply", web::post().to(client_config::apply))
                .route("/clients/{id}/restore", web::post().to(client_config::restore))
                .route("/clients/{id}/restore-backup", web::post().to(client_config::restore_backup))
        );
}
