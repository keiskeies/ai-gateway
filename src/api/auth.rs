use actix_web::body::MessageBody;
use actix_web::cookie::{time::Duration as CookieDuration, Cookie, SameSite};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::error::InternalError;
use actix_web::http::Method;
use actix_web::middleware::Next;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration as StdDuration, Instant};

use crate::api::settings::SharedAppConfig;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::auth::{
    AuthStatusResponse, AuthSuccessResponse, AuthUser, ChangePasswordRequest, LoginRequest,
    Session, SetupRequest,
};

pub const SESSION_COOKIE: &str = "aigw_session";
const SESSION_TTL_DAYS: i64 = 7;
const MAX_LOGIN_FAILURES: u32 = 5;
const LOGIN_LOCK_SECS: u64 = 300;

/// 登录失败节流：按来源 IP 统计，5 次失败锁定 5 分钟
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, LoginAttempt>>,
}

struct LoginAttempt {
    failures: u32,
    locked_until: Option<Instant>,
    last_failure: Instant,
}

impl LoginLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    fn check(&self, key: &str) -> Result<(), u64> {
        let mut attempts = self.attempts.lock();
        if let Some(attempt) = attempts.get_mut(key) {
            if let Some(until) = attempt.locked_until {
                let now = Instant::now();
                if until > now {
                    return Err(until.duration_since(now).as_secs().max(1));
                }
                attempt.locked_until = None;
                attempt.failures = 0;
            }
        }
        Ok(())
    }

    fn record_failure(&self, key: &str) {
        let now = Instant::now();
        let mut attempts = self.attempts.lock();
        if attempts.len() > 1024 {
            attempts.retain(|_, a| now.duration_since(a.last_failure).as_secs() < 3600);
        }
        let attempt = attempts.entry(key.to_string()).or_insert_with(|| LoginAttempt {
            failures: 0,
            locked_until: None,
            last_failure: now,
        });
        attempt.failures += 1;
        attempt.last_failure = now;
        if attempt.failures >= MAX_LOGIN_FAILURES {
            attempt.locked_until = Some(now + StdDuration::from_secs(LOGIN_LOCK_SECS));
        }
    }

    fn reset(&self, key: &str) {
        self.attempts.lock().remove(key);
    }
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn client_key(req: &HttpRequest) -> String {
    req.peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn is_public_path(path: &str) -> bool {
    matches!(
        path.trim_end_matches('/'),
        "/api/auth/status" | "/api/auth/login" | "/api/auth/setup"
    )
}

fn session_token(req: &HttpRequest) -> Option<String> {
    req.cookie(SESSION_COOKIE)
        .filter(|c| !c.value().is_empty())
        .map(|c| c.value().to_string())
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    PasswordHash::new(password_hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// 用户不存在时也做一次哈希校验，避免通过响应耗时枚举用户名
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("ai-gateway-dummy-password").unwrap_or_default())
}

fn validate_username(username: &str) -> Result<(), AppError> {
    let len = username.chars().count();
    if !(3..=64).contains(&len) {
        return Err(AppError::BadRequest(
            "Username must be 3-64 characters".to_string(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::BadRequest(
            "Username may only contain letters, digits, dot, dash or underscore".to_string(),
        ));
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    if password.len() > 128 {
        return Err(AppError::BadRequest("Password is too long".to_string()));
    }
    Ok(())
}

fn session_cookie(token: &str) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.to_string());
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_max_age(CookieDuration::days(SESSION_TTL_DAYS));
    cookie
}

pub(crate) fn clear_session_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_max_age(CookieDuration::seconds(0));
    cookie
}

/// 结束当前会话：删掉 sessions 行并让 Cookie 失效。
/// 整库还原后要用——sessions 表跟着一起回滚了，与其让用户带着一条可能已不存在的会话继续点，
/// 不如明确退回登录页。
pub(crate) async fn drop_session(req: &HttpRequest, db: &web::Data<DbPool>) -> AppResult<()> {
    if let Some(token) = session_token(req) {
        let token_hash = hash_token(&token);
        let pool = db.get_ref().clone();
        web::block(move || crate::db::auth::delete_session(&pool, &token_hash))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??;
    }
    Ok(())
}

fn create_session(pool: &DbPool, user_id: &str) -> AppResult<String> {
    let mut raw = [0u8; 32];
    let mut rng = OsRng;
    rng.fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let now = Utc::now();
    crate::db::auth::purge_expired_sessions(pool, &now.to_rfc3339())?;
    crate::db::auth::create_session(
        pool,
        &Session {
            token_hash: hash_token(&token),
            user_id: user_id.to_string(),
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::days(SESSION_TTL_DAYS)).to_rfc3339(),
        },
    )?;
    Ok(token)
}

