use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::RwLock;

use ai_gateway::api::settings::SharedAppConfig;
use ai_gateway::cache::{RouteCache, StatsWriter};

/// Port the backend will listen on (determined at runtime)
static BACKEND_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
static BACKEND_READY: AtomicBool = AtomicBool::new(false);
/// 关窗前记下的页面地址。窗口是真实销毁的，重建后若不指定地址就会落到路由默认页，
/// 而旧行为（隐藏）是连着页面状态一起留住的——这里把"回到原来那页"补回来。
static LAST_URL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

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
            let backend_handle = app.handle().clone();
            std::thread::spawn(move || {
                start_backend_server(backend_handle);
            });

            // 不在 UI 线程忙等：.setup() 跑在主/UI 线程上，曾经 sleep 轮询最多 15s，
            // 窗口创建被推迟、界面表现为卡死。这里先建窗口，等后端就绪后由后台线程
            // 投递一次「打开主窗口」，前端在窗口里重试即可。
            open_main_window(app.handle());

            let ready_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut waited = 0u64;
                while !BACKEND_READY.load(Ordering::Acquire) && waited < 300 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    waited += 1;
                }
                if BACKEND_PORT.load(Ordering::Acquire) == 0 {
                    tracing::error!("Backend server failed to start in time!");
                    return;
                }
                // 后端真正就绪后再确保窗口指向正确端口（首次建窗可能落在未就绪的地址）
                open_main_window(&ready_handle);
            });

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
    let url = match resume_url(port) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to parse URL {}: {}", port, e);
            return;
        }
    };

    // 尺寸/标题/最小尺寸原先由 tauri.conf.json 的 windows 声明，现在改由这里等价给出。
    let target = url.to_string();
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

    // 关窗前把当前地址留下：CloseRequested 只给事件本身，所以要握住窗口句柄才能读到 url
    let handle_for_url = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { .. } = event {
            if let Ok(u) = handle_for_url.url() {
                if let Ok(mut guard) = LAST_URL.lock() {
                    *guard = Some(u.as_str().to_string());
                }
            }
        }
    });

    tracing::info!("主窗口已创建: {}", target);

    #[cfg(debug_assertions)]
    window.open_devtools();
    #[cfg(not(debug_assertions))]
    let _ = &window;
}

/// 重建窗口时优先回到关窗前那一页。旧行为是隐藏窗口、连着页面状态一起留住，
/// 现在窗口真的销毁了，不指定地址就会落到路由默认页，对常驻大屏的人是回退。
/// 只接受与后端同源的地址——用户在窗口里点开过的外部链接不能被当成首页恢复。
fn resume_url(port: u16) -> Result<url::Url, url::ParseError> {
    let origin = url::Url::parse(&format!("http://127.0.0.1:{}", port))?;
    let last = LAST_URL.lock().ok().and_then(|g| g.clone());
    if let Some(raw) = last {
        if let Ok(u) = url::Url::parse(&raw) {
            let same_origin = u.scheme() == origin.scheme()
                && u.host_str() == origin.host_str()
                && u.port() == origin.port();
            if same_origin {
                tracing::info!("重建回到上次页面: {}", u);
                return Ok(u);
            }
        }
    }
    Ok(origin)
}

