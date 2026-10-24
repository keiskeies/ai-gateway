use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::RwLock;

use ai_gateway::api::settings::SharedAppConfig;
use ai_gateway::cache::{RouteCache, StatsWriter};

/// Port the backend will listen on (determined at runtime)
static BACKEND_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
static BACKEND_READY: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Determine the correct app directory by resolving the config.toml resource.
            // Tauri's resolve() API handles platform-specific resource path mapping:
            // - macOS: ../config.toml -> Contents/Resources/_up_/config.toml
            // - Windows: ../config.toml -> installed dir structure
            // - Linux: appropriate data directory
            use tauri::path::BaseDirectory;
            use tauri::Manager;
            let config_resource = app.path().resolve("../config.toml", BaseDirectory::Resource)
                .expect("Failed to resolve config.toml resource path");
            // The app directory is where config.toml lives
            let app_dir = config_resource.parent()
                .expect("config.toml resource has no parent directory")
                .to_path_buf();
            tracing::info!("Tauri resolved app_dir: {:?}", app_dir);
            ai_gateway::config::set_app_dir(app_dir);

            // Setup system tray
            setup_tray(app)?;

            // Start the backend actix-web server in a background thread
            std::thread::spawn(move || {
                start_backend_server();
            });

            // Wait for backend to be ready (up to 15 seconds)
            let mut waited = 0u64;
            while !BACKEND_READY.load(Ordering::Relaxed) && waited < 150 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                waited += 1;
            }

            let port = BACKEND_PORT.load(Ordering::Relaxed);
            if port == 0 {
                tracing::error!("Backend server failed to start in time!");
                return Ok(());
            }

            open_main_window(app.handle());

            Ok(())
        })
        // 用 build + run 回调而不是 Builder::run：需要在 RunEvent 里拦住"最后一个窗口被关闭"
        // 触发的自发退出，否则关掉控制台就等于杀掉整个网关。
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_handle, event| {
        if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
            // code == None 是"没有窗口了所以想退"，这里拦住：托盘与后端要继续跑。
            // 带退出码的退出（app.exit / restart）不受影响，托盘 Quit 走 process::exit。
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

/// 唤起主窗口：窗口在则前置，窗口已销毁则重建并直接指向后端地址。
///
/// 关窗走 WebView 的真实销毁（而不是 hide），这样 renderer 与 gpu 子进程会退出——
/// 常驻托盘时这一档能省下整个 WebView2 的量级。重建代价是一次页面加载。
fn open_main_window(handle: &tauri::AppHandle) {
    // Windows 上在事件回调里同步建窗会死锁（tauri WebviewWindowBuilder 文档），
    // 所以托盘点击与启动都走这一条投递到主线程的路径。
    let app = handle.clone();
    if let Err(e) = handle.run_on_main_thread(move || create_or_focus_window(&app)) {
        tracing::error!("投递窗口创建任务失败: {}", e);
    }
}

fn create_or_focus_window(handle: &tauri::AppHandle) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let port = BACKEND_PORT.load(Ordering::Relaxed);
    if port == 0 {
        tracing::error!("后端端口未就绪，无法打开窗口");
        return;
    }
    let url = match url::Url::parse(&format!("http://127.0.0.1:{}", port)) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to parse URL {}: {}", port, e);
            return;
        }
    };

    // 尺寸/标题/最小尺寸原先由 tauri.conf.json 的 windows 声明，现在改由这里等价给出。
    let window = match WebviewWindowBuilder::new(handle, "main", WebviewUrl::External(url))
        .title("AI Gateway")
        .inner_size(1280., 800.)
        .min_inner_size(960., 600.)
        .resizable(true)
        .center()
        .visible(true)
        .build()
    {
        Ok(w) => w,
        Err(e) => {
            // 上一次关闭还在事件循环里排队时标签可能未释放；此时窗口对象仍在，直接唤起
            tracing::error!("窗口创建失败: {}", e);
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            return;
        }
    };

    tracing::info!("主窗口已创建: http://127.0.0.1:{}", port);

    #[cfg(debug_assertions)]
    window.open_devtools();
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = app.handle().clone();
    let quit = app.handle().clone();

    // 启动时构建一次菜单，避免在事件回调中动态 set_menu 导致菜单闪烁消失
    let menu = build_tray_menu_items(app.handle());

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("AI Gateway")
        .menu(&menu)
        .on_tray_icon_event(move |_tray, event| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    // Left click: show main window（窗口已销毁时重建）
                    open_main_window(&show);
                }
                _ => {}
            }
        })
        .on_menu_event(move |_tray, event| {
            let id = event.id().as_ref().to_string();
            match id.as_str() {
                "show" => {
                    open_main_window(&quit);
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn build_tray_menu_items(app: &tauri::AppHandle) -> tauri::menu::Menu<tauri::Wry> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};

    let db_path = {
        let app_config = ai_gateway::config::AppConfig::load_or_default();
        app_config.resolved_db_path()
    };

    let db_pool = ai_gateway::db::init_pool(&db_path).expect("Failed to init DB for tray menu");

    // Get proxies for tray menu - show token stats instead of toggle
    let proxies = ai_gateway::db::proxy::list(&db_pool).unwrap_or_default();

    let mut menu = MenuBuilder::new(app);

    // Show window option
    let show_item = MenuItemBuilder::with_id("show", "Show AI Gateway").build(app).expect("Failed to build show menu item");
    menu = menu.item(&show_item);

    // Add separator
    let sep1 = PredefinedMenuItem::separator(app).expect("Failed to build separator");
    menu = menu.item(&sep1);

    // Add virtual model token stats items
    for proxy in &proxies {
        // Get token stats for this proxy（托盘菜单固定展示全量口径，不做时段过滤）
        let token_stats = ai_gateway::db::stats::proxy_stats(&db_pool, &proxy.id, None, None).ok();
        let total_tokens = token_stats
            .as_ref()
            .map(|s| s.total_token_input + s.total_token_output)
            .unwrap_or(0);
        let token_str = if total_tokens >= 1_000_000 {
            format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
        } else if total_tokens >= 1_000 {
            format!("{:.1}K", total_tokens as f64 / 1_000.0)
        } else {
            total_tokens.to_string()
        };
        let label = format!("{}  Tokens: {}", proxy.name, token_str);
        let item = MenuItemBuilder::with_id(format!("proxy_info_{}", proxy.id), label)
            .enabled(false)
            .build(app)
            .expect("Failed to build proxy menu item");
        menu = menu.item(&item);
    }

    // Add separator and quit
    let sep2 = PredefinedMenuItem::separator(app).expect("Failed to build separator");
    menu = menu.item(&sep2);
    let quit_item = MenuItemBuilder::with_id("quit", "Quit AI Gateway").build(app).expect("Failed to build quit menu item");
    menu = menu.item(&quit_item);

    menu.build().expect("Failed to build tray menu")
}

fn start_backend_server() {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async {
        use actix_cors::Cors;
        use actix_web::{web, App, HttpServer, HttpResponse, middleware};
        use actix_files as actix_files;

        let app_config = ai_gateway::config::AppConfig::load_or_default();

        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&app_config.server.log_level)))
            .try_init();

        tracing::info!("AI Gateway backend starting (Tauri mode)...");
        tracing::info!("App directory: {:?}", ai_gateway::config::get_app_dir());

        let db_path = app_config.resolved_db_path();
        tracing::info!("Database path: {:?}", db_path);

        let db_pool = ai_gateway::db::init_pool(&db_path)
            .expect("Failed to initialize database");

        // 自动检测系统代理
        let system_proxy = ai_gateway::config::SystemProxy::detect();
        let http_client = system_proxy.apply_to_builder(
            // 不设 Client 级 timeout：流式长生成远超 request_timeout，
            // 各请求点均单独设置 per-request timeout
            reqwest::Client::builder()
        ).build().expect("Failed to create HTTP client");

        let shared_config: SharedAppConfig = Arc::new(RwLock::new(app_config.clone()));
        let selector = Arc::new(ai_gateway::lb::BackendSelector::new());

        // 初始化内存缓存和异步统计写入器
        let cache = RouteCache::new(db_pool.clone());
        // 控制台实时事件总线：与独立服务模式同一套语义
        let events_hub = ai_gateway::events::EventHub::new();
        let stats_writer = StatsWriter::start(db_pool.clone(), events_hub.clone());
        let api_cache = Arc::new(cache.clone());
        let login_limiter = web::Data::new(ai_gateway::api::auth::LoginLimiter::new());

        let proxy_state = Arc::new(ai_gateway::proxy::handler::ProxyState {
            db: db_pool.clone(),
            // 与 /api/settings 同一份 Arc：改完超时或正文采集开关立即生效，不需要重启
            config: shared_config.clone(),
            selector,
            http_client,
            cache,
            stats_writer,
        });

        let static_dir = app_config.static_dir();
        let index_file = static_dir.join("index.html");
        let host = app_config.server.host.clone();
        let admin_port = app_config.server.admin_port;

        tracing::info!("Static dir: {:?}", static_dir);
        tracing::info!("Will serve frontend + API on port {}", admin_port);

        let server = HttpServer::new(move || {
            let cors = Cors::permissive();

            App::new()
                .wrap(cors)
                .wrap(middleware::Logger::default())
                // 静态资源缓存策略：index.html 必须回源校验，/assets/* 强缓存
                .wrap(middleware::from_fn(ai_gateway::web_cache::cache_headers))
                .app_data(web::Data::new(db_pool.clone()))
                .app_data(web::Data::new(proxy_state.clone()))
                .app_data(web::Data::new(shared_config.clone()))
                .app_data(web::Data::new(api_cache.clone()))
                .app_data(web::Data::new(events_hub.clone()))
                .app_data(login_limiter.clone())
                // JSON body 上限 64MB：actix 默认仅 2MB，长上下文 / base64 图片请求过网关会被 413 拦截
                .app_data(web::JsonConfig::default().limit(64 * 1024 * 1024))
                .configure(ai_gateway::api::configure)
                // Global virtual model proxy routes — all on admin port, model name in request body
                .route("/v1/chat/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
                .route("/v1/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
                .route("/v1/models", web::get().to(ai_gateway::proxy::handler::openai_list_models))
                .route("/v1/messages", web::post().to(ai_gateway::proxy::handler::anthropic_messages))
                .route("/health", web::get().to(|| async { HttpResponse::Ok().json(serde_json::json!({"status": "ok"})) }))
                .service(
                    actix_files::Files::new("/", &static_dir)
                        .index_file("index.html")
                        // SPA 深链回落，与独立服务模式保持一致：带扩展名的资源缺失仍按 404 返回，
                        // 避免把 404 的 JS 当成模块喂给浏览器
                        .default_handler(actix_web::dev::fn_service({
                            let index_file = index_file.clone();
                            move |req: actix_web::dev::ServiceRequest| {
                                let index = index_file.clone();
                                let is_asset = req.path().contains('.');
                                async move {
                                    let res = match tokio::fs::read(&index).await {
                                        Ok(body) if !is_asset => HttpResponse::Ok()
                                            .insert_header((
                                                actix_web::http::header::CONTENT_TYPE,
                                                "text/html; charset=utf-8",
                                            ))
                                            .body(body),
                                        _ => HttpResponse::NotFound().finish(),
                                    };
                                    Ok(req.into_response(res))
                                }
                            }
                        })),
                )
        })
        .bind(format!("{}:{}", host, admin_port))
        .expect("Failed to bind server");

        // bind 成功后才公布端口与就绪标记：端口被占时这里会 panic，
        // 全局仍保持 0，主线程就不会把一个"其实没起来"的服务当成可用后端去建窗口
        // （否则控制台会连到同端口的另一个实例上，读写到另一份库）。
        BACKEND_PORT.store(admin_port, Ordering::Relaxed);
        BACKEND_READY.store(true, Ordering::Relaxed);

        server.run().await.expect("Server error");
    });
}