fn current_user(req: &HttpRequest, pool: &DbPool) -> AppResult<Option<AuthUser>> {
    let Some(token) = session_token(req) else {
        return Ok(None);
    };
    let token_hash = hash_token(&token);
    Ok(crate::db::auth::find_session(pool, &token_hash)?.map(|(_, user)| user))
}

/// `/api` 作用域中间件：除登录、初始化、状态探测外均要求有效会话。
pub async fn require_auth<B: MessageBody>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<B>, actix_web::Error> {
    if req.method() == Method::OPTIONS || is_public_path(req.path()) {
        return next.call(req).await;
    }

    let Some(db) = req.app_data::<web::Data<DbPool>>() else {
        return Err(InternalError::from_response(
            "",
            AppError::Internal("Database not configured".to_string()).error_response(),
        )
        .into());
    };
    let pool = db.get_ref().clone();

    match current_user(req.request(), &pool) {
        Ok(Some(user)) => {
            req.extensions_mut().insert(user);
            next.call(req).await
        }
        Ok(None) => Err(InternalError::from_response(
            "",
            AppError::Unauthorized("Authentication required".to_string()).error_response(),
        )
        .into()),
        Err(e) => Err(InternalError::from_response("", e.error_response()).into()),
    }
}

fn no_store(mut resp: HttpResponse) -> HttpResponse {
    resp.headers_mut().insert(
        actix_web::http::header::CACHE_CONTROL,
        actix_web::http::header::HeaderValue::from_static("no-store"),
    );
    resp
}

pub async fn status(
    req: HttpRequest,
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let pool = db.get_ref().clone();
    let count_pool = pool.clone();
    let initialized = web::block(move || crate::db::auth::user_count(&count_pool))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?? > 0;
    let user = current_user(&req, &pool)?;
    Ok(no_store(HttpResponse::Ok().json(AuthStatusResponse {
        initialized,
        authenticated: user.is_some(),
        username: user.map(|u| u.username),
        admin_port: config.read().server.admin_port,
        local_console: crate::api::local::is_local_console(&req),
    })))
}

