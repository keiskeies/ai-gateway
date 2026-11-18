//! 桌面端「检查更新」。
//!
//! 只有桌面壳能用：安装包要落到本机磁盘，独立服务器模式没有注册 reveal opener，
//! 两个写入口都返回 409，前端也据此不渲染任何入口（`desktop_update`）。
//!
//! 版本号只有一个来源：`env!("CARGO_PKG_VERSION")`，和关于面板显示的
//! `__APP_VERSION__` 同为 Cargo.toml，不会出现两个页面各说一个版本。

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};

/// 发布仓库：安装包由 `.github/workflows/release.yml` 在推 `v*` tag 时产出。
const REPO: &str = "keiskeies/ai-gateway";
const LATEST_URL: &str = "https://api.github.com/repos/keiskeies/ai-gateway/releases/latest";

/// GitHub 未登录限流是 60 次/小时/IP，所以查到的结果留一会儿：
/// 「检查 → 点下载」这条链只打一次接口，不至于点了下载才被限流打断。
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);

/// 在文件管理器里定位安装包的动作由桌面端注册（core 不依赖 tauri，也不 spawn 进程）。
/// 只负责「露出来」，不代用户运行安装包——装新版本要替换正在运行的程序本身，
/// 这一步必须留在人手里。
type Revealer = Box<dyn Fn(String) -> Result<(), String> + Send + Sync>;

static REVEALER: OnceLock<Revealer> = OnceLock::new();

/// 桌面壳在启动时注册：把下载好的安装包在文件管理器里选中。
pub fn register_reveal_opener(revealer: Revealer) {
    let _ = REVEALER.set(revealer);
}

/// 本进程能不能下载并定位安装包（只有 Tauri 桌面端注册了 opener）。
pub fn supported() -> bool {
    REVEALER.get().is_some()
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
}

#[derive(Serialize, Clone)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    /// 是不是当前这台机器能装的那一个
    pub current_platform: bool,
    /// 只给后端下载用，不下发给前端
    #[serde(skip)]
    url: String,
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub release_url: String,
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 把 `v1.4.8` / `1.5.0-beta.1` 拆成数字段；非数字段按 0 算。
fn version_parts(raw: &str) -> Vec<u64> {
    let trimmed = raw.trim().trim_start_matches('v');
    let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed);
    core.split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// GitHub 的 tag 带 `v` 前缀，界面显示的是纯版本号。
fn normalize_tag(raw: &str) -> String {
    raw.trim().trim_start_matches('v').to_string()
}

/// GitHub 响应 → 界面要的那份信息。单独拆出来是因为 `v` 前缀这种坑
/// 只有喂真实形状的 payload 才测得出来。
fn build_info(release: GhRelease, current: &str) -> UpdateInfo {
    // tag 自己带 `v`，前端渲染还会再加一次，这里统一去掉：留着就是 `vv1.4.8`
    let latest = normalize_tag(&release.tag_name);
    let assets = release
        .assets
        .into_iter()
        .filter_map(|asset| {
            let name = safe_asset_name(&asset.name)?;
            if !is_release_url(&asset.browser_download_url) {
                return None;
            }
            Some(Asset {
                current_platform: is_current_platform(&name),
                name,
                size: asset.size,
                url: asset.browser_download_url,
            })
        })
        .collect::<Vec<_>>();

    UpdateInfo {
        current: current.to_string(),
        has_update: is_newer(&latest, current),
        latest,
        release_url: release.html_url,
        published_at: release.published_at,
        assets,
    }
}

/// `a` 是否严格新于 `b`。位数不齐的按 0 补，所以 `1.5` 比 `1.4.9` 新。
fn is_newer(a: &str, b: &str) -> bool {
    let left = version_parts(a);
    let right = version_parts(b);
    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        if l != r {
            return l > r;
        }
    }
    false
}

