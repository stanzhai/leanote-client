# 35 · 窗口 resize 时侧栏保宽

## 现象

拖动窗口边缘改变窗口大小时,左栏(nav)和笔记列表栏的宽度跟着变——用户拖好的宽度被窗口 resize 冲掉。

## 根因

`rust/vendor/gpui-base/src/resizable/mod.rs` `adjust_to_container_size()`:容器尺寸变化时把**所有面板按比例缩放**(`ratio = size/total`)。窗口 resize → `resizable/panel.rs` 的 group `on_prepaint` 检测到容器宽度变化 → 调 `adjust_to_container_size` → 侧栏按比例呼吸。

调用点有两类,语义不同:

- 窗口/容器 resize(`panel.rs:190`)——不是用户对面板的陈述
- 面板增删(`sync_panels_count` / `remove_panel` / `reset_panel`)——面板集合变了,幸存者应保比例(dock 的 tree-share 语义)

原实现两者共用同一个按比例缩放。

## 改动(vendor,继 e20b0e95 之后的又一次 gpui-base 定制)

`resizable/mod.rs` 拆成两个函数:

- `adjust_to_container_size`(仅窗口 resize 路径):sized 面板保宽(clamp 到自身 range),`container - sum(fixed)` 全部给**最后一个面板**(clamp 到其 range;低于 min 先压到 floor,再放不下交给 flexbox 溢出)。保留原 early-return:容器为零、任一面板 `size.is_none()`(placeholder,flex 已 fit)。
- `rebalance_shares`(面板增删路径):原按比例逻辑原样保留——dock 的 `sync_split_panels`/`adopt_sizes` 依赖 tree-share 语义。

细节:clamp 需容忍 `size_range` 为 `Default` 空区间 `0..0`(程序化构造、未渲染过的面板),否则 clamp 进空区间会把面板压成 0px(`dynamic_panel_lifecycle_is_owned_by_resizable_state` 测出)。

leanote-app 侧两个 group(`app_shell.rs` `main-panes` = [左栏 | 内层],内层 `mid-reader` = [中栏 | reader])末位都是 reader,正好匹配"末面板吸收增量"。

## 测试

- `mixed_sizing_is_stable_between_resize_and_followup_frame` 重写:容器 800→1200 后 sidebar 仍 240px、flexible 面板 960px、followup frame 稳定(原断言 `assert_ne!` 即"sidebar 必须跟着变",与新语义矛盾)。
- 新增 `a_window_resize_keeps_sized_panels_and_moves_only_the_remainder`:1200 和 600 两个方向,sidebar 恒 240。
- dock `removing_a_non_tail_child_shifts_the_split_sizes_with_it`:改为断言精确 px([768,1152] = tree 份额 200:300 缩放到 1920)。原断言从测量态取比例,而窗口 resize 不再重缩放测量态,两个真值源(树份额 vs 测量 px)只在编辑后一致——注释里写明。
- vendored 运行方式:`vendor/gpui-base/Cargo.toml` 末尾加空 `[workspace]`(否则 "believes it's in a workspace"),`cargo test` 1325 全绿;leanote-app 69 全绿;冒烟(LEANOTE_FAKE_SERVER)0 panic。

## 待观察

- dock dump/load 在"窗口 resize 后保存"场景:dump 用测量 px(保宽值),load 进不同宽度窗口时按份额重缩放——行为合理,leanote-app 未用 dock,不影响。
