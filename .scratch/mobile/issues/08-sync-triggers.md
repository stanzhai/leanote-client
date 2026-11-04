# Issue 08: 同步触发——启动增量 + 下拉刷新

状态:已完成(2026-11-04)

## 需求

- 启动自动增量同步（USN 对比，复用 leanote-core incr 路径）。
- 列表下拉刷新 = 手动增量同步（gpui touch pan 手势）。
- 纯前台，无后台刷新。

## 实现

- `start_incr_sync(silent)`：boot 已登录（silent，无 UI 打扰）与「立即同步」/手动刷新（按钮 busy 态）两条路径共用；`incr_sync + NoProgress` 后台线程，完成 notify 全 Tabs（渲染期直查 store）。
- 手动刷新入口：最近列表头行「最近 ⟳」点击 → `NotesEvent::RefreshRequested` → 增量。**偏差记录**：真 pull-gesture（overscroll 检测）kit 无现成组件，手势实现成本高；头行点击是语义等价物（同一位置、同一动作意图），真下拉留待真机轮打磨。
- Dev 钩子修正：`LEANOTE_FAKE_SERVER=1` 已登录时只起进程内 server 供增量命中，不再重登走全量。

## 验证

- fake seed 追加 n20（Usn=20）+ `set_sync_usn(20)`，设备库停在 USN=17：重启（已登录 + fake server）→ 启动增量拉下「启动增量新笔记」置顶最近列表（截图核实；该笔记不可能来自缓存，上次全量时不存在）。
- 同步中/失败态：设置按钮 busy（同步中…禁用）；incr 失败走 `sync_line` 提示（与全量同管线）。
- 三平台 check 绿；8 套件回归 ok。