/// 每日领取的登录窗口（必须在主线程调用）。
///
/// 每个 attempt 用独立的 WebView2 配置目录：否则第一个账号的会话会被第二个账号的
/// 登录窗口直接复用，窗口一打开就是「已登录」，抓回来的永远是同一个账号。
/// 窗口关闭后临时目录即丢弃，账号之间零串扰（代价是每个账号都要完整登录一次）。
///
/// 独立线程轮询 cookie：读 cookie 不能在同步命令/事件回调里做（Windows 上会死锁）。
fn open_checkin_login(
    handle: &tauri::AppHandle,
    pool: ai_gateway::db::DbPool,
    encrypt_key: String,
    platform: &'static str,
    attempt_id: String,
) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    const LABEL: &str = "checkin-login";

    if let Some(window) = handle.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    if platform != ai_gateway::checkin::MODELSCOPE {
        tracing::error!("未支持的每日领取平台: {}", platform);
        return;
    }
    let url = match url::Url::parse(ai_gateway::checkin::modelscope::LOGIN_URL) {
        Ok(url) => url,
        Err(error) => {
            tracing::error!("登录地址解析失败: {}", error);
            return;
        }
    };

    let profile_dir = std::env::temp_dir().join(format!(
        "aigw-checkin-login-{}",
        attempt_id.get(..8).unwrap_or(attempt_id.as_str())
    ));
    if let Err(error) = std::fs::create_dir_all(&profile_dir) {
        tracing::warn!("登录窗口配置目录创建失败（{}），改用默认目录", error);
    }

    let window = match WebviewWindowBuilder::new(handle, LABEL, WebviewUrl::External(url.clone()))
        .title("登录 ModelScope 魔搭")
        .inner_size(520., 760.)
        .min_inner_size(420., 620.)
        .center()
        .visible(true)
        .data_directory(profile_dir.clone())
        .build()
    {
        Ok(window) => window,
        Err(error) => {
            tracing::error!("登录窗口创建失败: {}", error);
            return;
        }
    };

    let handle = handle.clone();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        let session_cookie = ai_gateway::checkin::modelscope::SESSION_COOKIE;
        loop {
            // 用户自己关掉窗口就结束，不留孤儿线程
            if handle.get_webview_window(LABEL).is_none() {
                let _ = std::fs::remove_dir_all(&profile_dir);
                return;
            }
            if std::time::Instant::now() > deadline {
                let _ = window.close();
                let _ = std::fs::remove_dir_all(&profile_dir);
                return;
            }
            match window.cookies_for_url(url.clone()) {
                Ok(cookies) => {
                    let logged_in = cookies
                        .iter()
                        .any(|cookie| cookie.name() == session_cookie && !cookie.value().is_empty());
                    if logged_in {
                        let header = cookies
                            .iter()
                            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                            .collect::<Vec<_>>()
                            .join("; ");
                        let names = cookies
                            .iter()
                            .map(|cookie| cookie.name().to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build();
                        match runtime {
                            Ok(runtime) => runtime.block_on(
                                ai_gateway::checkin::complete_attempt(
                                    &pool,
                                    &encrypt_key,
                                    &attempt_id,
                                    &header,
                                    &names,
                                ),
                            ),
                            Err(error) => tracing::error!("登录线程 runtime 创建失败: {}", error),
                        }
                        let _ = window.close();
                        // WebView2 可能还握着目录句柄，删不掉就留给系统临时目录清理
                        let _ = std::fs::remove_dir_all(&profile_dir);
                        return;
                    }
                }
                Err(error) => tracing::warn!("读取登录窗口 cookie 失败: {}", error),
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    });
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = app.handle().clone();
    let quit = app.handle().clone();

    // 启动时先用「只有 Show / Quit」的轻量菜单建托盘，避免在 UI 线程上
    // init_pool（8 连接 + 跑迁移）再逐 proxy 跑统计 SQL —— 那是启动卡顿的来源。
    // 统计项随后在后台线程算好，再 set_menu 一次（托盘只在启动时建一次菜单，
    // 之后 set_menu 不会像事件回调里那样闪烁消失）。
    let menu = build_tray_menu_items(app.handle(), &[]);

    let tray = TrayIconBuilder::new()
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

    // 托盘统计项在后台算：单连接读、不建池、不跑迁移，算完把菜单整体 set 一次。
    // UI 线程不再被 DB 查询占住。菜单对象只在主线程上构建（set_menu 走主线程投递）。
    let refresh_handle = app.handle().clone();
    let tray_for_refresh = tray.clone();
    std::thread::spawn(move || {
        let entries = compute_tray_entries();
        if entries.is_empty() {
            return;
        }
        let handle = refresh_handle.clone();
        let menu_handle = refresh_handle.clone();
        let tray = tray_for_refresh.clone();
        let _ = handle.run_on_main_thread(move || {
            let menu = build_tray_menu_items(&menu_handle, &entries);
            let _ = tray.set_menu(Some(menu));
        });
    });

    Ok(())
}

/// 后台线程计算托盘菜单里的「模型名 + token 数」。
/// 用单连接读，不调用 init_pool —— 后者会建 8 连接池并跑一遍迁移。
fn compute_tray_entries() -> Vec<(String, String)> {
    let app_config = ai_gateway::config::AppConfig::load_or_default();
    match ai_gateway::db::tray_menu_entries(&app_config.resolved_db_path()) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("托盘菜单读取统计失败: {}", e);
            Vec::new()
        }
    }
}

