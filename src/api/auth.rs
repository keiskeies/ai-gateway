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
/// 「同一来源 + 同一用户名」连续失败多少次上锁，以及锁多久。
const MAX_LOGIN_FAILURES: u32 = 5;
const LOGIN_LOCK_SECS: u64 = 300;
/// 全站令牌桶的容量与每秒回填量。
///
/// 这一层的用途不是挡爆破（局域网内爆破者本来就在你信任边界里），而是**挡 CPU**：
/// 一次登录尝试 = 一次 19 MiB 内存的 Argon2id，绑 `0.0.0.0` 时任何同网段设备
/// 都能拿它当免费矿机。20 突发 + 每秒 2 次对真人毫无影响（每人每 7 天才登一次），
/// 但把"一键打满 CPU"变成"最多占用 10 秒"。
const GLOBAL_BURST: f64 = 20.0;
const GLOBAL_REFILL_PER_SEC: f64 = 2.0;

/// 登录失败节流，两层。
///
/// **刻意没有"纯按 IP 上锁"这一层。** 网关默认绑 `0.0.0.0` 给局域网用，README 还教人
/// 挂反向代理——那时所有用户的 `peer_addr` 都是 127.0.0.1。纯 IP 锁意味着任何一台
/// 设备连错 5 次密码，就能把全家/全组人锁在控制台外面。所以：
///
/// 1. **来源 IP + 用户名**：5 次失败锁 5 分钟。用户名转小写入键，
///    免得用 `Admin` / `ADMIN` 轮换大小写重置自己的计数。锁只落在"这个来源试这个账号"
///    上，别人不受影响。
/// 2. **全站令牌桶**：见上面常量注释。
///
/// 状态只在内存里，重启即清零——没有落库，每次失败写一行 SQLite 不值。
pub struct LoginLimiter {
    attempts: Mutex<LoginAttempts>,
}

struct LoginAttempts {
    /// 「ip|小写用户名」→ 失败计数
    entries: HashMap<String, LoginAttempt>,
    /// 全站令牌桶
    tokens: f64,
    last_refill: Instant,
}

struct LoginAttempt {
    failures: u32,
    locked_until: Option<Instant>,
    last_failure: Instant,
}

impl LoginLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(LoginAttempts {
                entries: HashMap::new(),
                tokens: GLOBAL_BURST,
                last_refill: Instant::now(),
            }),
        }
    }

    /// 进入登录处理前调用；返回 `Err(秒数)` 表示还要等多久才允许再试。
    ///
    /// 令牌桶在这里就扣，所以被用户名锁挡下的请求**也**消耗配额——
    /// 有意的：正在被锁的来源不该再免费重试别的用户名。
    fn check(&self, ip: &str, username: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut state = self.attempts.lock();
        refill(&mut state, now);

        let mut wait = 0u64;
        if let Some(attempt) = state.entries.get(&pair_key(ip, username)) {
            if let Some(until) = attempt.locked_until {
                if until > now {
                    wait = until.duration_since(now).as_secs().max(1);
                }
            }
        }
        if wait == 0 && state.tokens < 1.0 {
            let secs = ((1.0 - state.tokens) / GLOBAL_REFILL_PER_SEC).ceil() as u64;
            wait = secs.max(1);
        }
        if wait > 0 {
            return Err(wait);
        }

        state.tokens -= 1.0;
        Ok(())
    }

    fn record_failure(&self, ip: &str, username: &str) {
        let now = Instant::now();
        let mut state = self.attempts.lock();
        refill(&mut state, now);
        // 防着有人拿海量用户名把这张表撑爆：清掉一小时没动静的键
        if state.entries.len() > 1024 {
            state
                .entries
                .retain(|_, a| now.duration_since(a.last_failure).as_secs() < 3600);
        }
        let attempt = state
            .entries
            .entry(pair_key(ip, username))
            .or_insert_with(|| LoginAttempt {
                failures: 0,
                locked_until: None,
                last_failure: now,
            });
        // 锁已到期：重新起算，而不是把旧计数接着往上加
        if attempt.locked_until.map(|until| until <= now).unwrap_or(false) {
            attempt.locked_until = None;
            attempt.failures = 0;
        }
        attempt.failures += 1;
        attempt.last_failure = now;
        if attempt.failures >= MAX_LOGIN_FAILURES {
            attempt.locked_until = Some(now + StdDuration::from_secs(LOGIN_LOCK_SECS));
        }
    }

    /// 登录成功后清掉这个来源在这个用户名上的计数。
    fn reset(&self, ip: &str, username: &str) {
        self.attempts.lock().entries.remove(&pair_key(ip, username));
    }
}

fn pair_key(ip: &str, username: &str) -> String {
    format!("{}|{}", ip, username.trim().to_ascii_lowercase())
}

