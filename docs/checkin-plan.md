# 每日领取 / 登录换 Key（并入平台页）

> 状态：第二版实现中　|　日期：2026-10-31　|　位置：平台页行内按钮 + 新建平台弹窗

## 1. 目标

很多平台每天给账号发免费额度，前提是「带着登录态访问一次」；有的平台还能直接用网页登录态换
API Key。两者都并进「平台」页，**不单独做签到页面**：

- 平台行尾：`登录` / `领取` / `重新登录`，一眼看到这个账号今天领没领；
- 页头：`一键登录/领取` —— 先把已登录的平台都领一遍，再给还没登录的逐个弹登录窗口；
- 新建平台：对支持的预设提供 `登录并获取 Key`，登录后把 SDK 令牌回填进表单。

**平台即账号**：同一个平台多个账号 = 同一预设建多条平台记录（网关路由本来就支持同 URL 多账号，
模型目录按 `catalog_key` 共享）。签到会话绑在 `platform_id` 上，一行平台最多一份会话。

明确不做：

- 不存账号密码，不做验证码/风控绕过。登录永远由用户在真窗口里完成。
- 不自动创建新的访问令牌：优先复用账号已有的 SDK 令牌，没有就让用户手动创建/填写。
- 不做「同一 WebView 配置目录里切号」：见 §3 的隔离要求。

## 2. 已确认的事实（2026-10-31 抓取线上资源）

| 事实 | 证据 |
| --- | --- |
| 魔粒接口基址 `${origin}/openapi`，Cookie 认证 | `cdn.modelscope.cn/static/assets/aigc-DDxQM74S.js`：`S = \`${i}/openapi\`` |
| 余额 `GET /openapi/v1/magicubes/balance`；未登录 HTTP 401 + `InvalidAuthentication` | 线上探测 |
| 每日任务 `GET /openapi/v1/magicubes/earn/rules`，`category_key=daily` 有 `today_remain/amount/scene` | 同上 |
| 页面**没有签到按钮**：`today_remain === 0` 即今日完成 | `usage.js` |
| 登录态 Cookie 名是 `m_session_id` | `internal-C_4IS3Q9.js` |
| 账号信息 `GET /api/v1/users/info`，旧信封 `{Success, Data:{Name}}`，未登录 401 / Code 1010101003 | 线上探测 |
| SDK 令牌列表 `GET /api/v1/users/tokens/list`，`Data.SdkTokens[]`（创建接口 `POST /api/v1/users/tokens` 存在） | 访问令牌页的 `umi.js` |
| Windows 上同步命令/事件回调里读 WebView2 cookie 会死锁；`cookies_for_url` 要放独立线程 | tauri 2.10.3 `webview/mod.rs` 已知问题 |
| 单窗口可指定独立 WebView2 配置目录：`WebviewWindowBuilder::data_directory` | tauri 2.10.3 `webview/mod.rs:968` |

**结论**：魔搭的「领取」= 带着有效登录态访问一次；「登录换 Key」= 用同一份 cookie 读账号名和令牌列表。

## 3. 登录流程（桌面端）

```
点「登录」/「登录并获取 Key」
  → POST /api/checkin/login | /api/checkin/platforms/{id}/login
  → core 生成 attempt（15 分钟有效）并调用桌面端注册的 opener
  → Tauri 主线程建窗口，**每个 attempt 一个临时 WebView2 配置目录**
  → 用户手动登录（验证码/扫码都在窗口里完成）
  → 独立线程每 1.5s cookies_for_url，直到出现 m_session_id
  → core::complete_attempt：GET users/info（账号名）+ GET tokens/list（SDK 令牌）
      · 已有平台 → 加密 cookie 直接绑定到 platform_id
      · 新平台   → cookie/账号名/Key 暂存在 attempt 里
  → 前端轮询 GET /api/checkin/attempts/{id}
      · 新平台：回填名称与 API Key，用户保存时带 login_attempt，创建后绑定会话
  → 关窗并删除临时配置目录
```

**为什么每个 attempt 单独配置目录**：WebView2 的 cookie 是按配置目录共享的。如果所有登录窗口共用一个目录，
第一个账号登录后，第二个账号的窗口一打开就是「已登录」，轮询会立刻把同一个账号再抓一遍。
独立目录的代价是每个账号都要完整登录一次，这是账号隔离的必然结果。