/// 当前平台能装的那一个包。命名规则来自 release.yml 的 Tauri 产物。
/// 每个平台只允许命中一个：Windows 认 nsis 的 `-setup.exe`，不认 msi，
/// 否则一次发布会出现两个「本机可装」。
fn is_current_platform(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !(lower.starts_with("ai.gateway") || lower.starts_with("ai-gateway")) {
        return false;
    }
    let arm = std::env::consts::ARCH == "aarch64";
    let arch_hit = |candidate: &str| {
        if arm {
            candidate.contains("aarch64") || candidate.contains("arm64")
        } else {
            candidate.contains("x64") || candidate.contains("x86_64") || candidate.contains("amd64")
        }
    };
    match std::env::consts::OS {
        "windows" => lower.ends_with("-setup.exe") && lower.contains("x64"),
        "macos" => lower.ends_with(".dmg") && arch_hit(&lower),
        _ => lower.ends_with(".appimage") && arch_hit(&lower),
    }
}

/// 资产名会直接拼进本地路径，而 GitHub 上的名字不是我们能控制的输入：
/// 只保留最后一段文件名，且必须全在允许字符集内。
fn safe_asset_name(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
    {
        return None;
    }
    Some(name.to_string())
}

/// 下载地址必须还在本仓库的 release 下载前缀里，避免把接口响应里的任意 URL 当目标。
fn is_release_url(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/"))
}

fn build_client(connect_timeout: Duration) -> AppResult<reqwest::Client> {
    crate::config::SystemProxy::detect()
        .apply_to_builder(reqwest::Client::builder().connect_timeout(connect_timeout))
        .build()
        .map_err(|e| AppError::Internal(format!("创建 HTTP 客户端失败: {e}")))
}

static CACHE: Mutex<Option<(Instant, Arc<UpdateInfo>)>> = Mutex::new(None);

/// 向 GitHub 查最新 release 并和内置版本号比。
/// `force = false` 时优先用 10 分钟内的缓存，省下一次限流额度。
pub async fn check(force: bool) -> AppResult<UpdateInfo> {
    if !force {
        if let Some(cached) = CACHE.lock().ok().and_then(|slot| {
            slot.as_ref()
                .filter(|(at, _)| at.elapsed() < CACHE_TTL)
                .map(|(_, info)| info.as_ref().clone())
        }) {
            return Ok(cached);
        }
    }

    let client = build_client(Duration::from_secs(10))?;
    let response = client
        .get(LATEST_URL)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, "AI-Gateway")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("无法访问 GitHub: {e}")))?;

    let status = response.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(AppError::TooManyRequests(
            "GitHub 接口限流（未登录每小时 60 次），稍后再试".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "GitHub 返回 {status}：还没有发布过版本，或仓库不可访问"
        )));
    }

    let release: GhRelease = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("解析 GitHub 响应失败: {e}")))?;

    let info = build_info(release, current_version());
    if let Ok(mut slot) = CACHE.lock() {
        *slot = Some((Instant::now(), Arc::new(info.clone())));
    }
    Ok(info)
}

#[derive(Serialize, Clone)]
pub struct Progress {
    /// downloading / done / error
    pub state: String,
    pub name: String,
    pub received: u64,
    pub total: u64,
    /// 这次下载对应的是哪个版本。攒着不用就会串台：下了 1.5.0 没装，
    /// 过几天查到 1.6.0，面板上还挂着 1.5.0 的「双击安装包即可完成更新」。
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static PROGRESS: Mutex<Option<Progress>> = Mutex::new(None);

fn set_progress(progress: Progress) {
    if let Ok(mut slot) = PROGRESS.lock() {
        *slot = Some(progress);
    }
}

/// 当前下载进度；没下载过就是 idle，前端不用处理 null。
pub fn progress() -> Progress {
    PROGRESS
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(|| Progress {
            state: "idle".to_string(),
            name: String::new(),
            received: 0,
            total: 0,
            version: String::new(),
            path: None,
            error: None,
        })
}

/// 安装包落到用户的下载目录；拿不到家目录时退回应用目录下的 `updates/`。
fn downloads_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if !home.is_empty() {
        let dir = PathBuf::from(home).join("Downloads");
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    let fallback = crate::config::resolve_app_path("updates");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

/// 挑要下载的那个包：显式点名就用点名的，否则用本机平台的那个。
fn pick_asset(assets: &[Asset], name: Option<&str>) -> AppResult<Asset> {
    match name {
        Some(wanted) => assets
            .iter()
            .find(|asset| asset.name == wanted)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("这个发布里没有名为 {wanted} 的安装包"))),
        None => assets
            .iter()
            .find(|asset| asset.current_platform)
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound("这个发布里没有适配当前系统的安装包".to_string())
            }),
    }
}