fn refill(state: &mut LoginAttempts, now: Instant) {
    let elapsed = now.duration_since(state.last_refill).as_secs_f64();
    if elapsed > 0.0 {
        state.tokens = (state.tokens + elapsed * GLOBAL_REFILL_PER_SEC).min(GLOBAL_BURST);
        state.last_refill = now;
    }
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn client_ip(req: &HttpRequest) -> String {
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

/// 创建会话：同步的 purge/INSERT 放进 blocking 线程池，绝不占 actix worker。
async fn create_session(pool: &DbPool, user_id: &str) -> AppResult<String> {
    let mut raw = [0u8; 32];
    let mut rng = OsRng;
    rng.fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let now = Utc::now();
    let session = Session {
        token_hash: hash_token(&token),
        user_id: user_id.to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::days(SESSION_TTL_DAYS)).to_rfc3339(),
    };
    let purge_before = now.to_rfc3339();
    let db = pool.clone();
    web::block(move || -> AppResult<()> {
        crate::db::auth::purge_expired_sessions(&db, &purge_before)?;
        crate::db::auth::create_session(&db, &session)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(token)
}

/// 会话查询走 blocking 线程池：中间件里曾经同步跑 rusqlite，
/// 会把 actix worker 钉在 `pool.get()` / JOIN 上，N 并发时全站 /api 一起变慢。
async fn current_user(req: &HttpRequest, pool: &DbPool) -> AppResult<Option<AuthUser>> {
    let Some(token) = session_token(req) else {
        return Ok(None);
    };
    let token_hash = hash_token(&token);
    let db = pool.clone();
    web::block(move || -> AppResult<Option<AuthUser>> {
        Ok(crate::db::auth::find_session(&db, &token_hash)?.map(|(_, user)| user))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
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

    match current_user(req.request(), &pool).await {
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
    let user = current_user(&req, &pool).await?;
    Ok(no_store(HttpResponse::Ok().json(AuthStatusResponse {
        initialized,
        authenticated: user.is_some(),
        username: user.map(|u| u.username),
        admin_port: config.read().server.admin_port,
        local_console: crate::api::local::is_local_console(&req),
        checkin_login: crate::checkin::login_supported(),
        desktop_update: crate::update::supported(),
    })))
}

pub async fn setup(
    req: HttpRequest,
    db: web::Data<DbPool>,
    body: web::Json<SetupRequest>,
) -> AppResult<HttpResponse> {
    // 首次初始化只允许本机操作，避免服务暴露到局域网时被他人抢先注册管理员。
    //
    // 用 is_local_console 而不是裸 peer_addr：只看 peer 的话，挂反向代理（或有人拿
    // `ssh -R`/socat 把端口转回来）时 peer 恒为 127.0.0.1，这条守卫就形同虚设。
    // 反代场景要初始化请直接在网关那台机器上做，或按 local.rs 的注释让代理写转发头。
    if !crate::api::local::is_local_console(&req) {
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

    let token = create_session(&pool, &user.id).await?;
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
    let ip = client_ip(&req);
    // 用户名先 trim 再进键：与下面 find_user_by_username 用的是同一个 trim 后的值，
    // 否则「admin 」「admin\t」在库里是同一个账号、在限流器里却是四个计数。
    let tried_username = body.username.trim().to_string();
    if let Err(retry_after) = limiter.check(&ip, &tried_username) {
        return Err(AppError::TooManyRequests(format!(
            "Too many failed login attempts, try again in {} seconds",
            retry_after
        )));
    }

    let username = tried_username;
    let password = body.password.clone();
    if username.is_empty() || password.is_empty() {
        limiter.record_failure(&ip, &username);
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let pool = db.get_ref().clone();
    let lookup_pool = pool.clone();
    // username 会被下面的闭包 move 走，限流器要用的那份先留下
    let limiter_user = username.clone();
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
        limiter.record_failure(&ip, &limiter_user);
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    };

    limiter.reset(&ip, &limiter_user);
    let token = create_session(&pool, &user.id).await?;
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

    let token = create_session(&pool, &auth_user.id).await?;
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

    /// 键必须大小写无关：库里 `admin` 只有一个账号，若键按原样大小写分桶，
    /// 爆破者轮换 `Admin` / `ADMIN` 就永远撞不到 5 次阈值。
    #[test]
    fn limiter_lock_key_is_case_insensitive_and_trimmed() {
        assert_eq!(pair_key("::1", "Admin"), pair_key("::1", "  ADMIN "));
        assert_ne!(pair_key("::1", "admin"), pair_key("::1", "root"));
    }

    /// 锁只落在「这个来源试这个账号」上。局域网是这项目的一等使用场景：
    /// 手机输错 5 次密码，不能把电脑上正想登录的人也挡在外面。
    #[test]
    fn lock_is_scoped_to_ip_and_username() {
        let limiter = LoginLimiter::new();
        let phone = "192.168.1.23";
        let laptop = "192.168.1.24";

        for _ in 0..MAX_LOGIN_FAILURES {
            limiter.record_failure(phone, "admin");
        }
        assert!(limiter.check(phone, "admin").is_err(), "同来源同账号应已上锁");
        // 同来源换账号：没锁（全站令牌桶另有约束，20 突发下不该被扣到）
        assert!(limiter.check(phone, "other").is_ok());
        // 同账号换来源：没锁
        assert!(limiter.check(laptop, "admin").is_ok());
        // 大小写不同也算同一个账号
        assert!(limiter.check(phone, "ADMIN").is_err());

        limiter.reset(phone, "admin");
        assert!(limiter.check(phone, "admin").is_ok());
    }

    /// 全站令牌桶：挡的是"一键打满 CPU"（每次登录 = 19 MiB 的 Argon2id），
    /// 不是挡爆破。所以容量必须大到正常家用/办公绝不触发——20 次突发就是为此。
    #[test]
    fn global_bucket_absorbs_real_usage_then_throttles() {
        let limiter = LoginLimiter::new();
        // 突发容量内绝不拦截，即使全来自同一个来源
        for i in 0..GLOBAL_BURST as u32 {
            assert!(
                limiter.check("10.0.0.9", &format!("user{i}")).is_ok(),
                "第 {i} 次不应被令牌桶挡住"
            );
        }
        // 容量见底后，下一次才拒绝，且提示的等待秒数与回填速度一致（2 次/秒 → 1 秒）
        let wait = limiter.check("10.0.0.9", "overflow").unwrap_err();
        assert_eq!(wait, 1, "应提示 1 秒左右回填一次");
    }

    /// 锁到期后计数重新起算，而不是接着往上加（否则第 6 次失败就永久锁死）。
    #[test]
    fn expired_lock_resets_the_counter() {
        let limiter = LoginLimiter::new();
        for _ in 0..MAX_LOGIN_FAILURES {
            limiter.record_failure("10.0.0.7", "admin");
        }
        assert!(limiter.check("10.0.0.7", "admin").is_err());
        // 手动把锁推到过去，模拟 5 分钟已过
        {
            let mut state = limiter.attempts.lock();
            let attempt = state
                .entries
                .get_mut(&pair_key("10.0.0.7", "admin"))
                .expect("锁记录应存在");
            attempt.locked_until = Some(Instant::now() - StdDuration::from_secs(1));
        }
        assert!(limiter.check("10.0.0.7", "admin").is_ok(), "锁到期后应放行");
        // 到期后再错一次，只算 1 次，不该立刻又锁上
        limiter.record_failure("10.0.0.7", "admin");
        assert!(limiter.check("10.0.0.7", "admin").is_ok());
    }

    /// 反向代理 / 端口转发场景：peer 是 127.0.0.1，但 Host 或转发头暴露了外部来源，
    /// 初始化必须拒掉。只看 peer_addr 的老写法在这里会放行。
    #[actix_web::test]
    async fn setup_rejects_proxied_request_that_looks_loopback() {
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

        for (label, req) in [
            (
                "Host 是外部域名",
                aw_test::TestRequest::post()
                    .uri("/api/auth/setup")
                    .peer_addr("127.0.0.1:54321".parse().unwrap())
                    .insert_header(("Host", "gw.example.com"))
                    .set_json(serde_json::json!({"username": "attacker", "password": "secret123"}))
                    .to_request(),
            ),
            (
                "带了 X-Forwarded-For",
                aw_test::TestRequest::post()
                    .uri("/api/auth/setup")
                    .peer_addr("127.0.0.1:54321".parse().unwrap())
                    .insert_header(("X-Forwarded-For", "203.0.113.9"))
                    .set_json(serde_json::json!({"username": "attacker", "password": "secret123"}))
                    .to_request(),
            ),
        ] {
            let status = call_status!(&app, req);
            assert_eq!(status, 401, "{label} 的请求不该能初始化管理员");
        }

        // 库里确实没被建出账号
        let count = crate::db::auth::user_count(&pool).unwrap();
        assert_eq!(count, 0);

        drop(app);
        drop(pool);
        cleanup_db(&path);
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
        // 带 Host 头：is_local_console 会看 Host（真浏览器一定带），缺它等于"Host 不是回环"
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::post()
                .uri("/api/auth/setup")
                .peer_addr("127.0.0.1:54321".parse().unwrap())
                .insert_header(("Host", "127.0.0.1:1994"))
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
                .insert_header(("Host", "127.0.0.1:1994"))
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
            quota_usage_cache: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
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
