# 全局 UI 设计系统 · Quiet Analytics

> 概览大屏的设计语言（见 `dashboard-screen-design.md`）已推广到全部页面。
> 本文是**其余页面**（平台 / 虚拟模型 / API Key / 设置）的统一规范：表格、弹窗、表单、按钮、状态标签、页面骨架。
> 实现集中在 `frontend/src/main.css`（token + 组件覆盖）与 `frontend/src/main.tsx`（Mantine 主题默认值），页面代码只写结构，不写视觉参数。

---

## 1. 五条铁律

1. **单一强调色**：全站只有一个彩色 `--accent #4C8DFF`（侧栏激活态、主按钮、页面标题图标、焦点环、关键胶囊）。
2. **状态色只有 3 个**：`--ok / --warn / --err`，且**只用于表达状态**（成功/警告/错误），不做装饰。已清除此前的紫/粉/青/橙等彩虹色滥用。
3. **层级靠明度，不靠阴影**：页底 `--wall-bg` → 面板 `--panel-bg` → 面板内卡片 `--panel-bg-2`，全站 `box-shadow: none`。
4. **数字一律等宽**：所有数字用 `.ds-num` / `tabular-nums` 对齐。
5. **微动效 150–300ms**：hover 改边框/底色，无位移发光；按钮按下 `translateY(1px)`。

---

## 2. Token（`main.css` `:root`，亮色由 `[data-mantine-color-scheme='light']` 覆盖）

| 变量 | 暗色 | 用途 |
|---|---|---|
| `--wall-bg` | `#0B1120` | 页底 / AppShell main |
| `--panel-bg` | `#111A2E` | 面板、表格、弹窗、顶栏、侧栏 |
| `--panel-bg-2` | `#182441` | 面板内卡片、输入框、下拉、悬停 |
| `--line` / `--line-strong` | `rgba(255,255,255,.06/.12)` | 1px 分隔 / 强调分隔 |
| `--text-1/2/3` | 纯白 / 60% / 40% | 主 / 次 / 辅助 |
| `--accent` | `#4C8DFF` | 唯一强调色 |
| `--ok/warn/err` | `#2ECC8F / #FFB020 / #FF5C5C` | 状态语义 |

---

## 3. 页面骨架

```tsx
<div className="page">
  <div className="page-head">
    <div>
      <div className="page-title">
        <span className="page-title-chip"><IconX size={17} stroke={1.7} /></span>
        {标题}
      </div>
      <div className="page-sub">{数量} {t(locale,'total')}</div>
    </div>
    <div className="page-actions">
      <Button variant="default" leftSection={<IconRefresh size={15} />}>{刷新}</Button>
      <Button leftSection={<IconPlus size={15} />}>{新建}</Button>
    </div>
  </div>

  <div className="ds-panel">
    <div className="ds-table-wrap">…表格…</div>
  </div>
</div>
```

- `.page-title-chip` 是**唯一**带强调色的装饰件（淡强调底 + 强调色图标），每页一个。
- `.page-actions` 固定顺序：次操作 `variant="default"` 在左，主操作（实心 accent）在右。
- `.ds-panel` 自带 1px 边框 + 10px 圆角 + 裁切，表格不再用 `Card shadow`。

---

## 4. 表格

Mantine `<Table>` 已被全局覆盖，无需额外 className：

- 表头：11px / 600 / 大写 / `letter-spacing .06em` / `--text-3`，底部 1px `--line-strong`，`stickyHeader` 吸顶。
- 单元格：13px，上下列分隔 1px `--line`，末行无分隔，行 hover = `--panel-bg-2`。
- 列宽：`<Table.Th style={{ width: 200 }}>` 定宽，弹性列留给描述性字段；**操作列放最后**并加 `className="ds-num"` 右对齐。
- 空态 / 加载：`<div className="ds-empty">图标 + 文案</div>`（`colSpan` 单元格内）。
- 数字列 / 时间列：`className="ds-num"`，颜色 `--text-3`，字号 12。

**列内元素**

| 场景 | 写法 |
|---|---|
| 主标识（模型名、Key 名） | `<span className="ds-pill accent bare ds-mono">` 或 `.ds-strong` |
| 分类标签（平台类型） | `<span className="ds-pill neutral bare">` |
| 后端/模型列表 | 最多 4 个 `.ds-pill neutral bare` + 溢出 `+N` 强调胶囊（tooltip 列全量） |
| 长 URL / 密钥 | `.ds-mono .ds-dim` + `maxWidth` + `ellipsis`，`title` 给全量 |
| 操作 | `Group justify="flex-end"` + `ActionIcon size="md"`，危险操作 `color="red"`，每个都套 `Tooltip` |

---

## 5. 弹窗

- 全局默认：`centered`、圆角 `lg`、遮罩 `rgba(...,.62)` + `blur(3px)`、`box-shadow: none`、淡入 180ms。
- 头部：`--panel-bg` + 底部 1px 分隔，标题 14/600。
- 内容区：`max-height: calc(100vh - 190px)` 内部滚动。
- **底部操作条 `.ds-modal-footer` 吸底常驻**（取消在左、主操作在右），长表单也不会把按钮推到屏幕外。
- 表单分组：每组 `.ds-form-section` 与 `.ds-form-section-title`（11px 大写灰）；字段用 `<Stack gap="sm">` 或 `.ds-field-grid`（两列，<700px 单列）。
- 危险确认：`Modal size="sm"` + `.ds-modal-footer`，确认按钮 `color="red"`。

