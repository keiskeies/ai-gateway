# 客户端接入 · 从 CC Switch 借鉴的配置托管

> 日期：2026-10-25
> 范围：Claude Code、Codex、OpenCode、OpenClaw、Pi、Hermes。
> 结论：AI Gateway 不复制 CC Switch 的整套本地代理，而是在现有网关之上增加「客户端配置托管」：把各客户端指向 1994，只改 AI Gateway 拥有的字段，应用前备份，可撤销。

---

## 1. CC Switch 的关键机制

参考仓库：`D:\item\cc-switch`（Tauri 2 + Rust；MIT）。

### 1.1 它确实跑本地路由

- 默认 `127.0.0.1:15721`，暴露 `/v1/messages`、`/v1/chat/completions`、`/v1/responses`、`/v1/models` 等端点。
- 客户端只连本地路由；路由按供应商做协议转换、故障转移、模型映射。
- Claude Code 的 `ANTHROPIC_BASE_URL` 指向该路由；Codex 的 `base_url` 指向 `…/v1`；OpenCode/OpenClaw/Pi/Hermes 写各自的 provider 条目。

### 1.2 配置写入的“地板”模型

核心不是“整份配置替换”，而是：

- 每个客户端只列出一组**关键字段**（发到哪、凭什么鉴权、用哪个模型、哪种协议）。
- 切换时只清空/重写这些字段；`permissions`、`hooks`、`mcpServers`、`projects`、其他 provider 等原样保留。
- 首次写下载备份；撤销时只回滚“当前值仍等于自己写入值”的字段，用户后来改过的不动。
- 写入前记录契约摘要；契约不变就不重写文件、不要求重启客户端。

### 1.3 各客户端的落点（摘要）

| 客户端 | 文件 | 关键内容 |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | `env.ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_MODEL`、四档 `ANTHROPIC_DEFAULT_*_MODEL` / `_NAME`、`CLAUDE_CODE_SUBAGENT_MODEL`；顶层权限/MCP/hooks 不动 |
| Codex | `~/.codex/config.toml` + `auth.json` | `model_provider`、`model`、`[model_providers.ai-gateway]`（`base_url`、`wire_api`、`experimental_bearer_token`）；`auth.json` 留给官方登录 |
| OpenCode | `~/.config/opencode/opencode.json[c]` | `provider.<id>` = `npm`、`options.baseURL`、`options.apiKey`、`models` |
| OpenClaw | `~/.openclaw/openclaw.json` | `models.providers.<id>` + `agents.defaults.model.primary` |
| Pi | `~/.pi/agent/models.json` | `providers.<id>`；当前模型在 `settings.json`，由 Pi 自己管理 |
| Hermes | `~/.hermes/config.yaml`（Windows：`%LOCALAPPDATA%\hermes\config.yaml`） | `custom_providers[]` + 顶层 `model` |

### 1.4 其它值得借鉴的点

- Claude Code 的模型档位固定为 Sonnet/Opus/Haiku/Fable；每档有“模型 ID + 显示名”，后台任务走 Haiku。
- Codex 的 `requires_openai_auth` 要按“盘上有没有官方登录”现算，避免把官方 OAuth 发给第三方。
- JSONC/JSON5/YAML 原生解析；写入会规范化注释/格式，但备份保留原文件。
- 客户端开关、Key 占位符、首次备份、登录 stash 都是“可逆”设计的一部分。

---

## 2. 融合后的设计

AI Gateway 已经是本地网关，不需要再起第二个代理。客户端直接连 AI Gateway 的 1994 端口：

- Claude Code 用根地址 + `/v1/messages`。
- Codex/OpenCode/OpenClaw/Pi/Hermes 用 `…/v1` + `/v1/chat/completions`。
- 虚拟模型名就是客户端配置里的 `model`；一条专用网关 Key 写进客户端配置。

### 2.1 页面与流程

新增「工具 → 客户端接入」（`/clients`）。页面按「一个工具一条卡片」组织，不是按接口字段组织：

1. 检测到的工具各占一条卡片，一行话说明现状（「正在走 AI Gateway · 模型 agnes」/「当前指向 …」/「还没配置网关地址」）；没检测到的工具收进底部一行，只列名字。
2. 点「接入」就地展开：只有默认模型是必填主控件，网关地址与 Claude 档位映射折进「更多设置」。
3. 展开即自动算差异（不点「预览」）：值本来就等于目标的条目不显示；`env.*` 键名翻译成「网关地址 / 专用 Key / 默认模型」；档位与协议细节同值时合成一行「9 个模型档位与协议细节 → agnes」；密钥只出现占位文案。
4. **接入**：复用或创建一条专用网关 Key → 备份原文件 → 写入 → 记录托管状态。没有额外确认弹窗——差异就在按钮上方，改前备份在按钮下方一句话说明。
5. 成功后卡片保持展开，给「怎么确认生效」（Claude Code 要重开终端）与一条跳「流量」的入口。
6. **撤销接入** / **用备份整份覆盖**降级到「想退回去」区，各自带确认弹窗，说明哪些会被还原、哪些会丢。

