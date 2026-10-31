//! 每日领取（签到）与「登录换 Key」。
//!
//! 平台即账号：一行 `platforms` 记录对应最多一个 `checkin_accounts` 会话，
//! 同一平台多个账号 → 同一预设建多条平台记录，各自登录、各自领取。
//!
//! 登录分两种：
//! - **已有平台**（`platform_id = Some`）：抓到 cookie 后直接绑定到这一行，替换旧会话；
//! - **新建平台**（`platform_id = None`）：把 cookie/账号名/SDK 令牌暂存在内存 attempt 里，
//!   前端回填表单并保存后，`bind_attempt` 再把会话落到新平台 id 上。
//!
//! 登录窗口本身由桌面端实现（core 不依赖 tauri），服务器模式下没有 opener，
//! `login_*` 会明确返回「仅桌面端可用」。
//!
//! 设计见 `docs/checkin-plan.md`。

pub mod modelscope;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::api::settings::SharedAppConfig;
use crate::db::{checkin as db, DbPool};
use crate::error::{AppError, AppResult};

pub const MODELSCOPE: &str = "modelscope";

/// attempt 的有效期：用户从登录窗口切回表单、填完平台信息再保存的时间裕量。
const ATTEMPT_TTL: Duration = Duration::from_secs(15 * 60);

/// 登录窗口的注入点：core 不依赖 tauri，由桌面端在启动时注册。
/// 参数：平台 key、attempt id（绑定到哪个平台由 attempt 自己记着）。
type LoginOpener = Box<dyn Fn(&'static str, String) -> Result<(), String> + Send + Sync>;

static LOGIN_OPENER: OnceLock<LoginOpener> = OnceLock::new();

/// 同一平台同一时间只允许一次推送，避免手动点击与后台线程撞车。
static RUNNING: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

struct Attempt {
    platform: &'static str,
    platform_id: Option<String>,
    status: &'static str,
    identity: Option<String>,
    label: Option<String>,
    api_key: Option<String>,
    message: Option<String>,
    /// 新平台流程暂存登录态，等平台创建后绑定
    cookies: Option<(String, String)>,
    created: Instant,
}

static ATTEMPTS: LazyLock<Mutex<HashMap<String, Attempt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize, Clone)]
pub struct AttemptView {
    pub id: String,
    pub platform: String,
    pub status: String,
    pub identity: Option<String>,
    pub label: Option<String>,
    /// 只有新建平台流程回传 Key（表单回填用）；绑定已有平台时不回传
    pub api_key: Option<String>,
    pub message: Option<String>,
}

/// 前端在平台页据此判断「这一行能不能登录/领取」：按 base_url 前缀匹配，
/// 不靠平台名——用户改名后依然识别。
#[derive(Serialize, Clone)]
pub struct Capability {
    pub key: &'static str,
    pub brand: &'static str,
    pub base_url_prefix: &'static str,
    pub login_supported: bool,
}

pub fn capabilities() -> Vec<Capability> {
    let supported = login_supported();
    vec![Capability {
        key: MODELSCOPE,
        brand: "ModelScope",
        base_url_prefix: "https://api-inference.modelscope.cn",
        login_supported: supported,
    }]
}

#[derive(Serialize, Clone)]
pub struct AccountView {
    pub platform_id: String,
    pub platform: String,
    pub identity: String,
    pub label: String,
    pub status: String,
    pub message: String,
    pub last_run_at: Option<String>,
    pub last_run_day: Option<String>,
    pub last_ok_day: Option<String>,
    pub balance: Option<f64>,
    pub daily_amount: Option<f64>,
    pub daily_done: bool,
    pub updated_at: String,
}

/// 这个平台记录能不能做「登录/领取」。依据 Key 申请注册表里的品牌判断，
/// 不靠平台名硬编码——用户改名后仍然能识别。
pub fn platform_key_for(name: &str, base_url: &str) -> Option<&'static str> {
    let link = crate::models::key_link::KeyLink::find_for(Some(name), Some(base_url))?;
    match link.brand.as_str() {
        "ModelScope" => Some(MODELSCOPE),
        _ => None,
    }
}

