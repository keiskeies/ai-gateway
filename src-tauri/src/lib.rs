use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::RwLock;

use ai_gateway::api::settings::SharedAppConfig;
use ai_gateway::api::sync::{SharedSyncState, SyncState, start_scheduler};

/// Port the backend will listen on (determined at runtime)
static BACKEND_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
static BACKEND_READY: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            let url = format!("http://127.0.0.1:{}", port);
            tracing::info!("Loading frontend from: {}", url);

            // Navigate the main window to the backend URL using Tauri's navigate API
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                // On close requested, hide to tray instead of closing
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        window_clone.hide().unwrap_or_else(|e| {
                            tracing::warn!("Failed to hide window: {}", e);
                        });
                    }
                });

                match url::Url::parse(&url) {
                    Ok(parsed_url) => {
                        if let Err(e) = window.navigate(parsed_url) {
                            tracing::error!("Failed to navigate window: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse URL {}: {}", url, e);
                    }
                }
            }

            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::Manager;

    let show = app.handle().clone();
    let quit = app.handle().clone();

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("AI Gateway")
        .on_tray_icon_event(move |tray, event| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    // Left click: show main window
                    if let Some(window) = show.get_webview_window("main") {
                        window.show().unwrap_or_else(|e| {
                            tracing::warn!("Failed to show window: {}", e);
                        });
                        window.set_focus().unwrap_or_else(|e| {
                            tracing::warn!("Failed to focus window: {}", e);
                        });
                    }
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    // Right click: show context menu
                    let items = build_tray_menu_items(&show);
                    tray.set_menu(Some(items)).unwrap_or_else(|e| {
                        tracing::warn!("Failed to set tray menu: {}", e);
                    });
                }
                _ => {}
            }
        })
        .on_menu_event(move |_tray, event| {
            let id = event.id().as_ref().to_string();
            match id.as_str() {
                "show" => {
                    if let Some(window) = quit.get_webview_window("main") {
                        window.show().unwrap_or_else(|e| {
                            tracing::warn!("Failed to show window: {}", e);
                        });
                        window.set_focus().unwrap_or_else(|e| {
                            tracing::warn!("Failed to focus window: {}", e);
                        });
                    }
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
        // Get token stats for this proxy
        let token_stats = ai_gateway::db::stats::proxy_stats(&db_pool, &proxy.id).ok();
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

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(app_config.defaults.request_timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        let shared_config: SharedAppConfig = Arc::new(RwLock::new(app_config.clone()));
        let selector = Arc::new(ai_gateway::lb::BackendSelector::new());

        // Initialize auto-sync scheduler
        let sync_state: SharedSyncState = Arc::new(SyncState::new(db_pool.clone(), shared_config.clone()));
        start_scheduler(sync_state.clone());

        let proxy_state = Arc::new(ai_gateway::proxy::handler::ProxyState {
            db: db_pool.clone(),
            config: app_config.clone(),
            selector,
            http_client,
        });

        let static_dir = app_config.static_dir();
        let host = app_config.server.host.clone();
        let admin_port = app_config.server.admin_port;

        tracing::info!("Static dir: {:?}", static_dir);
        tracing::info!("Will serve frontend + API on port {}", admin_port);

        // Store the port so the main thread can find it
        BACKEND_PORT.store(admin_port, Ordering::Relaxed);
        BACKEND_READY.store(true, Ordering::Relaxed);

        HttpServer::new(move || {
            let cors = Cors::permissive();

            App::new()
                .wrap(cors)
                .wrap(middleware::Logger::default())
                .app_data(web::Data::new(db_pool.clone()))
                .app_data(web::Data::new(proxy_state.clone()))
                .app_data(web::Data::new(shared_config.clone()))
                .configure(ai_gateway::api::configure)
                // Global virtual model proxy routes — all on admin port, model name in request body
                .route("/v1/chat/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
                .route("/v1/completions", web::post().to(ai_gateway::proxy::handler::openai_chat_completions))
                .route("/v1/models", web::get().to(ai_gateway::proxy::handler::openai_list_models))
                .route("/v1/messages", web::post().to(ai_gateway::proxy::handler::anthropic_messages))
                .route("/health", web::get().to(|| async { HttpResponse::Ok().json(serde_json::json!({"status": "ok"})) }))
                .service(actix_files::Files::new("/", &static_dir).index_file("index.html"))
        })
        .bind(format!("{}:{}", host, admin_port))
        .expect("Failed to bind server")
        .run()
        .await
        .expect("Server error");
    });
}