### 2.2 六端写入矩阵

| 客户端 | AI Gateway 写什么 | 默认模型 | 备注 |
|---|---|---|---|
| Claude Code | `env` 下的 base URL、AUTH_TOKEN、`ANTHROPIC_MODEL`、四档 `_MODEL`/`_MODEL_NAME`、`CLAUDE_CODE_SUBAGENT_MODEL` | 五档同一默认（可分别覆盖） | 只改 env；`ANTHROPIC_API_KEY` 若存在会提示 |
| Codex | `model_provider = "ai-gateway"`、`model`、`[model_providers.ai-gateway]`（Chat wire API + bearer token） | 单模型 | `auth.json` 不动；Responses 专属能力暂不可用 |
| OpenCode | `provider.ai-gateway` = `@ai-sdk/openai-compatible` + baseURL/apiKey/models | 单模型 | 读取 JSONC；写入规范化 |
| OpenClaw | `models.providers.ai-gateway` + `agents.defaults.model.primary = ai-gateway/<model>` | 单模型 | 读取 JSON5；写入规范化 |
| Pi | `providers.ai-gateway` | 单模型（由 Pi 选择） | 不写 `settings.json` |
| Hermes | `custom_providers.ai-gateway` + 顶层 `model.{default,provider,base_url}` | 单模型 | 读取 YAML；写入规范化 |

### 2.3 安全与可逆

- 仅管理员会话可调用 `/api/clients/*`。
- 写入路径由固定客户端 id 决定，**不接受任意路径**。
- 应用前把原文件复制到 `data/client-backups/<client>/<file>.<ts>.bak`；Unix 下 0600。
- 只存 token 的 SHA-256（V22 `client_configs` 表）；明文只在客户端配置和备份里。
- 撤销时停用接入 Key（不删除，保留历史用量归属）；已轮换/改过的 token 不覆盖。
- 原凭据无法原地恢复时明确提示；需要完整回退用「恢复应用前备份」。
- `AIGW_CLIENT_HOME` 只在调试构建生效（E2E 隔离），发布版忽略；测试时同时忽略 `HERMES_HOME` 等客户端覆盖，避免误写真实目录。

### 2.4 协议边界

- Claude Code 走 Anthropic Messages（已有）。
- 其余五端走 OpenAI Chat Completions（已有）。
- Codex 目前写 `wire_api = "chat"`，因此 Codex 的 Responses 专属能力不可用；配置里已提示需要支持 `experimental_bearer_token` 的 Codex 版本。
- 未实现：`/v1/responses`、Codex OAuth / ChatGPT 订阅、聚合模型别名、MCP/Skills/Prompts 管理、Kimi/MiniMax/Oh My Pi 等 CC Switch 的其它客户端。

---

### 2.5 Claude Code 的附加参数：抄了什么、为什么不抄剩下的

cc-switch 的 Claude 面板上有一排字段，按"最终写到哪"分成三类，只有第一类适合进 `/clients`。

**写进 `settings.json` 的（已实现）**

| 面板字段 | 落点 |
|---|---|
| 档位显示名称 | `ANTHROPIC_DEFAULT_{SONNET,OPUS,HAIKU,FABLE}_MODEL_NAME`，与实际请求的 `_MODEL` 分开 |
| 禁用自动升级 | `DISABLE_AUTOUPDATER=1` |
| 启用 Tool Search | `ENABLE_TOOL_SEARCH=true` |
| 禁用 Artifact 工具 | `CLAUDE_CODE_DISABLE_ARTIFACT=1` |
| 关闭 auto mode 服务端检查 | `CLAUDE_CODE_AUTO_MODE_SERVER=0` |
| 隐藏 AI 署名 | `CLAUDE_CODE_ATTRIBUTION_HEADER=0` |
| 超时 / 最大输出 / 上下文窗口 | `API_TIMEOUT_MS`、`CLAUDE_CODE_MAX_OUTPUT_TOKENS`、`CLAUDE_CODE_MAX_CONTEXT_TOKENS` |
| 后台小模型 | `ANTHROPIC_SMALL_FAST_MODEL`（选虚拟模型） |
| 自定义 Header | `ANTHROPIC_CUSTOM_HEADERS`，每行一个「名称: 值」 |