fn platform_static(platform: &str) -> AppResult<&'static str> {
    match platform {
        MODELSCOPE => Ok(MODELSCOPE),
        other => Err(AppError::BadRequest(format!("未知平台: {other}"))),
    }
}

pub fn register_login_opener(opener: LoginOpener) {
    let _ = LOGIN_OPENER.set(opener);
}

pub fn login_supported() -> bool {
    LOGIN_OPENER.get().is_some()
}

fn prune_attempts() {
    let mut attempts = ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    attempts.retain(|_, attempt| attempt.created.elapsed() < ATTEMPT_TTL);
}

fn begin_attempt(platform: &'static str, platform_id: Option<String>) -> AppResult<String> {
    let opener = LOGIN_OPENER
        .get()
        .ok_or_else(|| AppError::Conflict("登录窗口仅在桌面应用中可用".to_string()))?;
    prune_attempts();
    let id = uuid::Uuid::new_v4().to_string();
    ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner()).insert(
        id.clone(),
        Attempt {
            platform,
            platform_id: platform_id.clone(),
            status: "pending",
            identity: None,
            label: None,
            api_key: None,
            message: None,
            cookies: None,
            created: Instant::now(),
        },
    );
    if let Err(error) = opener(platform, id.clone()) {
        ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        return Err(AppError::Internal(error));
    }
    Ok(id)
}

/// 新建平台流程：只认平台 key，不绑定任何平台行。
pub fn login_new(platform: &str) -> AppResult<String> {
    let key = platform_static(platform)?;
    begin_attempt(key, None)
}

/// 已有平台重新登录 / 首次登录。
pub fn login_platform(pool: &DbPool, platform_id: &str) -> AppResult<String> {
    let platform = crate::db::platform::get(pool, platform_id)
        .map_err(|_| AppError::NotFound("平台不存在".to_string()))?;
    let key = platform_key_for(&platform.name, &platform.base_url)
        .ok_or_else(|| AppError::BadRequest("这个平台暂不支持登录领取".to_string()))?;
    begin_attempt(key, Some(platform_id.to_string()))
}

pub fn attempt_view(id: &str) -> AppResult<AttemptView> {
    prune_attempts();
    let attempts = ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    let attempt = attempts
        .get(id)
        .ok_or_else(|| AppError::NotFound("登录结果已过期，请重试".to_string()))?;
    Ok(AttemptView {
        id: id.to_string(),
        platform: attempt.platform.to_string(),
        status: attempt.status.to_string(),
        identity: attempt.identity.clone(),
        label: attempt.label.clone(),
        api_key: if attempt.platform_id.is_none() {
            attempt.api_key.clone()
        } else {
            None
        },
        message: attempt.message.clone(),
    })
}

fn finish_attempt(
    id: &str,
    status: &'static str,
    identity: Option<String>,
    label: Option<String>,
    api_key: Option<String>,
    message: Option<String>,
    cookies: Option<(String, String)>,
) {
    let mut attempts = ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(attempt) = attempts.get_mut(id) {
        attempt.status = status;
        attempt.identity = identity;
        attempt.label = label;
        attempt.api_key = api_key;
        attempt.message = message;
        attempt.cookies = cookies;
    }
}

fn attempt_brief(id: &str) -> Option<(&'static str, Option<String>)> {
    let attempts = ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    attempts
        .get(id)
        .map(|attempt| (attempt.platform, attempt.platform_id.clone()))
}

