use std::path::PathBuf;

/// 全局配置
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub security: SecurityConfig,
    pub defaults: DefaultsConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub admin_port: u16,
    pub log_level: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub encrypt_key: String,
    pub admin_token: String,
}

#[derive(Debug, Clone)]
pub struct DefaultsConfig {
    pub lb_strategy: String,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    pub request_timeout_secs: u64,
    pub test_connection_timeout_secs: u64,
}

/// Resolve a path relative to the executable's parent directory.
/// This ensures that both standalone server and Tauri desktop modes
/// can find data files (database, config, static files) correctly.
pub fn resolve_app_path(relative_path: &str) -> PathBuf {
    let relative = PathBuf::from(relative_path);

    // If it's already an absolute path, use it as-is
    if relative.is_absolute() {
        return relative;
    }

    // Try to resolve relative to the executable directory
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let resolved = exe_dir.join(&relative);
            if resolved.exists() || exe_dir.join("config.toml").exists() {
                return resolved;
            }

            // In macOS .app bundle, the exe is in Contents/MacOS/
            // Tauri bundles resources with _up_ prefix for "../" paths
            if cfg!(target_os = "macos") {
                if let Some(contents_dir) = exe_dir.parent() {
                    let resources_dir = contents_dir.join("Resources");

                    // Try Resources/_up_/ first (Tauri's ../ encoding)
                    let up_dir = resources_dir.join("_up_");
                    let up_resolved = up_dir.join(&relative);
                    if up_resolved.exists() {
                        return up_resolved;
                    }

                    // Try Resources/ directly
                    let resources_resolved = resources_dir.join(&relative);
                    if resources_resolved.exists() {
                        return resources_resolved;
                    }
                }
            }
        }
    }

    // Fallback: try current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let resolved = cwd.join(&relative);
        if resolved.exists() {
            return resolved;
        }
    }

    // Last resort: return the relative path as-is
    relative
}

/// Get the base directory for the application (where config.toml and data/ reside)
pub fn get_app_dir() -> PathBuf {
    // Try current exe directory first
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Direct: exe_dir/config.toml
            if exe_dir.join("config.toml").exists() {
                return exe_dir.to_path_buf();
            }

            // macOS .app bundle: exe is in Contents/MacOS/
            if cfg!(target_os = "macos") {
                if let Some(contents_dir) = exe_dir.parent() {
                    let resources_dir = contents_dir.join("Resources");

                    // Tauri bundles resources with _up_ prefix for "../" paths
                    // e.g., "../config.toml" -> "Resources/_up_/config.toml"
                    let up_dir = resources_dir.join("_up_");
                    if up_dir.join("config.toml").exists() {
                        return up_dir;
                    }

                    // Also check Resources/config.toml directly
                    if resources_dir.join("config.toml").exists() {
                        return resources_dir;
                    }
                }
            }
        }
    }

    // Fallback to current directory
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                admin_port: 1994,
                log_level: "info".to_string(),
            },
            database: DatabaseConfig {
                path: PathBuf::from("data/ai-gateway.db"),
            },
            security: SecurityConfig {
                encrypt_key: "ai-gateway-default-key-change-me".to_string(),
                admin_token: String::new(),
            },
            defaults: DefaultsConfig {
                lb_strategy: "RoundRobin".to_string(),
                max_retries: 2,
                retry_backoff_ms: 500,
                request_timeout_secs: 120,
                test_connection_timeout_secs: 10,
            },
        }
    }
}

impl AppConfig {
    pub fn load_or_default() -> Self {
        let app_dir = get_app_dir();
        let config_path = app_dir.join("config.toml");

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    let value: toml::Value = match toml::from_str(&content) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!("Failed to parse config.toml: {}, using defaults", e);
                            return Self::default();
                        }
                    };
                    let mut config = Self::default();
                    if let Some(server) = value.get("server") {
                        if let Some(v) = server.get("host").and_then(|v| v.as_str()) {
                            config.server.host = v.to_string();
                        }
                        if let Some(v) = server.get("admin_port").and_then(|v| v.as_integer()) {
                            config.server.admin_port = v as u16;
                        }
                        if let Some(v) = server.get("log_level").and_then(|v| v.as_str()) {
                            config.server.log_level = v.to_string();
                        }
                    }
                    if let Some(db) = value.get("database") {
                        if let Some(v) = db.get("path").and_then(|v| v.as_str()) {
                            config.database.path = PathBuf::from(v);
                        }
                    }
                    if let Some(sec) = value.get("security") {
                        if let Some(v) = sec.get("encrypt_key").and_then(|v| v.as_str()) {
                            config.security.encrypt_key = v.to_string();
                        }
                        if let Some(v) = sec.get("admin_token").and_then(|v| v.as_str()) {
                            config.security.admin_token = v.to_string();
                        }
                    }
                    if let Some(def) = value.get("defaults") {
                        if let Some(v) = def.get("lb_strategy").and_then(|v| v.as_str()) {
                            config.defaults.lb_strategy = v.to_string();
                        }
                        if let Some(v) = def.get("max_retries").and_then(|v| v.as_integer()) {
                            config.defaults.max_retries = v as u32;
                        }
                        if let Some(v) = def.get("retry_backoff_ms").and_then(|v| v.as_integer()) {
                            config.defaults.retry_backoff_ms = v as u64;
                        }
                        if let Some(v) = def.get("request_timeout_secs").and_then(|v| v.as_integer()) {
                            config.defaults.request_timeout_secs = v as u64;
                        }
                        if let Some(v) = def.get("test_connection_timeout_secs").and_then(|v| v.as_integer()) {
                            config.defaults.test_connection_timeout_secs = v as u64;
                        }
                    }
                    config
                }
                Err(e) => {
                    tracing::error!("Failed to read config.toml: {}, using defaults", e);
                    Self::default()
                }
            }
        } else {
            let config = Self::default();
            // Generate default config file in app directory
            if let Ok(toml_str) = config.to_toml_string() {
                let _ = std::fs::write(&config_path, toml_str);
            }
            config
        }
    }

    /// Resolve the database path relative to the app directory
    pub fn resolved_db_path(&self) -> PathBuf {
        let app_dir = get_app_dir();
        let path = &self.database.path;
        if path.is_absolute() {
            path.clone()
        } else {
            app_dir.join(path)
        }
    }

    /// Get the static files directory path
    pub fn static_dir(&self) -> PathBuf {
        let app_dir = get_app_dir();
        app_dir.join("static")
    }

    pub fn to_toml_string(&self) -> Result<String, String> {
        Ok(format!(
            r#"[server]
host = "{}"
admin_port = {}
log_level = "{}"

[database]
path = "{}"

[security]
encrypt_key = "{}"
admin_token = "{}"

[defaults]
lb_strategy = "{}"
max_retries = {}
retry_backoff_ms = {}
request_timeout_secs = {}
test_connection_timeout_secs = {}
"#,
            self.server.host,
            self.server.admin_port,
            self.server.log_level,
            self.database.path.display(),
            self.security.encrypt_key,
            self.security.admin_token,
            self.defaults.lb_strategy,
            self.defaults.max_retries,
            self.defaults.retry_backoff_ms,
            self.defaults.request_timeout_secs,
            self.defaults.test_connection_timeout_secs,
        ))
    }
}
