# Issue 06: 搜索 Tab

状态:已完成(2026-11-04)

## 需求

- 复用统一搜索核心（桌面 ⌘K 同源），触屏输入框 + 结果列表（点击推入阅读页）。
- 中文拼音输入实测记录（IME 风险点）。

## 实现

- 新 `src/search_tab.rs`：`SearchTab`（InputState 输入框 + `leanote_core::search_notes` 250ms 防抖 + 结果行标题/摘录）。结果点击 `SearchEvent::OpenNote` → MobileApp 订阅 → 切笔记 Tab + `NotesTab::open_reader`（栈截断回根再推入）。
- 搜索实体在 `MobileApp::new` 随窗口建（InputState 需要 window，同步完成回调里没有），store 后挂（`set_store`）；与笔记 Tab 同一 attach 路径。
- Dev 深链：`LEANOTE_SEARCH=<kw>`（预填即搜）+ `LEANOTE_INITIAL_TAB=search`。

## 验证

- 模拟器截图：关键词「渲染」→ 两条结果，权重排序正确（标题命中「Markdown 渲染示例」在内容命中的「Mermaid 图示例」前），摘要截断展示，搜索 Tab 蓝色选中。
- 三平台 check 绿；8 测试套件回归 ok。

## Comments

### IME 记录（验收要求）

- 搜索框是 v1 唯一 IME 暴露面。kit 契约（桌面已验证）：IME preedit 期间不产生 `InputEvent::Change`，组合完成后一次性提交——`should_search` 同一谓词语义，拼音组合中途不会误触发搜索。
- **未自动化验证**：模拟器无法程序化调起系统键盘（simctl 无键盘 API），真实中文拼音组合输入的手感（候选条、组合中文本回显）留待 issue 09 真机/模拟器手动走查。gpui-mobile 自述 IME 组合输入未完成，Android 侧需 `GpuiInputActivity` 宿主（issue 10）。
