use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpResponse, middleware};
use actix_files as actix_files;
use std::sync::Arc;
use parking_lot::RwLock;

use ai_gateway::proxy::handler::ProxyState;
use ai_gateway::lb::BackendSelector;
use ai_gateway::api::settings::SharedAppConfig;
use ai_gateway::cache::{RouteCache, StatsWriter};
use ai_gateway::config::SystemProxy;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_config = ai_gateway::config::AppConfig::load_or_default();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&app_config.server.log_level)))
        .init();

    tracing::info!("AI Gateway starting...");
    tracing::info!("Admin UI + API: http://{}:{}", app_config.server.host, app_config.server.admin_port);
    tracing::info!("App directory: {:?}", ai_gateway::config::get_app_dir());

    let db_path = app_config.resolved_db_path();
    tracing::info!("Database path: {:?}", db_path);

    let db_pool = ai_gateway::db::init_pool(&db_path)
        .expect("Failed to initialize database");

    // 自动检测系统代理
    let system_proxy = SystemProxy::detect();
    let http_client = system_proxy.apply_to_builder(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(app_config.defaults.request_timeout_secs))
    ).build().expect("Failed to create HTTP client");

    let shared_config: SharedAppConfig = Arc::new(RwLock::new(app_config.clone()));
    let selector = Arc::new(BackendSelector::new());

    // 初始化内存缓存（启动时从 DB 加载，后续由管理操作触发刷新）
    let cache = RouteCache::new(db_pool.clone());

    // 启动后台统计写入器（异步批量写入，不阻塞请求）
    let stats_writer = StatsWriter::start(db_pool.clone());

    let proxy_state = Arc::new(ProxyState {
        db: db_pool.clone(),
        config: app_config.clone(),
        selector,
        http_client,
        cache: cache.clone(),
        stats_writer,
    });

    let static_dir = app_config.static_dir();
    let host = app_config.server.host.clone();
    let admin_port = app_config.server.admin_port;

    // 将 cache 的 Arc 引用传给 API 层用于缓存失效
    let api_cache = Arc::new(cache.clone());

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::new(proxy_state.clone()))
            .app_data(web::Data::new(shared_config.clone()))
            .app_data(web::Data::new(api_cache.clone()))
            .configure(ai_gateway::api::configure)
            .route("/v1/chat/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
            .route("/v1/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
            .route("/v1/models", web::get().to(ai_gateway::proxy::handler::openai_list_models))
            .route("/v1/messages", web::post().to(ai_gateway::proxy::handler::anthropic_messages))
            .route("/health", web::get().to(|| async { HttpResponse::Ok().json(serde_json::json!({"status": "ok"})) }))
            .service(actix_files::Files::new("/", &static_dir).index_file("index.html"))
    })
    .bind(format!("{}:{}", host, admin_port))?
    .run()
    .await
}