/// 桌面端抓到 cookie 后调用：探测账号 → 已有平台直接绑定，新平台暂存到 attempt。
/// 所有失败都写进 attempt 的 `message`，由前端展示，不向桌面端抛错。
pub async fn complete_attempt(
    pool: &DbPool,
    encrypt_key: &str,
    attempt_id: &str,
    cookie_header: &str,
    cookie_names: &str,
) {
    let Some((platform, platform_id)) = attempt_brief(attempt_id) else {
        tracing::warn!("登录 attempt 已过期: {}", attempt_id);
        return;
    };

    match modelscope::probe_account(cookie_header).await {
        Ok(info) => {
            let label = info.label();
            match platform_id {
                Some(platform_id) => {
                    // 同一个账号不允许绑到两条平台行：否则两条会重复领同一份奖励
                    let duplicate = !info.name.is_empty()
                        && db::list(pool).unwrap_or_default().iter().any(|row| {
                            row.platform_id != platform_id
                                && row.platform == platform
                                && row.identity == info.name
                        });
                    if duplicate {
                        finish_attempt(
                            attempt_id,
                            "error",
                            None,
                            None,
                            None,
                            Some(format!("账号 {} 已经绑在另一条平台记录上了", info.name)),
                            None,
                        );
                        return;
                    }
                    let saved = crate::crypto::encrypt(cookie_header, encrypt_key).and_then(|enc| {
                        db::upsert_session(
                            pool,
                            &platform_id,
                            platform,
                            &info.name,
                            &label,
                            &enc,
                            cookie_names,
                        )
                        .map(|_| ())
                    });
                    match saved {
                        Ok(()) => finish_attempt(
                            attempt_id,
                            "ok",
                            Some(info.name),
                            Some(label),
                            None,
                            None,
                            None,
                        ),
                        Err(error) => finish_attempt(
                            attempt_id,
                            "error",
                            None,
                            None,
                            None,
                            Some(format!("保存登录态失败：{error}")),
                            None,
                        ),
                    }
                }
                None => finish_attempt(
                    attempt_id,
                    "ok",
                    Some(info.name),
                    Some(label),
                    info.api_key,
                    None,
                    Some((cookie_header.to_string(), cookie_names.to_string())),
                ),
            }
        }
        Err(modelscope::PulseError::Expired) => finish_attempt(
            attempt_id,
            "error",
            None,
            None,
            None,
            Some("登录未完成或登录态无效，请重试".to_string()),
            None,
        ),
        Err(modelscope::PulseError::Failed(error)) => finish_attempt(
            attempt_id,
            "error",
            None,
            None,
            None,
            Some(format!("读取账号信息失败：{error}")),
            None,
        ),
    }
}

/// 平台创建成功后调用：把 attempt 里暂存的会话落到新平台 id 上。
pub fn bind_attempt(
    pool: &DbPool,
    encrypt_key: &str,
    attempt_id: &str,
    platform_id: &str,
) -> AppResult<()> {
    let attempt = ATTEMPTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(attempt_id)
        .ok_or_else(|| AppError::BadRequest("登录结果已过期，请重新登录".to_string()))?;
    if attempt.status != "ok" {
        return Err(AppError::BadRequest(
            attempt.message.unwrap_or_else(|| "登录未完成".to_string()),
        ));
    }
    let (header, names) = attempt
        .cookies
        .ok_or_else(|| AppError::BadRequest("登录结果里没有会话，请重新登录".to_string()))?;
    let platform = crate::db::platform::get(pool, platform_id)?;
    let key = platform_key_for(&platform.name, &platform.base_url)
        .ok_or_else(|| AppError::BadRequest("这个平台暂不支持登录领取".to_string()))?;
    if key != attempt.platform {
        return Err(AppError::BadRequest("登录平台与所选预设不一致".to_string()));
    }
    let encrypted = crate::crypto::encrypt(&header, encrypt_key)?;
    db::upsert_session(
        pool,
        platform_id,
        key,
        attempt.identity.as_deref().unwrap_or_default(),
        attempt.label.as_deref().unwrap_or_default(),
        &encrypted,
        &names,
    )?;
    Ok(())
}

pub fn accounts(pool: &DbPool) -> AppResult<Vec<AccountView>> {
    Ok(db::list(pool)?.into_iter().map(AccountView::from_row).collect())
}

pub fn clear_session(pool: &DbPool, platform_id: &str) -> AppResult<()> {
    db::delete_by_platform(pool, platform_id)?;
    Ok(())
}

fn claim_running(platform_id: &str) -> bool {
    RUNNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(platform_id.to_string())
}

fn release_running(platform_id: &str) {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(platform_id);
}

