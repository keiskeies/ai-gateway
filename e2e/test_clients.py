"""客户端接入页：临时 HOME + 真实格式的 Claude 配置，走“预览 → 应用 → 撤销”。

不碰当前用户的 ~/.claude；AIGW_CLIENT_HOME 把六种客户端的路径全部锁在临时目录。
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
        "ANTHROPIC_BASE_URL": "http://old",
        "ANTHROPIC_AUTH_TOKEN": "old-token",
        "ANTHROPIC_MODEL": "old-model",
    },
}


@pytest.fixture(scope="module")
def clients_setup(instances):
    base = REPO / ".dsh-tmp"
    base.mkdir(parents=True, exist_ok=True)
    home = Path(tempfile.mkdtemp(prefix="aigw-clients-", dir=str(base)))
    (home / ".claude").mkdir(parents=True, exist_ok=True)
    settings = home / ".claude" / "settings.json"
    settings.write_text(json.dumps(ORIGINAL, ensure_ascii=False, indent=2), encoding="utf-8")

    instance = instances("clients", extra_env={"AIGW_CLIENT_HOME": str(home)})
    api = instance.api
    api.setup_admin()
    db = instance.dir / "data" / "ai-gateway.db"
    with sqlite3.connect(db) as conn:
        conn.execute(
            "INSERT INTO proxies (id, name, created_at) VALUES ('p-agnes', 'agnes', '2026-01-01T00:00:00Z')"
        )
        conn.commit()
    yield instance, home
    shutil.rmtree(home, ignore_errors=True)


def _wait_enabled(page, text: str, timeout: int = 15000) -> None:
    page.wait_for_function(
        """(text) => [...document.querySelectorAll('button')]
            .some(button => button.textContent.trim() === text && !button.disabled)""",
        arg=text,
        timeout=timeout,
    )


def test_claude_code_apply_and_restore_from_page(clients_setup, browser_factory):
    instance, home = clients_setup
    api = instance.api
    console = browser_factory(api, init=INIT)
    page = console.new_page()
    errors: list[str] = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)

    console.goto(page, "/clients", ".console-shell")
    page.wait_for_selector(".console-clients-table", timeout=15000)
    _wait_enabled(page, "预览变更")

    table = page.inner_text(".console-clients-table")
    assert "Claude Code" in table
    assert "已检测到" in table
    paths = page.evaluate("() => [...document.querySelectorAll('.console-clients-path')].map(el => el.getAttribute('title'))")
    assert str(home / ".claude" / "settings.json") in paths
    assert "未检测到" in table, "the isolated home must not leak the real Hermes config"

    console.click_text(page, "预览变更", "button")
    page.wait_for_selector(".console-clients-preview", timeout=10000)
    preview = page.inner_text(".console-clients-preview")
    assert "env.ANTHROPIC_BASE_URL" in preview
    assert "env.ANTHROPIC_MODEL" in preview
    assert "应用时写入新 Key" in preview
    assert "sk-aigw-" not in preview, "preview must never contain the real gateway secret"
    SHOTS.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(SHOTS / "clients-preview.png"), full_page=True)

    console.click_text(page, "应用接入", "button")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    console.click_text(page, "确认应用", "button")
    page.wait_for_function(
        "() => document.body.textContent.includes('已接入')",
        timeout=15000,
    )
    page.wait_for_timeout(700)

    applied = json.loads((home / ".claude" / "settings.json").read_text(encoding="utf-8"))
    assert applied["permissions"] == ORIGINAL["permissions"]
    assert applied["mcpServers"] == ORIGINAL["mcpServers"]
    assert applied["env"]["ANTHROPIC_BASE_URL"] == f"http://127.0.0.1:{instance.port}"
    assert applied["env"]["ANTHROPIC_MODEL"] == "agnes"
    token = applied["env"]["ANTHROPIC_AUTH_TOKEN"]
    assert token.startswith("sk-aigw-")
    page.screenshot(path=str(SHOTS / "clients-applied.png"), full_page=True)

    db = instance.dir / "data" / "ai-gateway.db"
    with sqlite3.connect(db) as conn:
        assert conn.execute("SELECT COUNT(*) FROM client_configs WHERE client_id = 'claude_code'").fetchone()[0] == 1
        key = conn.execute("SELECT enabled, key_hash FROM api_keys ORDER BY created_at DESC LIMIT 1").fetchone()
        assert key and key[0] == 1
    backups = list((instance.dir / "data" / "client-backups" / "claude_code").glob("*.bak"))
    assert backups, "apply must create a pre-apply backup"

    _wait_enabled(page, "撤销接入")
    console.click_text(page, "撤销接入", "button")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    console.click_text(page, "确认撤销", "button")
    page.wait_for_timeout(900)

    restored = json.loads((home / ".claude" / "settings.json").read_text(encoding="utf-8"))
    assert restored["permissions"] == ORIGINAL["permissions"]
    assert restored["mcpServers"] == ORIGINAL["mcpServers"]
    assert restored["env"]["ANTHROPIC_BASE_URL"] == "http://old"
    assert restored["env"]["ANTHROPIC_MODEL"] == "old-model"
    assert "ANTHROPIC_AUTH_TOKEN" not in restored["env"], "old credential is only in the backup"
    with sqlite3.connect(db) as conn:
        assert conn.execute("SELECT COUNT(*) FROM client_configs").fetchone()[0] == 0
        assert conn.execute("SELECT enabled FROM api_keys").fetchone()[0] == 0

    violations = console.audit(page)
    assert not violations, violations
    assert errors == [], errors
    page.close()