/// 在锁内一次完成「空闲 → 下载中」。不能先读后写：中间那次 `check()` 要等网络，
/// 双击的两条请求都会读到 idle，于是起两个任务往同一个文件里写。
/// 返回 `Some` 表示已经有一个在跑，把它当前的进度还给调用方。
fn claim_download() -> AppResult<Option<Progress>> {
    let mut slot = PROGRESS
        .lock()
        .map_err(|_| AppError::Internal("下载状态不可用".to_string()))?;
    if slot.as_ref().map(|p| p.state.as_str()) == Some("downloading") {
        return Ok(slot.clone());
    }
    *slot = Some(Progress {
        state: "downloading".to_string(),
        name: String::new(),
        received: 0,
        total: 0,
        version: String::new(),
        path: None,
        error: None,
    });
    Ok(None)
}

/// 起一个后台下载：立刻返回，前端轮询 `progress()`。
/// 同一时刻只跑一个，重复点击直接复用当前进度。
pub async fn start_download(name: Option<String>) -> AppResult<Progress> {
    if !supported() {
        return Err(AppError::Conflict(
            "下载更新仅在桌面应用中可用".to_string(),
        ));
    }
    if let Some(busy) = claim_download()? {
        return Ok(busy);
    }

    let target = pick_target(name.as_deref()).await;
    let (asset, dest, version) = match target {
        Ok(triple) => triple,
        // 抢到了槽却没挑成，必须把状态还回去，否则进度永远停在「下载中」
        Err(error) => {
            abandon_download(&error);
            return Err(error);
        }
    };

    set_progress(Progress {
        state: "downloading".to_string(),
        name: asset.name.clone(),
        received: 0,
        total: asset.size,
        version: version.clone(),
        path: None,
        error: None,
    });
    actix_web::rt::spawn(download(asset, dest, version));
    Ok(progress())
}

/// 起头就失败：把「下载中」这个占位撤掉，否则进度条会一直挂着。
fn abandon_download(error: &AppError) {
    set_progress(Progress {
        state: "error".to_string(),
        name: String::new(),
        received: 0,
        total: 0,
        version: String::new(),
        path: None,
        error: Some(error_text(error)),
    });
}

/// `AppError` 的 `Display` 会拼上 `Conflict: ` 这类英文前缀，直接 `to_string()`
/// 就变成「下载没有完成：Conflict: 已经是最新版本」。进度里给人看，取正文。
fn error_text(error: &AppError) -> String {
    match error {
        AppError::NotFound(msg)
        | AppError::BadRequest(msg)
        | AppError::Forbidden(msg)
        | AppError::TooManyRequests(msg)
        | AppError::Conflict(msg)
        | AppError::Internal(msg) => msg.clone(),
        other => other.to_string(),
    }
}

/// 确认确实有新版本，并定下要下的包、落点与它属于哪个版本。
async fn pick_target(name: Option<&str>) -> AppResult<(Asset, PathBuf, String)> {
    let info = check(false).await?;
    if !info.has_update {
        return Err(AppError::Conflict("已经是最新版本".to_string()));
    }
    let asset = pick_asset(&info.assets, name)?;
    let dest = downloads_dir().join(&asset.name);
    Ok((asset, dest, info.latest))
}