pub async fn setup(
    req: HttpRequest,
    db: web::Data<DbPool>,
    body: web::Json<SetupRequest>,
) -> AppResult<HttpResponse> {
    // 首次初始化只允许本机操作，避免服务暴露到局域网时被他人抢先注册管理员
    let from_loopback = req
        .peer_addr()
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false);
    if !from_loopback {
        return Err(AppError::Unauthorized(
            "Initial setup is only allowed from localhost".to_string(),
        ));
    }

    let username = body.username.trim().to_string();
    validate_username(&username)?;
    validate_password(&body.password)?;

    let password = body.password.clone();
    let pool = db.get_ref().clone();
    let create_pool = pool.clone();
    let username_for_hash = username.clone();
    let created = web::block(move || {
        let password_hash = hash_password(&password)?;
        crate::db::auth::create_initial_user(&create_pool, &username_for_hash, &password_hash)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let Some(user) = created else {
        return Err(AppError::Conflict(
            "AI Gateway has already been initialized".to_string(),
        ));
    };

    let token = create_session(&pool, &user.id)?;
    Ok(no_store(
        HttpResponse::Ok()
            .cookie(session_cookie(&token))
            .json(AuthSuccessResponse {
                username: user.username,
            }),
    ))
}

pub async fn login(
    req: HttpRequest,
    db: web::Data<DbPool>,
    limiter: web::Data<LoginLimiter>,
    body: web::Json<LoginRequest>,
) -> AppResult<HttpResponse> {
    let key = client_key(&req);
    if let Err(retry_after) = limiter.check(&key) {
        return Err(AppError::TooManyRequests(format!(
            "Too many failed login attempts, try again in {} seconds",
            retry_after
        )));
    }

    let username = body.username.trim().to_string();
    let password = body.password.clone();
    if username.is_empty() || password.is_empty() {
        limiter.record_failure(&key);
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let pool = db.get_ref().clone();
    let lookup_pool = pool.clone();
    let (user, valid) = web::block(move || -> AppResult<(Option<crate::models::auth::User>, bool)> {
        let user = crate::db::auth::find_user_by_username(&lookup_pool, &username)?;
        let valid = match &user {
            Some(user) => verify_password(&password, &user.password_hash),
            None => {
                verify_password(&password, dummy_hash());
                false
            }
        };
        Ok((user, valid))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let Some(user) = user.filter(|_| valid) else {
        limiter.record_failure(&key);
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    };

    limiter.reset(&key);
    let token = create_session(&pool, &user.id)?;
    Ok(no_store(
        HttpResponse::Ok()
            .cookie(session_cookie(&token))
            .json(AuthSuccessResponse {
                username: user.username,
            }),
    ))
}

pub async fn logout(req: HttpRequest, db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    drop_session(&req, &db).await?;
    Ok(no_store(
        HttpResponse::Ok()
            .cookie(clear_session_cookie())
            .json(serde_json::json!({ "ok": true })),
    ))
}

pub async fn change_password(
    req: HttpRequest,
    db: web::Data<DbPool>,
    body: web::Json<ChangePasswordRequest>,
) -> AppResult<HttpResponse> {
    let auth_user = req
        .extensions()
        .get::<AuthUser>()
        .cloned()
        .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;
    validate_password(&body.new_password)?;

    let current_password = body.current_password.clone();
    let new_password = body.new_password.clone();
    let user_id = auth_user.id.clone();
    let pool = db.get_ref().clone();
    let update_pool = pool.clone();
    let changed = web::block(move || -> AppResult<bool> {
        let user = crate::db::auth::find_user_by_id(&update_pool, &user_id)?
            .ok_or_else(|| AppError::Unauthorized("Account no longer exists".to_string()))?;
        if !verify_password(&current_password, &user.password_hash) {
            return Ok(false);
        }
        let password_hash = hash_password(&new_password)?;
        crate::db::auth::update_password(&update_pool, &user.id, &password_hash)?;
        crate::db::auth::delete_user_sessions(&update_pool, &user.id)?;
        Ok(true)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if !changed {
        return Err(AppError::Unauthorized(
            "Current password is incorrect".to_string(),
        ));
    }

    let token = create_session(&pool, &auth_user.id)?;
    Ok(no_store(
        HttpResponse::Ok()
            .cookie(session_cookie(&token))
            .json(AuthSuccessResponse {
                username: auth_user.username,
            }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test as aw_test, App};
    use std::sync::Arc;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("auth-test-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::init_pool(&path).expect("init test db");
        (pool, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn session_header(token: &str) -> (&'static str, String) {
        ("Cookie", format!("{}={}", SESSION_COOKIE, token))
    }

    fn cookie_value(resp: &ServiceResponse<impl MessageBody>) -> String {
        resp.response()
            .cookies()
            .find(|c| c.name() == SESSION_COOKIE)
            .expect("session cookie")
            .value()
            .to_string()
    }

    /// 中间件返回 Err 时测试工具会直接抛出，这里统一取到 HTTP 状态码
    macro_rules! call_status {
        ($app:expr, $req:expr) => {{
            match aw_test::try_call_service($app, $req).await {
                Ok(resp) => resp.status(),
                Err(err) => err.as_response_error().status_code(),
            }
        }};
    }

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn validates_credentials() {
        assert!(validate_username("admin").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("bad name").is_err());
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
    }

    #[actix_web::test]
    async fn admin_api_requires_login_and_supports_setup() {
        let (pool, path) = test_db();
        let app = aw_test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(LoginLimiter::new()))
                .app_data(web::Data::new(
                    Arc::new(parking_lot::RwLock::new(crate::config::AppConfig::default()))
                        as SharedAppConfig,
                ))
                .configure(crate::api::configure),
        )
        .await;

        // 未初始化且未登录
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::get().uri("/api/auth/status").to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = aw_test::read_body_json(resp).await;
        assert_eq!(body["initialized"], false);

        // 管理接口被拦截
        let status = call_status!(
            &app,
            aw_test::TestRequest::get().uri("/api/platforms").to_request()
        );
        assert_eq!(status, 401);

        // 非本机不能初始化
        let status = call_status!(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/setup")
                .peer_addr("192.168.1.50:5000".parse().unwrap())
                .set_json(serde_json::json!({"username": "attacker", "password": "secret123"}))
                .to_request()
        );
        assert_eq!(status, 401);

        // 首次初始化并自动登录（仅本机）
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/setup")
                .peer_addr("127.0.0.1:54321".parse().unwrap())
                .set_json(serde_json::json!({"username": "admin", "password": "secret123"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let setup_cookie = cookie_value(&resp);

        // 已初始化后不能再次 setup
        let status = call_status!(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/setup")
                .peer_addr("127.0.0.1:54321".parse().unwrap())
                .set_json(serde_json::json!({"username": "other", "password": "secret123"}))
                .to_request()
        );
        assert_eq!(status, 409);

        // 携带会话可访问管理接口
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::get()
                .uri("/api/platforms")
                .insert_header(session_header(&setup_cookie))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        // 错误密码被拒绝
        let status = call_status!(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/login")
                .set_json(serde_json::json!({"username": "admin", "password": "wrong-pass"}))
                .to_request()
        );
        assert_eq!(status, 401);

        // 正确密码可登录
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/login")
                .set_json(serde_json::json!({"username": "admin", "password": "secret123"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let login_cookie = cookie_value(&resp);

        // 修改密码会使旧会话失效并返回新会话
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::put()
                .uri("/api/auth/password")
                .insert_header(session_header(&login_cookie))
                .set_json(serde_json::json!({
                    "current_password": "secret123",
                    "new_password": "new-secret456"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let new_cookie = cookie_value(&resp);

        let status = call_status!(
            &app,
            aw_test::TestRequest::get()
                .uri("/api/settings")
                .insert_header(session_header(&setup_cookie))
                .to_request()
        );
        assert_eq!(status, 401);

        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::get()
                .uri("/api/settings")
                .insert_header(session_header(&new_cookie))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        // 登出后会话失效
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/logout")
                .insert_header(session_header(&new_cookie))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        let status = call_status!(
            &app,
            aw_test::TestRequest::get()
                .uri("/api/settings")
                .insert_header(session_header(&new_cookie))
                .to_request()
        );
        assert_eq!(status, 401);

        drop(app);
        drop(pool);
        cleanup_db(&path);
    }

    #[actix_web::test]
    async fn v1_models_remains_public() {
        let (pool, path) = test_db();
        let cache = crate::cache::RouteCache::new(pool.clone(), "test-key".to_string());
        let stats_writer = crate::cache::StatsWriter::start(pool.clone(), crate::events::EventHub::new());
        let proxy_state = Arc::new(crate::proxy::handler::ProxyState {
            db: pool.clone(),
            config: Arc::new(parking_lot::RwLock::new(crate::config::AppConfig::default())),
            selector: Arc::new(crate::lb::BackendSelector::new()),
            http_client: reqwest::Client::new(),
            cache,
            stats_writer,
        });
        let app = aw_test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(LoginLimiter::new()))
                .app_data(web::Data::new(proxy_state))
                .configure(crate::api::configure)
                .route(
                    "/v1/models",
                    web::get().to(crate::proxy::handler::openai_list_models),
                ),
        )
        .await;

        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::get().uri("/v1/models").to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        drop(app);
        drop(pool);
        cleanup_db(&path);
    }
}