**虚拟模型弹窗**额外用 `.ds-backend-card`（`--panel-bg-2` 底 + 圆角 8）承载每个后端：`.ds-backend-head` 内是 `.ds-backend-index`（强调色序号）+ 删除图标，字段用 `.ds-field-grid` 两列排布。

---

## 6. 表单与控件

- 输入框：`--panel-bg-2` 底 + 1px `--line` 边框 + 8px 圆角 + 13px 字；聚焦 = 强调色边框 + `0 0 0 3px` 淡强调光环。
- label 12/600 `--text-2`；description 11 `--text-3`；error 11 `--err`。
- 下拉/菜单/Popover：`--panel-bg-2` 底、1px 边框、**无阴影**，选项 hover = 14% 强调底。
- 分段控件：`--panel-bg-2` 容器 + 强调色指示块；Tabs 同色系。
- 按钮：主 = 实心 accent；次 = `variant="default"`（透明 + `--line-strong` 边框）；弱 = `variant="subtle"`；虚线「添加」= `.ds-add-btn`（**Mantine 7 无 `dashed` 变体，必须用这个类**）。

---

## 7. 状态胶囊

```tsx
<span className="ds-pill ok|warn|err|accent|neutral [bare]">…</span>
```

- 高度 20px、圆角 999px、11/600，默认带 6px 同色圆点（`bare` 去掉圆点）。
- 语义色只给状态；分类/中性信息用 `neutral`。

---

## 8. 侧栏与顶栏

- 顶栏/侧栏底 = `--panel-bg`，分隔 1px `--line`。
- 导航项 `.nav-item`：默认 `--text-2` → hover `--panel-bg-2` → 激活 = 13% 强调底 + 左侧 3px 强调竖条 + 600 字重。**不再使用彩色按钮**。

---

## 9. 落地清单（新增页面时对照）

- [ ] 用 `.page` / `.page-head` / `.page-title-chip` / `.page-sub` / `.page-actions` 起骨架
- [ ] 列表放 `.ds-panel` + `.ds-table-wrap`，表头定宽，操作列右对齐
- [ ] 空态用 `.ds-empty`，数字/时间用 `.ds-num`
- [ ] 弹窗表单分组 `.ds-form-section`，底部 `.ds-modal-footer`
- [ ] 状态用 `.ds-pill`，分类用 `.ds-pill neutral`
- [ ] 不写死颜色/阴影/圆角，一律走 token 与组件类
- [ ] 表单字段用 `components/Field.tsx`（label 与控件用 `useId()` 绑定），不手写兄弟节点 label
- [ ] 数据界面四态齐：加载 / 空 / 失败 / 过期；取数失败不退回 0 或"暂无数据"

---

## 10. 这一轮补上的硬规则

- **字号下限 11px**：正文/表格 12–13px，辅助与元信息 11px，大写 mono 小标也不低于 11px（靠字距 0.14–0.18em 与明度做层级，不靠缩小字号）。实现里曾出现 51 处 8px，1440 上不可读；整体抬升后重生成像素基线。
- **填充面的文字色走 `--accent-ink`**：Mantine 的 `autoContrast` 按 **light** 方案的 primaryShade 判深浅，暗色真正铺的底是 `brand[4]`，"亮蓝底自动配深字"从未生效（实测白字 2.7:1）。现在由 `variantColorResolver` 把 brand 填充面的文字交给 token：暗色深墨 6:1，浅色白字 5.8:1。不要靠调暗 `--accent` 来修对比度——它在暗色面板上同时是链接与文字色。
- **图标按钮自己带 `aria-label`**：`ActionIcon`、`Pagination`（用 `getControlProps` 给 first/previous/next/last）、无 label 的 `Select` 都要有名字。外面套 Mantine `Tooltip` 只写 `aria-describedby`，永远不进可访问名。
- **可访问名里不要把可见徽标数字放进括号**：axe 的 `label-content-name-mismatch` 先 NFKD 归一再删掉括号内容（全角括号也算），`通知中心（未读 1 条）` 判失败，`通知中心，未读 1 条` 通过。
- **失败不是空**：取数失败时面板写"这一块的数取不到，点上方重试"，KPI 大数换成 `—`（`.wall-kpi-missing`），计数副行同样换 `—`。远看的大屏上 `0` 和"没取到"长得一样。
- **响应式断点由页面自己反推**：期望列数从 `window.matchMedia` 取，不写死像素边界（1280 正好压在 `.wall-main-grid` 的 `max-width: 1279px` 上，滚动条算不算会决定分支）。
- **改动会被像素 diff 拦住**：`e2e/test_visual.py` 拿 `/`、`/traffic`、`/settings`、`/wall` 四屏与 `.dsh-tmp/visual/` 里的基线逐像素比，容忍 0.05%。有意改设计就 `UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py` 重生成，别去手改 PNG。