约束：只有 `CLAUDE_ENV_KEYS` 白名单里的键可写，传别的按名字拒绝——这一页不是通用环境变量编辑器。取消勾选会把键从文件里删掉，所以 `preview` 必须回读文件当前值给表单播种，否则"没勾"分不清是"没开"还是"没读到"。反复接入时托管记录里保留的始终是**用户最初的值**，不是上一次 AI Gateway 写的值。

**由 cc-switch 自己的代理消费，不进配置文件（不抄）**：上游格式（OpenAI Chat ↔ Anthropic 转换）、完整 URL 开关、User-Agent、Header/Body 覆盖。这些在我们的架构里对应「平台/后端」的协议转换与 `param_overrides`；搬到客户端页等于同一件事摆两个地方，两边都能改就没人知道哪边生效。

**逃生口：直接编辑配置文件**。白名单永远会漏键（`CLAUDE_CODE_DISABLE_1M_CONTEXT` 就是一开始没收录的），所以卡片动作行里有「直接编辑配置文件」：读原文、按该客户端的格式校验（JSON / TOML / JSON5 / YAML）、备份后整体写回。它**绕过托管**——写进去的内容不进 `managed`，不出现在差异预览里，撤销接入也不会替你还原；语法不过一律 400 拒绝，一个字节都不写。

**需要网关侧先配合（暂缓）**：「声明支持 1M」。cc-switch 的做法是给模型 id 加 `[1m]` 后缀，Claude Code 原样发出；我们的虚拟模型按名字精确匹配，直接写后缀会让请求打不到模型。要做，得先在解析时剥掉 `[1m]` 并把它当成上下文窗口标记——属于后端改动，先只开放 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 这个全局窗口值。

## 3. 代码落点

- `src/clients/mod.rs`：客户端 id、路径解析、Adapter 契约、公共类型。
- `src/clients/{claude,codex,opencode,openclaw,pi,hermes}.rs`：各端 plan/status/restore。
- `src/clients/service.rs`：模型/地址校验、Key 复用或创建、备份、原子写入、状态、撤销。
- `src/db/client_config.rs` + V22 迁移：托管状态表。
- `src/api/client_config.rs`：`/api/clients` 列表、`/preview`、`/apply`、`/restore`、`/restore-backup`、`/config`（读写原文）。
- `frontend/src/pages/Clients.tsx`：卡片列表、就地展开、自动差异（语义化 + 同值合并）、结果与「想退回去」。
- `e2e/test_clients.py`：临时 HOME + 真实格式 Claude 配置的端到端走查。

---

## 4. 验证

- `cargo test`：125 个 lib 测试（含六适配器、服务层 Key 生命周期、V22 迁移）。
- `python -m pytest e2e -q`：26 条（客户端接入 1 条：展开即算差异且不含密钥、裸 `env.*` 键名不出现、数值键写成 JSON 数字、应用后权限/MCP 保留、备份存在、撤销后 Key 停用、直接编辑拒绝非法 JSON 且不动文件、排版对齐探针、axe 0 违规；截图留在 `.dsh-tmp/clients/`）。
- a11y/键盘矩阵已加入 `/clients`，明暗两套都通过。

---

## 5. 已知限制与下一步

1. **Codex Responses**：如果要完整支持 Codex，需要新增 `/v1/responses` 与 SSE 事件转换；当前 Chat 模式是兼容折中。
2. **OAuth / 订阅**：CC Switch 的 Codex OAuth、Kimi OAuth 等不在本次范围；AI Gateway 目前以 API Key 为主。
3. **聚合模型列表**：AI Gateway 用虚拟模型聚合供应商；客户端 `/model` 能选的模型名就是虚拟模型名。若要像 CC Switch 那样按供应商前缀列出一组模型，需要额外的模型目录/别名机制。
4. **格式保真**：OpenCode/OpenClaw/Pi 的 JSONC/JSON5 与 Hermes 的 YAML 在写入时会规范化；备份保留原文件，但当前没有 JSONC/YAML 注释保留器。
5. **旧凭据恢复**：撤销只回滚非敏感字段；旧 token 需要整份备份恢复。若要做到原地恢复旧 token，需要加密保存 before 值或把备份做成可选择的版本。
6. **客户端检测**：以配置文件是否存在为准；没有检查 CLI 是否安装、版本是否支持对应 wire API。
7. **备份不在整库快照里**：`data/client-backups` 是独立文件，数据库备份/还原不包含它。
