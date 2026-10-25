use std::time::Duration;

use crate::api::settings::SharedAppConfig;
use crate::db::DbPool;

/// 自动维护：按保留期清理请求明细，并顺带清理过期的积分流水。
///
/// 刻意不放进请求链路：清理是后台的整表操作，失败只影响“旧数据还在”，
/// 不影响任何转发。每轮检查配置，用户关掉开关后下一轮就停。
/// 启动时先跑一次，之后每小时看一眼，同一自然日只真正执行一次。
pub fn spawn_auto_purge(db: DbPool, config: SharedAppConfig) {
    std::thread::spawn(move || {
        let mut last_run_day: Option<String> = None;
        loop {
            let days = config.read().defaults.auto_purge_days;
            if days >= 30 {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                if last_run_day.as_deref() != Some(today.as_str()) {
                    match crate::db::stats::purge_older_than(&db, days as i64, false) {
                        Ok(result) => {
                            tracing::info!(
                                "自动清理：删除 {} 条请求明细、{} 条尝试、{} 条请求正文、{} 条请求头、{} 条响应正文（早于 {}）",
                                result.rows, result.attempts, result.bodies, result.headers, result.responses, result.cutoff
                            );
                        }
                        Err(e) => tracing::warn!("自动清理失败: {}", e),
                    }
                    // 积分流水保留 180 天；状态表不按时间清，永远只保留当前窗口一行。
                    let cutoff = (chrono::Utc::now() - chrono::Duration::days(180))
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
                    if let Err(e) = crate::db::credit::cleanup_ledger_before(&db, &cutoff) {
                        tracing::warn!("清理积分流水失败: {}", e);
                    }
                    last_run_day = Some(today);
                }
            }
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}