/// 边下边写：先落 `.part`，成功后改名，免得半个包被当成完整安装包打开。
async fn download(asset: Asset, dest: PathBuf, version: String) {
    let name = asset.name.clone();
    let fail = |message: String| {
        set_progress(Progress {
            state: "error".to_string(),
            name: name.clone(),
            received: 0,
            total: 0,
            version: version.clone(),
            path: None,
            error: Some(message),
        });
    };

    let client = match build_client(Duration::from_secs(20)) {
        Ok(client) => client,
        Err(error) => return fail(error_text(&error)),
    };
    let response = match client
        .get(&asset.url)
        .header(reqwest::header::USER_AGENT, "AI-Gateway")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => return fail(format!("下载失败: {error}")),
    };
    if !response.status().is_success() {
        return fail(format!("下载失败: GitHub 返回 {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let file_name = dest
        .file_name()
        .map(|raw| raw.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());
    let part = dest.with_file_name(format!("{file_name}.part"));

    let mut file = match tokio::fs::File::create(&part).await {
        Ok(file) => file,
        Err(error) => return fail(format!("无法写入 {}: {error}", part.display())),
    };

    let mut received: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut last_tick = Instant::now();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Err(error) => {
                let _ = fs::remove_file(&part);
                return fail(format!("下载中断: {error}"));
            }
            Ok(bytes) => {
                if let Err(error) = file.write_all(&bytes).await {
                    let _ = fs::remove_file(&part);
                    return fail(format!("写入失败: {error}"));
                }
                received += bytes.len() as u64;
                // 300ms 刷一次就够，进度条不需要每个分片重绘一遍
                if last_tick.elapsed() >= Duration::from_millis(300) {
                    last_tick = Instant::now();
                    set_progress(Progress {
                        state: "downloading".to_string(),
                        name: name.clone(),
                        received,
                        total,
                        version: version.clone(),
                        path: None,
                        error: None,
                    });
                }
            }
        }
    }
    let _ = file.flush().await;
    drop(file);

    if let Err(error) = tokio::fs::rename(&part, &dest).await {
        let _ = fs::remove_file(&part);
        return fail(format!("下载完成但保存失败: {error}"));
    }

    set_progress(Progress {
        state: "done".to_string(),
        name,
        received,
        total: received,
        version,
        path: Some(dest.display().to_string()),
        error: None,
    });
}

