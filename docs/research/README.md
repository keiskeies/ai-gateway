# 调研资料（支撑 `docs/trae-workbuddy-credits-plan.md`）

本目录是 `docs/trae-workbuddy-credits-plan.md` 的**一手调研材料**，保留原始证据与来源 URL，供实现时核对，也供日后判断「某个结论是否还成立」。

| 文件 | 内容 | 主要来源 |
|---|---|---|
| `trae.md` | Trae 国内/国际双站的模型接口、凭据形态、积分机制、ToS 原文 | 官方定价页与文档、`wangqi233/trae2api` 的 `docs/PROTOCOL.md`、`autumnsentiment/Trae2api-cn` |
| `workbuddy.md` | 腾讯 WorkBuddy / CodeBuddy 的接口、header、额度查询、风控对抗点、合规原文 | 腾讯官方文档、`yuan240324/workbuddy-openai-proxy`、`linguo2625469/workbuddy2api-panel` |
| `api-key-links.md` | 20 家厂商的 API Key 申请页 URL + OpenAI 兼容 base_url + 逐条实测状态码 | 逐 URL 实测（2026-10-27） |

## 调研背景（重要）

本会话的 `web_search` / `web_fetch` 工具**全部后端 403 不可用**，所有结论均改用 `pwsh` 直连 HTTP 取得。这带来两个方法论上的坑，已在设计文档 §7.3 与 §16 记录：

1. **ModelScope 是 umi SPA，任意 `/my/xxx` 都返回 200** → 状态码不能证明路径有效，需 grep `umi.js` 路由表
2. **Agnes 控制台是 Next.js，未知路径返回真实 404** → 可反向确认有效路径

验证 base_url 时用「不带 Key 请求 `/v1/models` 看是否 401」比看 200 更可靠——401 证明端点存在且需鉴权。

## 三个产品身份

用户口语与实际产品的对应关系（调研的核心产出之一）：

| 用户原话 | 实际产品 | 国内站 | 海外站 |
|---|---|---|---|
| trae | Trae（字节跳动） | `trae.cn` | `trae.ai` |
| workbuddy | WorkBuddy（腾讯，与 CodeBuddy 积分共享） | `workbuddy.cn` | `workbuddy.ai` |
| **agens** | **Agnes AI**（拼写 a-g-n-**e**-s） | `agnes-ai.cn` | `agnes-ai.com` |
| 魔塔社区 | **魔搭**社区 ModelScope（阿里） | `modelscope.cn` | — |

> 「agens」的判定证据：本仓库 `src/protocol/openai.rs:437`、`src/protocol/content.rs:128`、`src/proxy/handler.rs:1077` 已有 `agnes` 的注释，且文档里出现过 `agnes-3.0-flash` / `agnes-5` 模型名，与 Agnes AI 官方文档的模型命名吻合。

## 时效性提醒

这些材料标注了 2026-10-27 的实测状态。**以下事实会随时间变化，实现前应重新验证**：

- 各厂商的 Key 申请页 URL（会改版）
- Trae / WorkBuddy 的私有协议（随客户端升级变化）
- WorkBuddy 上游的封堵强度（2026-10 起正在收紧）
- 定价与积分档位
