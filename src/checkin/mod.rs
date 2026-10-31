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

use chrono::Timelike;
use serde::Serialize;

use crate::api::settings::SharedAppConfig;
use crate::db::{checkin as db, checkin_task as db_task, DbPool};
use crate::error::{AppError, AppResult};

pub const MODELSCOPE: &str = "modelscope";

/// attempt 的有效期：用户从登录窗口切回表单、填完平台信息再保存的时间裕量。
const ATTEMPT_TTL: Duration = Duration::from_secs(15 * 60);

/// 任务内/任务间的重试间隔：魔搭的每日奖励可能晚于零点发放，
/// 首轮没到账时 3 小时后再看一次。
const RETRY_AFTER: Duration = Duration::from_secs(3 * 60 * 60);

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

/// 一条定时领取任务。账号用平台行 id 引用，界面自己拿平台列表翻译成名字。
#[derive(Debug, Serialize, Clone)]
pub struct TaskView {
    pub id: String,
    pub name: String,
    /// `'HH:MM'` 本地时间
    pub time: String,
    pub platform_ids: Vec<String>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub last_run_day: Option<String>,
    pub last_message: String,
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

// ── 定时任务 ────────────────────────────────────────────────

/// `'HH:MM'` → 当天第几分钟；格式不合法返回 None（脏数据宁可跳过也不猜）
fn minutes_of(raw: &str) -> Option<u32> {
    let raw = raw.trim();
    let (hour, minute) = raw.split_once(':')?;
    if hour.is_empty() || hour.len() > 2 || minute.len() != 2 {
        return None;
    }
    let hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

fn normalize_time(raw: &str) -> AppResult<String> {
    let minutes = minutes_of(raw).ok_or_else(|| {
        AppError::BadRequest("时间格式应为 HH:MM，例如 00:05".to_string())
    })?;
    Ok(format!("{:02}:{:02}", minutes / 60, minutes % 60))
}

/// 任务里的账号必须是「支持登录领取的平台行」。按 base_url 前缀识别，不按名字。
fn validate_targets(pool: &DbPool, ids: &[String]) -> AppResult<Vec<String>> {
    if ids.is_empty() {
        return Err(AppError::BadRequest("至少选择一个账号".to_string()));
    }
    let mut kept: Vec<String> = Vec::new();
    for id in ids {
        if kept.contains(id) {
            continue;
        }
        let platform = crate::db::platform::get(pool, id)
            .map_err(|_| AppError::BadRequest(format!("账号不存在：{id}")))?;
        if platform_key_for(&platform.name, &platform.base_url).is_none() {
            return Err(AppError::BadRequest(format!(
                "「{}」不支持登录领取",
                platform.name
            )));
        }
        kept.push(id.clone());
    }
    Ok(kept)
}

pub fn tasks(pool: &DbPool) -> AppResult<Vec<TaskView>> {
    Ok(db_task::list(pool)?.into_iter().map(TaskView::from_row).collect())
}

/// 「立即运行」要先拿到任务行（含账号清单）才能跑；跑完再回读视图给前端。
pub fn task_row(pool: &DbPool, id: &str) -> AppResult<db_task::CheckinTaskRow> {
    db_task::get(pool, id)?.ok_or_else(|| AppError::NotFound("定时任务不存在".to_string()))
}

fn task_by_id(pool: &DbPool, id: &str) -> AppResult<TaskView> {
    db_task::get(pool, id)?
        .map(TaskView::from_row)
        .ok_or_else(|| AppError::NotFound("定时任务不存在".to_string()))
}

pub fn task_view(pool: &DbPool, id: &str) -> AppResult<TaskView> {
    task_by_id(pool, id)
}

pub fn create_task(
    pool: &DbPool,
    name: &str,
    time: &str,
    platform_ids: &[String],
    enabled: Option<bool>,
) -> AppResult<TaskView> {
    let time = normalize_time(time)?;
    let ids = validate_targets(pool, platform_ids)?;
    let id = db_task::create(pool, name.trim(), &time, &ids, enabled.unwrap_or(true))?;
    task_by_id(pool, &id)
}

pub fn update_task(
    pool: &DbPool,
    id: &str,
    name: &str,
    time: &str,
    platform_ids: &[String],
    enabled: Option<bool>,
) -> AppResult<TaskView> {
    let time = normalize_time(time)?;
    let ids = validate_targets(pool, platform_ids)?;
    // 没带 enabled 就维持现状：改点位不该顺手把停用的任务打开
    let keep = task_row(pool, id)?.enabled;
    if !db_task::update(pool, id, name.trim(), &time, &ids, enabled.unwrap_or(keep))? {
        return Err(AppError::NotFound("定时任务不存在".to_string()));
    }
    task_by_id(pool, id)
}

pub fn delete_task(pool: &DbPool, id: &str) -> AppResult<()> {
    if !db_task::remove(pool, id)? {
        return Err(AppError::NotFound("定时任务不存在".to_string()));
    }
    Ok(())
}

/// 跑一条任务。`force`（界面上的「立即运行」）绕过账号级的「今天已领/不足 3 小时」过滤。
/// 逐个账号串行领取：同一站点同时来多个账号的请求没有好处。
pub async fn run_task(
    pool: &DbPool,
    encrypt_key: &str,
    task: &db_task::CheckinTaskRow,
    force: bool,
) -> AppResult<String> {
    let accounts = db::list(pool)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now = chrono::Utc::now();
    let run_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

    let mut credited = 0;
    let mut waiting = 0;
    let mut relogin = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut offline = 0;

    for id in &task.platform_ids {
        let Some(account) = accounts.iter().find(|row| &row.platform_id == id) else {
            // 任务建好后退出登录或删了平台：这一行没有会话可领
            offline += 1;
            continue;
        };
        if !force && !needs_run(account, &today, now) {
            if account.status == "expired" {
                relogin += 1;
            } else {
                skipped += 1;
            }
            continue;
        }
        match run_once(pool, encrypt_key, id).await {
            Ok(view) => match view.status.as_str() {
                "ok" => credited += 1,
                "waiting" => waiting += 1,
                "expired" => relogin += 1,
                _ => failed += 1,
            },
            Err(error) => {
                tracing::warn!("定时领取（{id}）失败: {error}");
                failed += 1;
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if credited > 0 {
        parts.push(format!("{credited} 已到账"));
    }
    if waiting > 0 {
        parts.push(format!("{waiting} 待发放"));
    }
    if relogin > 0 {
        parts.push(format!("{relogin} 需重新登录"));
    }
    if failed > 0 {
        parts.push(format!("{failed} 领取失败"));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} 今日已领"));
    }
    if offline > 0 {
        parts.push(format!("{offline} 未登录"));
    }
    let message = if parts.is_empty() {
        "已全部到账".to_string()
    } else {
        parts.join(" · ")
    };
    db_task::update_run(pool, &task.id, &run_at, &today, &message)?;
    Ok(message)
}

/// 成功读到余额后，用它校准平台的积分台账（`spent = budget - 上游可用余额`）。
///
/// 为什么不是直接把余额当「剩余」显示：余额只在领取时读一次，请求期间不会变，
/// 界面会看起来「扣了分却不减」。校准之后积分列继续随每笔请求扣减，领取时再纠偏。
fn apply_pulse(
    pool: &DbPool,
    row: &db::CheckinAccountRow,
    pulse: &modelscope::Pulse,
) -> AppResult<()> {
    let Ok(platform) = crate::db::platform::get(pool, &row.platform_id) else {
        return Ok(());
    };
    let (Some(budget), Some(window)) = (platform.credit_budget, platform.credit_window.as_deref())
    else {
        return Ok(());
    };
    match crate::db::credit::calibrate(pool, &row.platform_id, budget, window, pulse.available) {
        Ok((old, new, delta)) if delta != 0.0 => tracing::info!(
            "积分校准（{}）：已用 {} → {}（上游余额 {}，预算 {}）",
            row.platform_id,
            old,
            new,
            pulse.available,
            budget
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!("积分校准失败（{}）: {}", row.platform_id, error),
    }
    Ok(())
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
            apply_pulse(pool, row, &pulse)?;
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
                Some(pulse.available),
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

/// 后台线程：每 60 秒看一次任务表。
///
/// 为什么用「本地时分 ≥ 点位」而不是精确唤醒：应用可能在整个点位上没开（或机器在睡觉），
/// 当天首次 tick 就把错过的任务补跑一次，比错过一整天好。跑过之后每 3 小时跟进仍未到账的
/// 账号（发放可能晚于零点），全部到账后账号级 `needs_run` 会挡住真实请求。
pub fn spawn_checkin_runner(pool: DbPool, config: SharedAppConfig) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::error!("定时领取线程无法创建 tokio runtime: {}", error);
                return;
            }
        };
        loop {
            runtime.block_on(tick(&pool, &config));
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

async fn tick(pool: &DbPool, config: &SharedAppConfig) {
    let encrypt_key = config.read().security.encrypt_key.clone();
    let tasks = match db_task::list(pool) {
        Ok(tasks) => tasks,
        Err(error) => {
            tracing::warn!("定时领取读取任务失败: {}", error);
            return;
        }
    };
    let now_local = chrono::Local::now();
    let today = now_local.format("%Y-%m-%d").to_string();
    for task in tasks {
        if !task_due(&task, &today, now_local) {
            continue;
        }
        match run_task(pool, &encrypt_key, &task, false).await {
            Ok(message) => tracing::info!(
                "定时领取（{} · {}）：{}",
                task.time,
                if task.name.is_empty() { "任务" } else { &task.name },
                message
            ),
            Err(error) => tracing::warn!("定时领取任务（{}）失败: {}", task.id, error),
        }
    }
}

/// 到点没有：启用 + 本地时分已过点位 + 今天还没跑过（或跑过但已满 3 小时的跟进间隔）
fn task_due(
    task: &db_task::CheckinTaskRow,
    today: &str,
    now_local: chrono::DateTime<chrono::Local>,
) -> bool {
    if !task.enabled {
        return false;
    }
    let Some(target) = minutes_of(&task.time) else {
        return false;
    };
    if now_local.hour() * 60 + now_local.minute() < target {
        return false;
    }
    if task.last_run_day.as_deref() != Some(today) {
        return true;
    }
    let Some(last) = task.last_run_at.as_deref() else {
        return true;
    };
    let now = now_local.with_timezone(&chrono::Utc);
    match chrono::DateTime::parse_from_rfc3339(last) {
        Ok(at) => {
            now.signed_duration_since(at.with_timezone(&chrono::Utc))
                >= chrono::Duration::seconds(RETRY_AFTER.as_secs() as i64)
        }
        // 时间读不懂就当作没跑过，让这一轮重新记账
        Err(_) => true,
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
            now.signed_duration_since(at.with_timezone(&chrono::Utc))
                >= chrono::Duration::seconds(RETRY_AFTER.as_secs() as i64)
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

impl TaskView {
    fn from_row(row: db_task::CheckinTaskRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            time: row.time,
            platform_ids: row.platform_ids,
            enabled: row.enabled,
            last_run_at: row.last_run_at,
            last_run_day: row.last_run_day,
            last_message: row.last_message,
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

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("checkin-test-{}.db", uuid::Uuid::new_v4()));
        (crate::db::init_pool(&path).expect("init test db"), path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn successful_pulse_calibrates_the_credit_ledger() {
        let (pool, path) = test_db();
        let platform = crate::db::platform::create(
            &pool,
            &crate::models::platform::CreatePlatformRequest {
                name: "ModelScope 魔搭".into(),
                platform_type: crate::models::platform::PlatformType::OpenAI,
                base_url: "https://api-inference.modelscope.cn/v1".into(),
                api_key: "sk-test".into(),
                organization: None,
                custom_headers: serde_json::Value::Null,
                credit_budget: Some(250.0),
                credit_window: Some("day".into()),
                credit_unit: Some("credits".into()),
                adapter: None,
                login_attempt: None,
            },
        )
        .expect("create platform");
        crate::db::checkin::upsert_session(
            &pool,
            &platform.id,
            MODELSCOPE,
            "demo",
            "ModelScope 魔搭 · demo",
            "enc",
            "m_session_id",
        )
        .expect("seed session");
        let row = crate::db::checkin::get_by_platform(&pool, &platform.id)
            .expect("read session")
            .expect("session exists");

        apply_pulse(
            &pool,
            &row,
            &modelscope::Pulse {
                balance: 400.0,
                available: 241.0,
                daily_done: true,
                daily_amount: Some(100.0),
                daily_total: 1,
                daily_completed: 1,
            },
        )
        .expect("apply pulse");

        let usage = crate::db::credit::usage_all(&pool)
            .expect("usage")
            .into_iter()
            .find(|item| item.platform_id == platform.id)
            .expect("platform in usage");
        assert_eq!(usage.spent, 9.0, "spent = 预算 250 - 上游可用余额 241");
        assert_eq!(usage.remaining, 241.0);
        cleanup_db(&path);
    }

    #[test]
    fn pulse_balance_above_budget_clamps_spent_to_zero() {
        let (pool, path) = test_db();
        let platform = crate::db::platform::create(
            &pool,
            &crate::models::platform::CreatePlatformRequest {
                name: "ModelScope 魔搭".into(),
                platform_type: crate::models::platform::PlatformType::OpenAI,
                base_url: "https://api-inference.modelscope.cn/v1".into(),
                api_key: "sk-test".into(),
                organization: None,
                custom_headers: serde_json::Value::Null,
                credit_budget: Some(250.0),
                credit_window: Some("day".into()),
                credit_unit: Some("credits".into()),
                adapter: None,
                login_attempt: None,
            },
        )
        .expect("create platform");
        crate::db::checkin::upsert_session(&pool, &platform.id, MODELSCOPE, "demo", "demo", "enc", "m_session_id")
            .expect("seed session");
        let row = crate::db::checkin::get_by_platform(&pool, &platform.id)
            .expect("read session")
            .expect("session exists");
        apply_pulse(
            &pool,
            &row,
            &modelscope::Pulse {
                balance: 1200.0,
                available: 1000.0,
                daily_done: true,
                daily_amount: Some(250.0),
                daily_total: 1,
                daily_completed: 1,
            },
        )
        .expect("apply pulse");
        let usage = crate::db::credit::usage_all(&pool)
            .expect("usage")
            .into_iter()
            .find(|item| item.platform_id == platform.id)
            .expect("platform in usage");
        assert_eq!(usage.spent, 0.0, "余额超过预算时不能出现负已用");
        assert_eq!(usage.remaining, 250.0);
        cleanup_db(&path);
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

    fn task(
        time: &str,
        enabled: bool,
        last_run_day: Option<&str>,
        last_run_at: Option<&str>,
    ) -> db_task::CheckinTaskRow {
        db_task::CheckinTaskRow {
            id: "task".into(),
            name: String::new(),
            time: time.into(),
            platform_ids: vec!["p1".into()],
            enabled,
            last_run_at: last_run_at.map(str::to_string),
            last_run_day: last_run_day.map(str::to_string),
            last_message: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// 把「今天」的本地钟点固定下来，点位判定才能测
    fn local_at(hour: u32, minute: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local::now()
            .with_hour(hour)
            .and_then(|at| at.with_minute(minute))
            .and_then(|at| at.with_second(0))
            .expect("fix the local clock")
    }

    #[test]
    fn task_time_is_validated_and_canonicalised() {
        assert_eq!(normalize_time("00:05").expect("valid"), "00:05");
        assert_eq!(normalize_time(" 7:30 ").expect("补零"), "07:30");
        assert_eq!(normalize_time("23:59").expect("valid"), "23:59");
        for bad in ["24:00", "7:60", "7:5", "abc", "0730", ""] {
            let error = normalize_time(bad).unwrap_err().to_string();
            assert!(error.contains("HH:MM"), "{bad} 要报格式错：{error}");
        }
        assert_eq!(minutes_of("23:59"), Some(24 * 60 - 1));
    }

    #[test]
    fn task_due_waits_for_the_point_and_follows_up_every_three_hours() {
        // 点位之前不跑；点位之后（哪怕错过了大半天）当天补跑一次
        assert!(!task_due(&task("00:05", true, None, None), "2026-11-01", local_at(0, 4)));
        assert!(task_due(&task("00:05", true, None, None), "2026-11-01", local_at(0, 5)));
        assert!(task_due(&task("00:05", true, None, None), "2026-11-01", local_at(23, 0)));
        assert!(!task_due(&task("00:05", false, None, None), "2026-11-01", local_at(23, 0)));
        assert!(!task_due(&task("25:00", true, None, None), "2026-11-01", local_at(23, 0)));

        // 今天跑过之后按 3 小时间隔跟进；跑过但时间读不懂则重新记账
        let ran = local_at(20, 0);
        let ago = |hours: i64| {
            (ran.with_timezone(&chrono::Utc) - chrono::Duration::hours(hours))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        };
        assert!(!task_due(&task("00:05", true, Some("2026-11-01"), Some(&ago(2))), "2026-11-01", ran));
        assert!(task_due(&task("00:05", true, Some("2026-11-01"), Some(&ago(3))), "2026-11-01", ran));
        assert!(task_due(&task("00:05", true, Some("2026-10-31"), Some(&ago(2))), "2026-11-01", ran));
        assert!(task_due(&task("00:05", true, Some("2026-11-01"), Some("nope")), "2026-11-01", ran));
    }

    fn add_platform(pool: &DbPool, name: &str, base_url: &str) -> String {
        crate::db::platform::create(
            pool,
            &crate::models::platform::CreatePlatformRequest {
                name: name.into(),
                platform_type: crate::models::platform::PlatformType::OpenAI,
                base_url: base_url.into(),
                api_key: "sk-test".into(),
                organization: None,
                custom_headers: serde_json::Value::Null,
                credit_budget: None,
                credit_window: None,
                credit_unit: None,
                adapter: None,
                login_attempt: None,
            },
        )
        .expect("create platform")
        .id
    }

    #[test]
    fn task_targets_must_be_supported_accounts() {
        let (pool, path) = test_db();
        let modelscope = add_platform(&pool, "ModelScope 魔搭", "https://api-inference.modelscope.cn/v1");
        let deepseek = add_platform(&pool, "DeepSeek", "https://api.deepseek.com/v1");

        let empty = create_task(&pool, "", "00:05", &[], Some(true)).unwrap_err().to_string();
        assert!(empty.contains("至少选择一个账号"), "{empty}");

        let unsupported = create_task(&pool, "", "00:05", &[deepseek.clone()], Some(true))
            .unwrap_err()
            .to_string();
        assert!(unsupported.contains("DeepSeek"), "要点名是哪一行：{unsupported}");

        let view = create_task(
            &pool,
            " 每日 ",
            "7:05",
            &[modelscope.clone(), modelscope.clone()],
            None,
        )
        .expect("create task");
        assert_eq!(view.time, "07:05", "点位规范化后才落库");
        assert_eq!(view.name, "每日", "名称两侧空白去掉");
        assert_eq!(view.platform_ids, vec![modelscope.clone()], "重复账号只留一份");
        assert!(view.enabled, "没表态就是启用");

        let updated = update_task(&pool, &view.id, "早上", "23:59", &[modelscope.clone()], None)
            .expect("update task");
        assert_eq!((updated.time.as_str(), updated.name.as_str()), ("23:59", "早上"));
        assert!(updated.enabled, "改点位不能顺手把开关打开或关掉");

        let paused = update_task(&pool, &view.id, "早上", "23:59", &[modelscope.clone()], Some(false))
            .expect("pause task");
        assert!(!paused.enabled);
        assert_eq!(tasks(&pool).expect("list").len(), 1);

        delete_task(&pool, &view.id).expect("delete");
        assert!(tasks(&pool).expect("list").is_empty());
        assert!(delete_task(&pool, &view.id).is_err(), "重复删除要报错而不是假装成功");
        cleanup_db(&path);
    }

    #[test]
    fn deleting_a_platform_drops_it_from_every_task() {
        let (pool, path) = test_db();
        let keep = add_platform(&pool, "ModelScope 魔搭 1", "https://api-inference.modelscope.cn/v1");
        let gone = add_platform(&pool, "ModelScope 魔搭 2", "https://api-inference.modelscope.cn/v1");
        let view =
            create_task(&pool, "", "00:05", &[keep.clone(), gone.clone()], Some(true)).expect("create");

        crate::db::platform::delete(&pool, &gone).expect("delete platform");

        let after = task_view(&pool, &view.id).expect("read task");
        assert_eq!(after.platform_ids, vec![keep], "任务不能继续带着一个已删除的账号");
        cleanup_db(&path);
    }
}
