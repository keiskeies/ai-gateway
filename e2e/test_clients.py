"""客户端接入页：临时 HOME + 真实格式的 Claude / Codex 配置，走「接入 → 撤销」。

不碰当前用户的 ~/.claude；AIGW_CLIENT_HOME 把六种客户端的路径全部锁在临时目录。
截图留在 .dsh-tmp/clients/，用于按第一次使用的人的视角检查排版。
"""

import json
import shutil
import sqlite3
import tempfile
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
SHOTS = REPO / ".dsh-tmp" / "clients"
INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"

ORIGINAL = {
    "permissions": {"allow": ["Bash"]},
    "mcpServers": {"fs": {"command": "npx"}},
    "env": {
        "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
        "ANTHROPIC_AUTH_TOKEN": "old-token",
        "ANTHROPIC_MODEL": "old-model",
    },
}

CODEX = 'model = "gpt-5"\n\n[features]\nmulti_agent = true\n'


@pytest.fixture(scope="module")
def clients_setup(instances):
    base = REPO / ".dsh-tmp"
    base.mkdir(parents=True, exist_ok=True)
    home = Path(tempfile.mkdtemp(prefix="aigw-clients-", dir=str(base)))
    (home / ".claude").mkdir(parents=True, exist_ok=True)
    (home / ".claude" / "settings.json").write_text(
        json.dumps(ORIGINAL, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    (home / ".codex").mkdir(parents=True, exist_ok=True)
    (home / ".codex" / "config.toml").write_text(CODEX, encoding="utf-8")

    instance = instances("clients", extra_env={"AIGW_CLIENT_HOME": str(home)})
    instance.api.setup_admin()
    with sqlite3.connect(instance.dir / "data" / "ai-gateway.db") as conn:
        conn.execute(
            "INSERT INTO proxies (id, name, created_at) VALUES ('p-agnes', 'agnes', '2026-01-01T00:00:00Z')"
        )
        conn.commit()
    yield instance, home
    shutil.rmtree(home, ignore_errors=True)


def _card(page, name: str):
    return page.locator(".console-client", has=page.locator(f".console-client-name:text-is('{name}')"))


def _click_in(locator, text: str) -> None:
    """在指定卡片里按可见文案点按钮，避免同名按钮跨卡片误点。"""
    button = locator.locator("button", has_text=text).first
    button.wait_for(state="visible", timeout=10000)
    button.evaluate("el => el.click()")


def _wait_diff_ready(card) -> str:
    card.locator(".console-client-computing").wait_for(state="detached", timeout=15000)
    card.locator(".console-client-diff, .console-client-same").first.wait_for(state="visible", timeout=10000)
    return card.locator(".console-client-changes").inner_text()


def test_connect_and_disconnect_claude_code_from_page(clients_setup, browser_factory):
    instance, home = clients_setup
    api = instance.api
    console = browser_factory(api, init=INIT)
    page = console.new_page()
    errors: list[str] = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)

    console.goto(page, "/clients", ".console-shell")
    page.wait_for_selector(".console-client", timeout=15000)
    SHOTS.mkdir(parents=True, exist_ok=True)

    claude = _card(page, "Claude Code")
    assert "当前指向 http://127.0.0.1:15721" in claude.locator(".console-client-state").inner_text()
    assert "old-model" in claude.locator(".console-client-state").inner_text()
    codex = _card(page, "Codex")
    assert "还没配置网关地址" in codex.locator(".console-client-state").inner_text()
    assert "gpt-5" in codex.locator(".console-client-state").inner_text()
    # 隔离的家目录里不该出现真实安装：未检测到的工具收进底部一行，只留名字
    absent = page.locator(".console-clients-absent-list").inner_text()
    assert "Hermes" in absent and "Claude Code" not in absent
    page.screenshot(path=str(SHOTS / "clients-list.png"), full_page=True)

    _click_in(claude, "接入")
    claude.locator(".console-client-body").wait_for(state="visible", timeout=10000)
    diff = _wait_diff_ready(claude)
    # 主行说人话，不出现 env.* 键名；密钥永不回显明文
    assert "网关地址" in diff and "专用 Key" in diff and "默认模型" in diff
    assert "env.ANTHROPIC_BASE_URL" not in diff
    assert "sk-aigw-" not in page.inner_text("body")
    page.screenshot(path=str(SHOTS / "clients-expanded.png"), full_page=True)

    # 9 个档位/细节键同值，折成一行；页面上不再出现任何裸 env 键名
    muted = claude.locator(".console-client-diff.muted").inner_text()
    assert "9 个模型档位与协议细节" in muted and "agnes" in muted
    assert "env." not in claude.locator(".console-client-changes").inner_text()
    # 值本来就等于目标的条目不该出现在「会改这些」里
    assert "http://127.0.0.1:%d" % instance.port not in diff.splitlines()[-1]

    _click_in(claude, "更多设置")
    claude.locator(".console-client-tiers").wait_for(state="visible", timeout=10000)

    # 排版探针：三列必须真的对齐——同一行两个控件中心 y 相同，跨行左右边线相同
    rows = claude.locator(".console-client-trow:not(.head)").evaluate_all(
        """rows => [...rows].map(row => [...row.children]
            .map(el => {
                const r = el.getBoundingClientRect();
                return { x: r.x, y: r.y, w: r.width, mid: r.y + r.height / 2 };
            })
            // Mantine 会在行里留一个 0 尺寸节点，它不参与对齐
            .filter(box => box.w > 0 && box.mid > 0))"""
    )
    assert len(rows) == 5, rows
    for index, row in enumerate(rows):
        assert len(row) == 3, f"row {index} lost a column: {row}"
        assert abs(row[1]["mid"] - row[2]["mid"]) <= 2, f"row {index} 的两个控件没对齐：{row}"
    for column in (0, 1, 2):
        edges = {round(row[column]["x"], 1) for row in rows}
        widths = {round(row[column]["w"], 1) for row in rows if column > 0}
        assert len(edges) == 1, f"column {column} 左边线不齐：{edges}"
        assert len(widths) <= 1, f"column {column} 宽度不一：{widths}"
    rail = claude.locator(".console-client-block-head").evaluate_all(
        "els => [...new Set(els.map(el => Math.round(el.getBoundingClientRect().x)))]"
    )
    assert len(rail) == 1, f"标签栏没对齐在同一条左边线：{rail}"
    widest = claude.locator(".console-client-more input, .console-client-more textarea").evaluate_all(
        "els => Math.max(...els.map(el => el.getBoundingClientRect().width))"
    )
    assert widest <= 620, f"控件被拉满整屏（{widest}px）"

    # 档位显示名、开关、请求参数：都写进 Claude Code 自己的环境变量
    # 按 label 定位：Mantine Select 会多带一个隐藏 input，按序号取会打进下拉的搜索框
    claude.get_by_label("显示名称").nth(1).fill("Opus 4.5")
    claude.locator(".mantine-Checkbox-root", has_text="禁用自动升级").locator("input").evaluate("el => el.click()")
    claude.get_by_label("请求超时 (ms)").fill("300000")
    diff = _wait_diff_ready(claude)
    assert "禁用自动升级" in diff and "请求超时" in diff
    # 档位显示名与其余同值键不再同值，折叠项要能展开看明细（且用中文行名）
    _click_in(claude, "另外")
    assert "Opus 档 显示名称" in claude.locator(".console-client-diff.muted").inner_text()
    page.screenshot(path=str(SHOTS / "clients-more.png"), full_page=True)

    _click_in(claude, "接入 AI Gateway")
    claude.locator(".console-client-result").wait_for(state="visible", timeout=15000)
    result = claude.locator(".console-client-result").inner_text()
    assert "已接入" in result and "怎么确认生效" in result and "新开一个终端" in result
    page.wait_for_timeout(400)

    applied = json.loads((home / ".claude" / "settings.json").read_text(encoding="utf-8"))
    assert applied["permissions"] == ORIGINAL["permissions"]
    assert applied["mcpServers"] == ORIGINAL["mcpServers"]
    assert applied["env"]["ANTHROPIC_BASE_URL"] == f"http://127.0.0.1:{instance.port}"
    assert applied["env"]["ANTHROPIC_MODEL"] == "agnes"
    assert applied["env"]["ANTHROPIC_AUTH_TOKEN"].startswith("sk-aigw-")
    assert applied["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"] == "agnes", "显示名不该改动实际请求模型"
    assert applied["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"] == "Opus 4.5"
    assert applied["env"]["DISABLE_AUTOUPDATER"] == "1"
    assert applied["env"]["API_TIMEOUT_MS"] == "300000"
    assert "正在走 AI Gateway" in claude.locator(".console-client-state").inner_text()
    page.screenshot(path=str(SHOTS / "clients-applied.png"), full_page=True)

    with sqlite3.connect(instance.dir / "data" / "ai-gateway.db") as conn:
        assert conn.execute("SELECT COUNT(*) FROM client_configs WHERE client_id = 'claude_code'").fetchone()[0] == 1
        assert conn.execute("SELECT enabled FROM api_keys ORDER BY created_at DESC LIMIT 1").fetchone()[0] == 1
    backups = list((instance.dir / "data" / "client-backups" / "claude_code").glob("*.bak"))
    assert backups, "接入前必须留下备份"

    # 接入后卡片保持展开：结果、下一步与「想退回去」就在同一屏
    secondary = claude.locator(".console-client-secondary")
    secondary.wait_for(state="visible", timeout=10000)
    assert "撤销接入" in secondary.inner_text() and "用备份整份覆盖" in secondary.inner_text()
    page.screenshot(path=str(SHOTS / "clients-managed.png"), full_page=True)

    _click_in(secondary, "撤销")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    console.click_text(page, "撤销")
    page.wait_for_function(
        "() => document.body.textContent.includes('已撤销接入')",
        timeout=15000,
    )
    page.wait_for_timeout(700)

    restored = json.loads((home / ".claude" / "settings.json").read_text(encoding="utf-8"))
    assert restored["permissions"] == ORIGINAL["permissions"]
    assert restored["mcpServers"] == ORIGINAL["mcpServers"]
    assert restored["env"]["ANTHROPIC_BASE_URL"] == "http://127.0.0.1:15721"
    assert restored["env"]["ANTHROPIC_MODEL"] == "old-model"
    assert "ANTHROPIC_AUTH_TOKEN" not in restored["env"], "旧凭据只存在于备份里"
    # 附加开关与参数是这次新写的，撤销后必须一起消失
    for key in ("DISABLE_AUTOUPDATER", "API_TIMEOUT_MS", "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"):
        assert key not in restored["env"], f"{key} survived the disconnect"
    with sqlite3.connect(instance.dir / "data" / "ai-gateway.db") as conn:
        assert conn.execute("SELECT COUNT(*) FROM client_configs").fetchone()[0] == 0
        assert conn.execute("SELECT enabled FROM api_keys").fetchone()[0] == 0

    violations = console.audit(page)
    assert not violations, violations
    assert errors == [], errors
    page.close()
