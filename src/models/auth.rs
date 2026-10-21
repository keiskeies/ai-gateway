use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
}

/// 通过会话中间件注入 request extensions 的当前登录用户
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub token_hash: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub initialized: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    /// 公开返回监听端口，供 Tauri 在未登录时也能发现后端
    pub admin_port: u16,
}

#[derive(Debug, Serialize)]
pub struct AuthSuccessResponse {
    pub username: String,
}