cookie 与 API Key 同级对待：`security.encrypt_key` 做 AES-GCM 落库，接口只回 cookie 名字。

## 4. 数据表（V31）

`checkin_accounts`（`platform_id` 唯一；删平台级联删会话）：

| 列 | 说明 |
| --- | --- |
| id | UUID |
| platform_id | 平台行 id（唯一） |
| platform | 适配器 key（当前只有 `modelscope`） |
| identity | 登录后的账号名 |
| label | 展示名（`ModelScope 魔搭 · {账号名}`） |
| cookies_enc / cookie_names | 加密的整条 Cookie 头 / 仅名字 |
| status | `new` / `ok` / `waiting` / `expired` / `error` |
| last_message | 最近一次运行的诊断 |
| last_run_at / last_run_day / last_ok_day | 运行时间 / 本地日期 |
| balance / balance_at | 领取时同步到的站点余额 / 同步时间（积分列「剩余」的锚点，见 §5） |
| daily_amount / daily_done | 今日到账 / 是否到账 |

## 5. 领取逻辑（core，桌面端与服务器模式都跑）

1. 先 `GET` 一次页面 URL（相当于「打开网页」），失败忽略；
2. `GET balance`：401 → `Expired`；`success=false` → `Failed`；
3. `GET earn/rules`：`daily` 分类里 `short_term` 看 `today_remain == 0`、`long_term` 看总额领完；
   `daily_done = 任一完成`，`daily_amount = 完成规则的 amount 之和`；
4. 余额与同步时间一并落库（`balance` / `balance_at`）；平台配了积分预算时，用上游可用余额
   **校准本地台账**：把余额记成 `platform_credit_state.anchor`（余额锚点）并把已用量清零
   （V34）。此后每笔请求从这份余额往下扣，积分列的分子显示 `anchor − spent`；
   失败的一次领取既不校准也不刷新余额；
5. 状态：`Expired → expired`；成功且到账 `→ ok`；成功未到账 `→ waiting`。

> 为什么不是 `spent = 预算 − 余额`：站点余额和配置预算本来就不是一回事（赠送魔粒、跨日结转）。
> 余额一旦高于预算，这个式子会给负数、被夹成 0，积分列于是恒等于预算数字，
> 看起来像「每次领取都变回 250」。
>
> 但**分母不变**：界面上的「总额」始终是配置的那条预算（用户配的每日赠送上限），
> 锚点只决定分子与路由闸门——余额 366 / 预算 250 就显示 `358 / 250`，
> 请求也允许跑到真实余额见底，而不是在 250 处提前拦截。

后台线程：每 60 秒 tick 一次，是否真的发请求由任务判定（见 §9）；`expired` 不再重试（重新登录重置成 `new`）。