/// 立即跑一次 pulse（手动「领取」与后台线程共用）。
pub async fn run_once(
    pool: &DbPool,
    encrypt_key: &str,
    platform_id: &str,
) -> AppResult<AccountView> {
    let row = db::get_by_platform(pool, platform_id)?
        .ok_or_else(|| AppError::NotFound("这个平台还没有登录过".to_string()))?;
    if !claim_running(platform_id) {
        return Err(AppError::Conflict("这次领取还没结束，请稍候".to_string()));
    }
    let outcome = execute(pool, encrypt_key, &row).await;
    release_running(platform_id);
    outcome?;
    db::get_by_platform(pool, platform_id)?
        .map(AccountView::from_row)
        .ok_or_else(|| AppError::NotFound("这个平台还没有登录过".to_string()))
}

/// 顶部「一键登录/领取」的第一步：把所有已登录账号依次领一遍。
/// 单个账号失败只记日志，不阻断其它账号。
pub async fn run_all(pool: &DbPool, encrypt_key: &str) -> AppResult<Vec<AccountView>> {
    let rows = db::list(pool)?;
    let mut views = Vec::new();
    for row in rows {
        match run_once(pool, encrypt_key, &row.platform_id).await {
            Ok(view) => views.push(view),
            Err(error) => tracing::warn!("领取 {} 失败: {}", row.platform_id, error),
        }
    }
    Ok(views)
}

async fn execute(pool: &DbPool, encrypt_key: &str, row: &db::CheckinAccountRow) -> AppResult<()> {
    let platform = platform_static(&row.platform)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let run_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

    // encrypt_key 改过时旧密文解不开：这是账号状态，不是接口错误，落库后让用户重新登录
    let cookie = match crate::crypto::decrypt(&row.cookies_enc, encrypt_key) {
        Ok(cookie) => cookie,
        Err(error) => {
            tracing::warn!("登录态解密失败（{}）: {}", row.platform, error);
            db::update_run(
                pool,
                &row.id,
                &db::RunUpdate {
                    status: "error",
                    message: "登录态读取失败（加密密钥可能已更换），请重新登录",
                    run_at: &run_at,
                    run_day: &today,
                    ok_day: None,
                    balance: None,
                    balance_at: None,
                    daily_amount: None,
                    daily_done: None,
                },
            )?;
            return Ok(());
        }
    };

    let outcome = match platform {
        MODELSCOPE => modelscope::pulse(&cookie).await,
        _ => unreachable!("platform_static 已经拦截"),
    };

    let (status, message, ok_day, balance, daily_amount, daily_done) = match outcome {
        Ok(pulse) => {
            let done = pulse.daily_done;
            let status = if done { "ok" } else { "waiting" };
            let message = if done {
                format!(
                    "今日已到账 {} 魔粒（每日规则 {}/{}）",
                    pulse.daily_amount.unwrap_or(0.0).round(),
                    pulse.daily_completed,
                    pulse.daily_total
                )
            } else {
                format!(
                    "登录有效，每日奖励尚未到账（规则 {}/{}）",
                    pulse.daily_completed, pulse.daily_total
                )
            };
            (
                status,
                message,
                done.then(|| today.clone()),
                Some(pulse.balance),
                pulse.daily_amount,
                Some(done),
            )
        }
        Err(modelscope::PulseError::Expired) => (
            "expired",
            "登录已失效，请重新登录".to_string(),
            None,
            None,
            None,
            None,
        ),
        Err(modelscope::PulseError::Failed(error)) => {
            ("error", format!("领取失败：{error}"), None, None, None, None)
        }
    };

    db::update_run(
        pool,
        &row.id,
        &db::RunUpdate {
            status,
            message: &message,
            run_at: &run_at,
            run_day: &today,
            ok_day: ok_day.as_deref(),
            balance,
            // 只有这次真的读到余额才刷新同步时间
            balance_at: balance.map(|_| run_at.as_str()),
            daily_amount,
            daily_done,
        },
    )
}

/// 后台线程：启动即跑一轮，之后每 30 分钟看一次。是否真的请求由 `needs_run` 决定：
/// 今天没到账的账号每 3 小时重试一次（发放时间可能晚于零点），到账后当天不再打扰。
pub fn spawn_daily_runner(pool: DbPool, config: SharedAppConfig) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::error!("每日领取线程无法创建 tokio runtime: {}", error);
                return;
            }
        };
        loop {
            runtime.block_on(tick(&pool, &config));
            std::thread::sleep(Duration::from_secs(30 * 60));
        }
    });
}