/// 构建托盘菜单。`entries` 是后台线程预先算好的「模型名 + token 数」，
/// 空则只有 Show / Quit —— 菜单数据不再在 UI 线程上查库。
fn build_tray_menu_items(
    app: &tauri::AppHandle,
    entries: &[(String, String)],
) -> tauri::menu::Menu<tauri::Wry> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};

    let mut menu = MenuBuilder::new(app);

    // Show window option
    let show_item = MenuItemBuilder::with_id("show", "Show AI Gateway").build(app).expect("Failed to build show menu item");
    menu = menu.item(&show_item);

    // Add separator
    let sep1 = PredefinedMenuItem::separator(app).expect("Failed to build separator");
    menu = menu.item(&sep1);

    // Add virtual model token stats items
    for (id, label) in entries {
        let item = MenuItemBuilder::with_id(format!("proxy_info_{}", id), label.clone())
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

fn start_backend_server(app_handle: tauri::AppHandle) {
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

        // 每日领取的登录窗口只有桌面端能开：core 通过这个 opener 请求 Tauri 建窗，
        // 抓到 cookie 后再回调 core 加密入库。服务器模式不注册，接口会明确返回 409。
        {
            let handle = app_handle.clone();
            let pool = db_pool.clone();
            let encrypt_key = app_config.security.encrypt_key.clone();
            ai_gateway::checkin::register_login_opener(Box::new(
                move |platform, attempt_id| {
                    let dispatch = handle.clone();
                    let pool = pool.clone();
                    let encrypt_key = encrypt_key.clone();
                    handle
                        .run_on_main_thread(move || {
                            open_checkin_login(
                                &dispatch,
                                pool,
                                encrypt_key,
                                platform,
                                attempt_id,
                            )
                        })
                        .map_err(|error| error.to_string())
                },
            ));
        }

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
        let cache = RouteCache::new(db_pool.clone(), shared_config.read().security.encrypt_key.clone());
        // 控制台实时事件总线：与独立服务模式同一套语义
        let events_hub = ai_gateway::events::EventHub::new();
        let stats_writer = StatsWriter::start(db_pool.clone(), events_hub.clone());
        ai_gateway::maintenance::spawn_auto_purge(db_pool.clone(), shared_config.clone());
        // 每日领取：按平台页配的定时任务跑，每 60 秒看一眼任务表
        ai_gateway::checkin::spawn_checkin_runner(db_pool.clone(), shared_config.clone());
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
            quota_usage_cache: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set_last(v: Option<&str>) {
        let mut guard = LAST_URL.lock().unwrap();
        *guard = v.map(|s| s.to_string());
    }

    // 一个测试跑完所有分支：LAST_URL 是进程级全局，拆成多个 #[test] 会并行互踩。
    #[test]
    fn resume_url_restores_only_same_origin() {
        // 没有记录：落回后端首页
        set_last(None);
        assert_eq!(resume_url(1994).unwrap().as_str(), "http://127.0.0.1:1994/");

        // 同源深链（hash 路由带 query）原样恢复
        set_last(Some("http://127.0.0.1:1994/#/wall?v=3"));
        assert_eq!(resume_url(1994).unwrap().as_str(), "http://127.0.0.1:1994/#/wall?v=3");

        // 外部站点必须拒绝：否则重开的窗口会带着 shell:allow-open 的权限去加载别人的页面
        set_last(Some("https://example.com/#/wall"));
        assert_eq!(resume_url(1994).unwrap().as_str(), "http://127.0.0.1:1994/");

        // 同 host 不同端口也拒绝——换过端口的旧地址不算"上次那一页"
        set_last(Some("http://127.0.0.1:1995/#/traffic"));
        assert_eq!(resume_url(1994).unwrap().as_str(), "http://127.0.0.1:1994/");

        // 解析不了的串不能拖垮建窗
        set_last(Some("not a url"));
        assert_eq!(resume_url(1994).unwrap().as_str(), "http://127.0.0.1:1994/");

        set_last(None);
    }
}