## 6. API

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/checkin/accounts` | `{accounts, platforms:[{key,brand,base_url_prefix,login_supported}]}`；平台页按前缀匹配行 |
| POST | `/api/checkin/login` | 新平台流程，body `{platform}` → `{attempt_id}` |
| POST | `/api/checkin/platforms/{platform_id}/login` | 已有平台登录 / 重新登录 → `{attempt_id}` |
| GET | `/api/checkin/attempts/{id}` | 轮询登录结果；新平台流程带回 `api_key` |
| POST | `/api/checkin/platforms/{platform_id}/run` | 立即领取 |
| POST | `/api/checkin/run-all` | 一键领取所有已登录账号 |
| DELETE | `/api/checkin/platforms/{platform_id}/session` | 只退出登录，保留平台 |

- `POST /api/platforms` 增加可选 `login_attempt`：平台创建成功后把该 attempt 的会话绑到新平台。
- 整段 `/api/checkin` 仍只允许本机控制台；`GET /api/auth/status.checkin_login` 表示能否开登录窗口（只有 Tauri 为 true）。

## 7. 前端

- 平台页（`/providers`）：
  - 仅当 `local_console && checkin_login` 时显示「每日领取」列与页头按钮（服务器模式整列隐藏）；
  - 行内：状态点 + 账号名（未登录显示「未登录」）+ 按钮（`登录` → 登录并领取；`领取`；`expired` → `重新登录`）；
  - 页头「一键登录/领取」：先 `run-all`，再给缺会话的受支持平台逐个弹窗登录，最后汇总提示；
  - 新建平台弹窗：匹配到能力表时出现「登录并获取 Key」，成功后回填名称与 API Key，保存时带 `login_attempt`。
- 所有文案走 i18n（zh/en）。

## 8. 定时任务（V33）

任务 = **一个时间点位 + 多选账号**，只做「领取」。登录不在范围内：登录要在 WebView 里输密码、
过验证码，到点没人坐在电脑前时弹窗口只会晾在那里；未登录/已过期的账号在任务里**跳过**并在结果里点名。

### 表

`checkin_tasks(id, name, time, platform_ids, enabled, last_run_at, last_run_day, last_message, created_at, updated_at)`

- `time` 是 `'HH:MM'` **本地**时间（与 `_schema_version` 之外所有「今天」的口径一致，都按 `chrono::Local`）；
- `platform_ids` 是 JSON 数组，存平台行 id——平台即账号，所以「魔搭1 + 魔搭2」就是两个 id；
- `last_run_day` 记本地日期，用于「今天已经跑过」判定。

### 调度

runner 每 60 秒 tick，`task_due`：

1. `enabled`；
2. 本地时分 **≥** `time`（不是精确相等——应用没开或电脑睡着错过点位时，当天首次启动就补跑）；
3. 且满足其一：`last_run_day != 今天`（当天首轮），或距 `last_run_at` ≥ 3 小时（跟进仍未到账的账号）。

原来的「启动即全量领一遍 + 每 3 小时重试所有账号」隐式行为**取消**：当天首轮改由任务负责，
否则「00:05」这个时间点是假的。3 小时跟进保留，但只在任务今天已经跑过之后发生。

### 执行

`run_task(pool, config, task, force)`：目标 = `platform_ids` ∩ 有会话的账号；非 `force` 时再按
`needs_run`（今天已到账 / `expired` / 距上次不足 3 小时 → 跳过）过滤；逐个串行 `run_once`
（不并发，避免同一站点同时收到多个账号的请求）。汇总写 `last_message`，例如
`3 个账号：2 已到账 · 1 待发放 · 1 需重新登录`。

### API

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/checkin/accounts` | 响应新增 `tasks`，平台页一次请求拿到账号与任务 |
| POST | `/api/checkin/tasks` | `{name?, time, platform_ids}` |
| PUT | `/api/checkin/tasks/{id}` | 同上 |
| DELETE | `/api/checkin/tasks/{id}` | |
| POST | `/api/checkin/tasks/{id}/run` | 立即运行（`force`，绕过 `needs_run`） |

校验：`time` 必须是合法 `HH:MM`；`platform_ids` 非空，且每一行都能在能力表里匹配到受支持平台
（按 base_url 前缀，不按名字）——否则 400 并点名是哪一行。删平台时和 `checkin_accounts` 一起在
同一事务里从所有任务的 `platform_ids` 中剔除。

### 前端

平台页头部「定时任务」（与「一键登录/领取」同样只在 `local_console && checkin_login` 时出现）：
弹窗内是任务列表（时间 · 账号 · 上次结果 · 启用开关 · 立即运行/编辑/删除）和新建表单
（名称可选、时/分两个 Select、账号 MultiSelect 只列已登录的受支持平台行）。
顶部一行说明：到点需要应用正在运行（关窗口不算，进程还在），错过顺延到下次启动补跑。

## 9. 风险与待验证

| 风险 | 处理 |
| --- | --- |
| 仅调接口可能不触发「每日发放」 | 如实显示「尚未到账」；若实测如此，第二版加隐藏 WebView pulse |
| `m_session_id` 过期 | pulse 401 → `expired`，行内提示重新登录 |
| 账号没有 SDK 令牌 | 提示用户去令牌页创建或手动填写；不自动创建令牌 |
| 令牌列表字段名无文档 | 先按常见键取，再兜底扫描 `ms-` 前缀；首次真实账号登录后按实际响应校正 |
| 登录窗口被用户提前关掉 | attempt 停在 `pending`，15 分钟后过期；前端提示可重试 |
| 平台 cookie 泄密 | 加密落库、接口不回传、日志不打印 |
