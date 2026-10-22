"""§12.7 视觉回归的最后一格：像素 diff。基线落在 .dsh-tmp/visual/（gitignore 里，不进仓库）。

确定性靠两件事，缺一个这条就是随机失败：
① 一份全新的空库——另起一个实例，不用 session 那份被别的用例写满统计的库；只建一个
   平台 / 虚拟模型 / Key，一笔网关请求都不打，所以屏上没有走势、没有相对时间。对象名字
   写死（不像 mock_stack 那样带时间戳 tag），否则每次跑屏上文字都在变。
② page.clock 把页面时间钉死——/wall 页头那颗时钟每秒在走，不钉住就是每秒一张新图。

首次跑生成基线并跳过断言；之后每次跑比对。改设计或换机器后
`UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py` 重新生成，别手改 PNG。
"""

import os
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

REPO = Path(__file__).resolve().parents[1]
BASELINES = REPO / ".dsh-tmp" / "visual"
FIXED_TIME = "2026-03-04T05:06:07"
# 单像素三通道差超过这个值才算"不同"：字体抗锯齿与亚像素抖动落在它以下
PIXEL_FLOOR = 24
# 允许不同的像素占比。同一份构建连跑两次实测 0.00%（端口写死 + page.clock 钉时间之后），
# 所以这条可以卡得很紧：0.05% 在 1440×900 上约 65 个像素，一次文字换色就超
RATIO_LIMIT = 0.0005

SHOTS = [
    ("/traffic", ".console-shell", "dark"),
    ("/settings", ".console-shell", "light"),
    ("/wall", ".wall-root", "dark"),
]


@pytest.fixture(scope="module")
def clean_api(instances, mock_upstream):
    """只有三个对象、零请求的库：屏上没有任何随时间或流量变化的读数。

    端口写死（1996，不在夹具禁用的 1994/1995 里）——端口会印在侧栏页脚、设置页的
    管理端口和数据库路径上，换端口就是换文字。
    """
    instance = instances("visual", 1996)
    if instance.port != 1996:
        pytest.skip("1996 被占用，端口落进屏上文字，像素 diff 不再稳定")
    api = instance.api
    api.setup_admin()
    tag = "vis-fixed"
    status, platform = api.call(
        "POST", "/api/platforms", {"name": tag, "type": "OpenAI", "base_url": mock_upstream, "api_key": "mock-key"}
    )
    assert status == 201, f"建平台失败 {status} {platform}"
    status, synced = api.call("POST", f"/api/platforms/{platform['id']}/models/sync")
    assert status == 200 and synced.get("created") == 1, f"同步模型失败 {status} {synced}"
    status, proxy = api.call("POST", "/api/proxies", {"name": tag})
    assert status == 201, f"建虚拟模型失败 {status} {proxy}"
    status, _ = api.call(
        "POST", f"/api/proxies/{proxy['id']}/routes", {"backends": [{"platform_id": platform["id"], "model_id": "mock-mini"}]}
    )
    assert status == 201, f"建路由失败 {status}"
    status, key = api.call("POST", "/api/api-keys", {"name": tag, "key": f"sk-{tag}-fixed-key-1234567890"})
    assert status == 201, f"建 Key 失败 {status} {key}"
    return api


def diff_ratio(baseline: Path, actual: Path) -> tuple[float | None, tuple[tuple[int, int], tuple[int, int]] | None]:
    """返回 (不同像素占比, 尺寸不匹配时的两边尺寸)；尺寸变了就不必再逐像素比。"""
    with Image.open(baseline) as a, Image.open(actual) as b:
        if a.size != b.size:
            return None, (a.size, b.size)
        left = np.asarray(a.convert("RGB"), dtype=np.int16)
        right = np.asarray(b.convert("RGB"), dtype=np.int16)
        return float((np.abs(left - right).max(axis=2) > PIXEL_FLOOR).mean()), None


@pytest.mark.parametrize("route,mount,scheme", SHOTS)
def test_visual_matches_baseline(clean_api, browser_factory, route, mount, scheme):
    name = f"{route.strip('/') or 'home'}-{scheme}"
    BASELINES.mkdir(parents=True, exist_ok=True)
    baseline = BASELINES / f"{name}.png"
    actual = BASELINES / f"{name}-actual.png"

    init = (
        "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped'); "
        f"localStorage.setItem('theme', '{scheme}')"
    )
    console = browser_factory(clean_api, init=init)
    page = console.new_page()
    page.clock.set_fixed_time(FIXED_TIME)
    console.goto(page, route, mount)
    # recharts 入场动画 500ms，等它落定再拍，否则拍到的是半张图
    page.wait_for_timeout(1200)
    page.screenshot(path=str(actual), full_page=True)
    page.close()

    if os.environ.get("UPDATE_BASELINES") or not baseline.exists():
        actual.replace(baseline)
        pytest.skip(f"基线{'已更新' if os.environ.get('UPDATE_BASELINES') else '首次生成'}：{baseline}")

    ratio, sizes = diff_ratio(baseline, actual)
    if ratio is None:
        pytest.fail(f"{name} 整页尺寸变了：基线 {sizes[0]}，本次 {sizes[1]}。本次截图在 {actual}")
    assert ratio <= RATIO_LIMIT, (
        f"{name} 与基线有 {ratio:.2%} 的像素不同（上限 {RATIO_LIMIT:.2%}）。"
        f"确认过是有意改动就重生成基线：UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py；"
        f"本次截图在 {actual}"
    )