async fn tick(pool: &DbPool, config: &SharedAppConfig) {
    let encrypt_key = config.read().security.encrypt_key.clone();
    let rows = match db::list(pool) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!("每日领取读取账号失败: {}", error);
            return;
        }
    };
    let now = chrono::Utc::now();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    for row in rows {
        if !needs_run(&row, &today, now) {
            continue;
        }
        match run_once(pool, &encrypt_key, &row.platform_id).await {
            Ok(view) => tracing::info!(
                "每日领取（{}）：{} — {}",
                row.platform,
                view.status,
                view.message
            ),
            Err(error) => tracing::warn!("每日领取（{}）失败: {}", row.platform, error),
        }
    }
}

fn needs_run(row: &db::CheckinAccountRow, today: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    if row.last_ok_day.as_deref() == Some(today) {
        return false;
    }
    // 登录态失效后继续打接口没有意义，等用户重新登录
    if row.status == "expired" {
        return false;
    }
    match row
        .last_run_at
        .as_deref()
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
    {
        Some(at) => {
            now.signed_duration_since(at.with_timezone(&chrono::Utc)) >= chrono::Duration::hours(3)
        }
        None => true,
    }
}

impl AccountView {
    fn from_row(row: db::CheckinAccountRow) -> Self {
        Self {
            platform_id: row.platform_id,
            platform: row.platform,
            identity: row.identity,
            label: row.label,
            status: row.status,
            message: row.last_message,
            last_run_at: row.last_run_at,
            last_run_day: row.last_run_day,
            last_ok_day: row.last_ok_day,
            balance: row.balance,
            daily_amount: row.daily_amount,
            daily_done: row.daily_done,
            updated_at: row.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::checkin::CheckinAccountRow;

    fn row(status: &str, last_ok_day: Option<&str>, last_run_at: Option<&str>) -> CheckinAccountRow {
        CheckinAccountRow {
            id: "acc".into(),
            platform_id: "p1".into(),
            platform: MODELSCOPE.into(),
            identity: "demo".into(),
            label: "ModelScope 魔搭 · demo".into(),
            cookies_enc: "enc".into(),
            cookie_names: "m_session_id,csn".into(),
            status: status.into(),
            last_message: String::new(),
            last_run_at: last_run_at.map(str::to_string),
            last_run_day: None,
            last_ok_day: last_ok_day.map(str::to_string),
            balance: None,
            balance_at: None,
            daily_amount: None,
            daily_done: false,
            created_at: "2026-10-31T00:00:00Z".into(),
            updated_at: "2026-10-31T00:00:00Z".into(),
        }
    }

    fn at(raw: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(raw).unwrap().with_timezone(&chrono::Utc)
    }

    #[test]
    fn needs_run_rules() {
        let now = at("2026-10-31T12:00:00Z");

        assert!(!needs_run(&row("ok", Some("2026-10-31"), Some("2026-10-31T00:10:00Z")), "2026-10-31", now));
        assert!(!needs_run(&row("expired", None, Some("2026-10-30T00:10:00Z")), "2026-10-31", now));
        assert!(needs_run(&row("new", None, None), "2026-10-31", now));
        assert!(!needs_run(&row("waiting", None, Some("2026-10-31T10:30:00Z")), "2026-10-31", now));
        assert!(needs_run(&row("waiting", None, Some("2026-10-31T08:00:00Z")), "2026-10-31", now));
        assert!(needs_run(&row("error", None, Some("not-a-time")), "2026-10-31", now));
    }

    #[test]
    fn platform_key_follows_key_link_registry() {
        assert_eq!(
            platform_key_for("ModelScope 魔搭", "https://api-inference.modelscope.cn/v1"),
            Some(MODELSCOPE)
        );
        assert_eq!(
            platform_key_for("我的魔搭", "https://api-inference.modelscope.cn/v1"),
            Some(MODELSCOPE)
        );
        assert_eq!(platform_key_for("DeepSeek", "https://api.deepseek.com/v1"), None);
    }
}