/// 在文件管理器里定位下载好的安装包。
pub fn reveal_installer() -> AppResult<()> {
    let reveal = REVEALER
        .get()
        .ok_or_else(|| AppError::Conflict("定位安装包仅在桌面应用中可用".to_string()))?;
    let path = progress()
        .path
        .ok_or_else(|| AppError::NotFound("还没有下载好的安装包".to_string()))?;
    reveal(path).map_err(AppError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_handles_rollover() {
        assert!(is_newer("1.5.0", "1.4.8"));
        // 纯字符串比较会在这里翻车：8 > 10
        assert!(is_newer("1.4.10", "1.4.8"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.4.8", "1.4.8"));
        assert!(!is_newer("1.4.7", "1.4.8"));
        // 位数不齐按 0 补
        assert!(is_newer("1.5", "1.4.9"));
        assert!(!is_newer("1.4", "1.4.2"));
    }

    #[test]
    fn tag_prefix_and_prerelease_are_stripped() {
        assert_eq!(version_parts("v1.4.8"), vec![1, 4, 8]);
        assert_eq!(version_parts("1.5.0-beta.1"), vec![1, 5, 0]);
        assert!(is_newer("v1.5.0-beta.1", "1.4.8"));
    }

    #[test]
    fn tag_never_keeps_its_v_prefix() {
        // 前端渲染是 `v{latest}`，tag 自己带 v 就会出 `vv1.4.8`
        assert_eq!(normalize_tag("v1.4.8"), "1.4.8");
        assert_eq!(normalize_tag("  v1.4.8  "), "1.4.8");
        assert_eq!(normalize_tag("1.4.8"), "1.4.8");
    }

    fn release_fixture(tag: &str) -> GhRelease {
        // 带上各平台的真实产物名：断言才不会依赖跑测试的这台机器是什么系统
        let names = [
            "AI.Gateway_1.5.0_x64-setup.exe",
            "AI.Gateway_1.5.0_x64_en-US.msi",
            "AI.Gateway_1.5.0_x64.dmg",
            "AI.Gateway_1.5.0_aarch64.dmg",
            "AI.Gateway_1.5.0_amd64.AppImage",
        ];
        GhRelease {
            tag_name: tag.to_string(),
            html_url: "https://github.com/keiskeies/ai-gateway/releases/tag/v1.5.0".to_string(),
            published_at: Some("2026-11-18T02:13:09Z".to_string()),
            assets: names
                .iter()
                .map(|name| GhAsset {
                    name: name.to_string(),
                    size: 8972900,
                    browser_download_url: format!(
                        "https://github.com/keiskeies/ai-gateway/releases/download/v1.5.0/{name}"
                    ),
                })
                // 别处的资产不该成为下载目标
                .chain(std::iter::once(GhAsset {
                    name: "somewhere-else.bin".to_string(),
                    size: 1,
                    browser_download_url: "https://evil.example/file.bin".to_string(),
                }))
                .collect(),
        }
    }

    /// GitHub 真实响应是 `"tag_name": "v1.5.0"`，字段直接透传就会渲染成 `vv1.5.0`。
    #[test]
    fn github_shaped_payload_maps_to_display_ready_fields() {
        let info = build_info(release_fixture("v1.5.0"), "1.4.8");
        assert_eq!(info.latest, "1.5.0");
        assert_eq!(info.current, "1.4.8");
        assert!(info.has_update);
        // 非本仓库 release 前缀的资产被丢掉：5 个真产物，1 个外来 URL
        assert_eq!(info.assets.len(), 5);
        assert!(info.assets.iter().all(|a| is_release_url(&a.url)));
        // 每台机器恰好命中一个可装的包
        assert_eq!(
            info.assets.iter().filter(|a| a.current_platform).count(),
            1,
            "本机可装的包应当只有一个"
        );
        assert_eq!(info.published_at.as_deref(), Some("2026-11-18T02:13:09Z"));
    }

    #[test]
    fn same_release_is_not_an_update() {
        let info = build_info(release_fixture("v1.4.8"), "1.4.8");
        assert!(!info.has_update);
        assert_eq!(info.latest, "1.4.8");
    }

    #[test]
    fn a_second_click_never_restarts_the_download_it_claims() {
        set_progress(Progress {
            state: "idle".to_string(),
            name: String::new(),
            received: 0,
            total: 0,
            version: String::new(),
            path: None,
            error: None,
        });
        assert!(claim_download().unwrap().is_none(), "空闲时第一次点击必须抢到槽");

        set_progress(Progress {
            state: "downloading".to_string(),
            name: "AI.Gateway_1.5.0_x64-setup.exe".to_string(),
            received: 40_000_000,
            total: 100_000_000,
            version: "1.5.0".to_string(),
            path: None,
            error: None,
        });
        let busy = claim_download().unwrap().expect("已在下载时必须拿回当前进度");
        assert_eq!(busy.received, 40_000_000, "重复点击不能把进度清零");
        assert_eq!(busy.version, "1.5.0", "重复点击也不能把版本改掉");

        // 下一轮：下完之后槽要能重新抢到
        set_progress(Progress {
            state: "done".to_string(),
            name: busy.name,
            received: 100_000_000,
            total: 100_000_000,
            version: busy.version,
            path: None,
            error: None,
        });
        assert!(claim_download().unwrap().is_none());
        // 全局状态是本进程的下载槽，测完归位，别留给别的测试一个假的「下载中」
        set_progress(Progress {
            state: "idle".to_string(),
            name: String::new(),
            received: 0,
            total: 0,
            version: String::new(),
            path: None,
            error: None,
        });
    }

    #[test]
    fn asset_names_cannot_escape_the_download_dir() {
        assert_eq!(safe_asset_name("evil.exe").as_deref(), Some("evil.exe"));
        assert_eq!(
            safe_asset_name("AI.Gateway_1.4.8_x64-setup.exe").as_deref(),
            Some("AI.Gateway_1.4.8_x64-setup.exe")
        );
        // 只取最后一段：穿越被削平成一个普通文件名，落不到目录外面
        assert_eq!(safe_asset_name("../../windows/win.ini").as_deref(), Some("win.ini"));
        assert_eq!(safe_asset_name("..\\..\\x.exe").as_deref(), Some("x.exe"));
        // 目录名本身、空串、隐藏文件和带 shell 元字符的都拒掉
        assert_eq!(safe_asset_name(".."), None);
        assert_eq!(safe_asset_name(""), None);
        assert_eq!(safe_asset_name(".hidden"), None);
        assert_eq!(safe_asset_name("x;rm -rf /"), None);
    }

    #[test]
    fn download_url_is_pinned_to_the_repo() {
        assert!(is_release_url(
            "https://github.com/keiskeies/ai-gateway/releases/download/v1.4.8/a.exe"
        ));
        assert!(!is_release_url("https://evil.example/keiskeies/ai-gateway/a.exe"));
        assert!(!is_release_url(
            "http://github.com/keiskeies/ai-gateway/releases/download/v1.4.8/a.exe"
        ));
    }

    #[test]
    fn platform_match_picks_exactly_one_real_release_asset() {
        let expected = match std::env::consts::OS {
            "windows" => "AI.Gateway_1.4.8_x64-setup.exe",
            "macos" if std::env::consts::ARCH == "aarch64" => "AI.Gateway_1.4.8_aarch64.dmg",
            "macos" => "AI.Gateway_1.4.8_x64.dmg",
            _ => "AI.Gateway_1.4.8_amd64.AppImage",
        };
        let names = [
            "AI.Gateway-1.4.8-1.x86_64.rpm",
            "AI.Gateway_1.4.8_aarch64.dmg",
            "AI.Gateway_1.4.8_amd64.AppImage",
            "AI.Gateway_1.4.8_amd64.deb",
            "AI.Gateway_1.4.8_x64-setup.exe",
            "AI.Gateway_1.4.8_x64.dmg",
            "AI.Gateway_1.4.8_x64_en-US.msi",
        ];
        let hits: Vec<&&str> = names.iter().filter(|n| is_current_platform(n)).collect();
        assert_eq!(hits.len(), 1, "当前平台应只命中一个包，实际 {hits:?}");
        assert_eq!(hits[0], &expected);
    }

    #[test]
    fn pick_asset_falls_back_to_current_platform() {        let assets = vec![
            Asset {
                name: "AI.Gateway_1.4.8_aarch64.dmg".to_string(),
                size: 1,
                current_platform: false,
                url: "https://github.com/keiskeies/ai-gateway/releases/download/v1/a.dmg".to_string(),
            },
            Asset {
                name: "current.bin".to_string(),
                size: 2,
                current_platform: true,
                url: "https://github.com/keiskeies/ai-gateway/releases/download/v1/current.bin"
                    .to_string(),
            },
        ];
        assert_eq!(pick_asset(&assets, None).unwrap().name, "current.bin");
        assert_eq!(
            pick_asset(&assets, Some("AI.Gateway_1.4.8_aarch64.dmg"))
                .unwrap()
                .name,
            "AI.Gateway_1.4.8_aarch64.dmg"
        );
        assert!(pick_asset(&assets, Some("missing.exe")).is_err());
    }
}
