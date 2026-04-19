use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;

use crate::config::AppConfig;

/// Shared mutable config state
pub type SharedAppConfig = Arc<RwLock<AppConfig>>;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigResponse {
    pub admin_port: u16,
    pub host: String,
    pub log_level: String,
    pub lb_strategy: String,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    pub request_timeout_secs: u64,
    pub test_connection_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfigRequest {
    pub admin_port: Option<u16>,
    pub host: Option<String>,
    pub log_level: Option<String>,
    pub lb_strategy: Option<String>,
    pub max_retries: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
    pub request_timeout_secs: Option<u64>,
    pub test_connection_timeout_secs: Option<u64>,
}

impl From<&AppConfig> for ConfigResponse {
    fn from(cfg: &AppConfig) -> Self {
        ConfigResponse {
            admin_port: cfg.server.admin_port,
            host: cfg.server.host.clone(),
            log_level: cfg.server.log_level.clone(),
            lb_strategy: cfg.defaults.lb_strategy.clone(),
            max_retries: cfg.defaults.max_retries,
            retry_backoff_ms: cfg.defaults.retry_backoff_ms,
            request_timeout_secs: cfg.defaults.request_timeout_secs,
            test_connection_timeout_secs: cfg.defaults.test_connection_timeout_secs,
        }
    }
}

pub async fn get_config(config: web::Data<SharedAppConfig>) -> HttpResponse {
    let cfg = config.read();
    HttpResponse::Ok().json(ConfigResponse::from(&*cfg))
}

pub async fn update_config(
    config: web::Data<SharedAppConfig>,
    body: web::Json<UpdateConfigRequest>,
) -> HttpResponse {
    // Apply updates under write lock, then release lock before file I/O
    let (new_cfg, need_save) = {
        let mut cfg = config.write();
        let mut need_save = false;

        if let Some(port) = body.admin_port {
            if port > 0 && port <= 65535 {
                cfg.server.admin_port = port;
                need_save = true;
            }
        }
        if let Some(ref host) = body.host {
            cfg.server.host = host.clone();
            need_save = true;
        }
        if let Some(ref level) = body.log_level {
            cfg.server.log_level = level.clone();
            need_save = true;
        }
        if let Some(ref strategy) = body.lb_strategy {
            cfg.defaults.lb_strategy = strategy.clone();
            need_save = true;
        }
        if let Some(retries) = body.max_retries {
            cfg.defaults.max_retries = retries;
            need_save = true;
        }
        if let Some(backoff) = body.retry_backoff_ms {
            cfg.defaults.retry_backoff_ms = backoff;
            need_save = true;
        }
        if let Some(timeout) = body.request_timeout_secs {
            cfg.defaults.request_timeout_secs = timeout;
            need_save = true;
        }
        if let Some(timeout) = body.test_connection_timeout_secs {
            cfg.defaults.test_connection_timeout_secs = timeout;
            need_save = true;
        }

        let snapshot = cfg.clone();
        (snapshot, need_save)
    }; // write lock released here

    // Save to file outside the lock
    if need_save {
        if let Err(e) = save_config(&new_cfg) {
            return HttpResponse::InternalServerError().json(serde_json::json!({"error": e}));
        }
    }

    HttpResponse::Ok().json(ConfigResponse::from(&new_cfg))
}

fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let toml_str = cfg.to_toml_string()?;
    let config_path = crate::config::get_app_dir().join("config.toml");
    std::fs::write(&config_path, toml_str).map_err(|e| format!("Failed to write config: {}", e))
}
