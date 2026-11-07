//! 移动端壳（issue 02/03）：登录门 → 全量同步 → 底部 Tab（笔记/搜索/设置）。
//! 各 Tab 内容占位逐 issue 填充；同步只拉不推（ADR-0013）。

use crate::i18n::t;
use crate::login::{LoginEvent, LoginView};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::label::Label;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rems, App, Entity, SharedString, Task, Window};
use leanote_core::sync::{full_sync, Api, SyncProgress, SyncStats};
use leanote_core::{app_data_dir, Account, Store};
use std::sync::{Arc, Mutex};

enum SyncPhase {
    Progress,
    Done(SyncStats),
}

pub struct MobileApp {
    account: Option<Account>,
    /// 登录账户的设备端库；登录前无。Arc<Mutex> 让后台同步线程与 UI 共享。
    pub store: Option<Arc<Mutex<Store>>>,
    sync: Option<SyncPhase>,
    sync_line: SharedString,
    sync_tick: (usize, usize),
    login: Entity<LoginView>,
    shell: Entity<TabShell>,
    /// 输入框要随窗口建（同步完成时无 window），故搜索实体常驻。
    search_entity: Entity<crate::search_tab::SearchTab>,
    settings_entity: Entity<crate::settings_tab::SettingsTab>,
    prefs: crate::prefs::MobilePrefs,
    _task: Option<Task<()>>,
}

impl MobileApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 已登录账户直接进入主壳（离线可读，issue 08 再加启动增量）。
        let boot = Store::open(app_data_dir(), "user1".into())
            .ok()
            .and_then(|s| {
                s.active_account()
                    .ok()
                    .flatten()
                    .map(|account| (Arc::new(Mutex::new(s)), account))
            });
        let (store, account) = match boot {
            Some((s, a)) => (Some(s), Some(a)),
            None => (None, None),
        };
        let remembered = Store::open(app_data_dir(), "user1".into())
            .ok()
            .and_then(|s| s.last_account().ok().flatten());

        // Dev 冒烟（桌面同款约定）：LEANOTE_FAKE_SERVER=1 起进程内 fake server
        // 并自动登录——模拟器无法程序化键入。`http://…` 只预填 host。
        let fake = std::env::var("LEANOTE_FAKE_SERVER")
            .ok()
            .filter(|v| !v.is_empty());
        let prefill = fake
            .as_ref()
            .filter(|v| v.starts_with("http"))
            .map(|v| (v.clone(), String::new()))
            .or(remembered);

        let prefs = crate::prefs::MobilePrefs::load();
        crate::i18n::set_lang(&prefs.lang);
        // 主题/缩放（issue 07）：在窗口语境里应用，避开 open_window 前的
        // mode-reload 覆盖；外观切换 observer 重放。
        crate::theme::apply(&prefs, Some(window), cx);
        let prefs_for_observer = prefs.clone();
        window
            .observe_window_appearance(move |_window, cx| {
                // App 级外观此刻 keyWindow 已建，读得到真相。
                crate::theme::apply(&prefs_for_observer, None, cx);
            })
            .detach();
        let account_info = account.as_ref().map(|a| {
            (
                a.email
                    .clone()
                    .is_empty()
                    .then(|| a.username.clone())
                    .unwrap_or_else(|| a.email.clone()),
                a.host.clone(),
            )
        });

        let login = cx.new(|cx| LoginView::new(prefill, window, cx));
        let search_entity = cx.new(|cx| crate::search_tab::SearchTab::new(window, cx));
        let settings_entity =
            cx.new(|_| crate::settings_tab::SettingsTab::new(prefs.clone(), account_info));
        // Dev 深链：LEANOTE_INITIAL_TAB=search（模拟器无法点击 Tab）。
        let initial_tab = match std::env::var("LEANOTE_INITIAL_TAB").as_deref() {
            Ok("search") => TabId::Search,
            Ok("settings") => TabId::Settings,
            _ => TabId::Notes,
        };
        let shell = cx.new(|_| TabShell {
            tab: initial_tab,
            search: Some(search_entity.clone()),
            settings: Some(settings_entity.clone()),
            ..TabShell::default()
        });
        // 设置事件（issue 07）：偏好应用/持久化、立即同步、登出。
        cx.subscribe(
            &settings_entity,
            |this, _, event: &crate::settings_tab::SettingsEvent, cx| match event {
                crate::settings_tab::SettingsEvent::PrefsChanged(p) => {
                    crate::i18n::set_lang(&p.lang);
                    this.prefs = p.clone();
                    p.save();
                    crate::theme::apply(&p, None, cx);
                    cx.notify();
                }
                crate::settings_tab::SettingsEvent::SyncNow => this.start_incr_sync(cx, false),
                crate::settings_tab::SettingsEvent::Logout => this.logout(cx),
            },
        )
        .detach();
        cx.subscribe(&login, |this, _, event, cx| match event {
            LoginEvent::Success(account) => {
                this.account = Some(account.clone());
                this.start_full_sync(account.clone(), cx);
            }
        })
        .detach();

        if fake.as_deref().is_some_and(|v| !v.starts_with("http")) && account.is_none() {
            let rx = crate::fake::boot_fake_server();
            let login = login.clone();
            cx.spawn(async move |_, cx| {
                let url = cx
                    .background_spawn(async move { rx.recv().expect("fake server url") })
                    .await;
                login.update(cx, |l, cx| {
                    l.submit_with(url, "user@example.com".into(), "right-password".into(), cx);
                });
            })
            .detach();
        }

        let mut app = Self {
            account,
            store,
            sync: None,
            sync_line: "".into(),
            sync_tick: (0, 0),
            login,
            shell,
            search_entity,
            settings_entity,
            prefs,
            _task: None,
        };
        let already = app.account.is_some();
        if already && fake.as_deref().is_some_and(|v| !v.starts_with("http")) {
            let _ = crate::fake::boot_fake_server(); // 已登录：仅起 server 供增量
        }
        if already {
            app.attach_notes_tab(cx);
            // 启动自动增量（issue 08，ADR-0013 前台同步）。
            app.start_incr_sync(cx, true);
        }
        app
    }

    fn sync_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let muted = cx.theme().muted_foreground;
        let mut col = gpui_kit::base::v_flex()
            .gap_3()
            .w(px(300.))
            .child(Label::new(t("sync_title")).text_size(rems(1.4)));
        match self.sync.as_ref().expect("sync_screen only while syncing") {
            SyncPhase::Progress => {
                let (done, total) = self.sync_tick;
                col = col
                    .child(
                        Label::new(self.sync_line.clone())
                            .text_size(rems(0.85))
                            .text_color(muted),
                    )
                    .child(
                        Label::new(if total > 0 {
                            format!("{} {done}/{total}", t("note_content"))
                        } else {
                            "…".into()
                        })
                        .text_size(rems(0.85))
                        .text_color(muted),
                    );
            }
            SyncPhase::Done(stats) => {
                col = col
                    .child(
                        Label::new(format!(
                            "同步完成：{} 笔记本 · {} 篇笔记（内容 {}）",
                            stats.notebooks, stats.notes, stats.contents
                        ))
                        .text_size(rems(0.85))
                        .text_color(muted),
                    )
                    .child(
                        Button::new("start")
                            .label(t("start"))
                            .primary()
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.sync = None;
                                cx.notify();
                            })),
                    );
            }
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .child(col)
    }

    /// 立即同步/启动增量（issue 08）：incr_sync，完成后刷新 Tabs。
    fn start_incr_sync(&mut self, cx: &mut Context<Self>, silent: bool) {
        let Some(account) = self.account.clone() else {
            return;
        };
        let Some(store) = self.store.clone() else {
            return;
        };
        self._task = Some(cx.spawn(async move |this, cx| {
            let silent = silent;
            let stats = cx
                .background_spawn(async move {
                    let s = store.lock().unwrap();
                    let api = Api::for_account(&account);
                    leanote_core::sync::incr_sync(&api, &s, &leanote_core::sync::NoProgress)
                        .map_err(|e| e.to_string())
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if let Ok(st) = &stats {
                    v.sync_line = format!(
                        "{}：{} 笔记本 · {} 笔记",
                        crate::i18n::t("sync_done"),
                        st.notebooks,
                        st.notes
                    )
                    .into();
                }
                // 设置按钮复位 + 列表重查（渲染期直查 store，notify 即刷新）。
                if !silent {
                    let _ = v
                        .settings_entity
                        .update(cx, |s, cx| s.set_sync_busy(false, cx));
                }
                v.shell.update(cx, |_, cx| cx.notify());
                let _ = v
                    .notes_entity(cx)
                    .map(|n| n.update(cx, |_, cx| cx.notify()));
                cx.notify();
            });
        }));
    }

    /// 当前笔记 Tab 实体（若有）。
    fn notes_entity(&self, cx: &App) -> Option<Entity<crate::notes_tab::NotesTab>> {
        self.shell.read(cx).notes.clone()
    }

    /// 登出（issue 07）：deactivate + 回登录页。数据保留在设备库。
    fn logout(&mut self, cx: &mut Context<Self>) {
        if let Some(store) = &self.store {
            let _ = store.lock().unwrap().deactivate_account();
        }
        self.account = None;
        self.store = None;
        self.sync = None;
        self.shell.update(cx, |s, cx| {
            s.notes = None;
            s.tab = TabId::default();
            cx.notify();
        });
        let _ = self.search_entity.update(cx, |s, cx| s.clear(cx));
        cx.notify();
    }

    /// 库就绪后挂上笔记/搜索 Tab（boot 已登录或同步完成两个路径共用）。
    fn attach_notes_tab(&mut self, cx: &mut Context<Self>) {
        // 已挂载（boot 路径登录过）则跳过：重复挂载会换新实体，把用户
        // 正在进行的浏览栈/抽屉状态整屏重置。
        if self.shell.read(cx).notes.is_some() {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let notes = cx.new(|_| crate::notes_tab::NotesTab::new(Some(store.clone())));
        // 手动刷新（issue 08）：最近头行点击 → 静默增量。
        cx.subscribe(
            &notes,
            |this, _, event: &crate::notes_tab::NotesEvent, cx| match event {
                crate::notes_tab::NotesEvent::RefreshRequested => this.start_incr_sync(cx, false),
                // 搜索进入的阅读屏返回 → 回到搜索列表。
                crate::notes_tab::NotesEvent::ReturnToSearch => {
                    let _ = this.shell.update(cx, |s, cx| {
                        s.tab = TabId::Search;
                        cx.notify();
                    });
                }
            },
        )
        .detach();
        let search = self.search_entity.clone();
        let _ = search.update(cx, |t, cx| t.set_store(store, cx));
        // 搜索结果点击 → 切到笔记 Tab 并推入阅读屏。
        let notes_for_nav = notes.clone();
        let shell = self.shell.clone();
        cx.subscribe(
            &search,
            move |_, _, event: &crate::search_tab::SearchEvent, cx| match event {
                crate::search_tab::SearchEvent::OpenNote { note_id, title } => {
                    let notes = notes_for_nav.clone();
                    let _ = shell.update(cx, |s, cx| {
                        s.tab = TabId::Notes;
                        cx.notify();
                    });
                    let _ = notes.update(cx, |n, cx| {
                        n.open_reader(note_id.clone(), title.clone(), cx)
                    });
                }
            },
        )
        .detach();
        let shell = self.shell.clone();
        let _ = shell.update(cx, |s, cx| {
            s.notes = Some(notes.clone());
            // 阅读屏进出（NotesTab push/pop）要联动 Tab 栏显隐 → 观察。
            cx.observe(&notes, |_, _, cx| cx.notify()).detach();
            cx.notify();
        });
    }

    /// 登录后的全量同步（issue 03）：只拉不推。进度流回 UI，完成后展示
    /// 摘要行 + 「开始使用」（对齐桌面同步摘要卡语义的最小版）。
    fn start_full_sync(&mut self, account: Account, cx: &mut Context<Self>) {
        self.sync = Some(SyncPhase::Progress);
        self.sync_line = t("syncing").into();
        self.sync_tick = (0, 0);
        cx.notify();

        // 桌面同款：ChannelProgress 把 note()/tick() 挤进一个 channel，
        // sender 随 work 结束而 drop，recv() 结束即同步完成。
        let (tx, rx) = smol::channel::unbounded::<String>();
        let progress = ChannelProgress(Arc::new(Mutex::new(tx)));
        let dir = app_data_dir();
        let uid = account.user_id.clone();
        let work = cx.background_spawn(async move {
            (|| -> Result<(SyncStats, Arc<Mutex<Store>>), String> {
                let store =
                    Store::open(dir, uid).map_err(|e| format!("cannot open local store: {e}"))?;
                let store = Arc::new(Mutex::new(store));
                let api = Api::for_account(&account);
                let stats = full_sync(&api, &store.lock().unwrap(), &progress)
                    .map_err(|e| e.to_string())?;
                Ok((stats, store))
            })()
        });

        self._task = Some(cx.spawn(async move |this, cx| {
            let mut tick = (0usize, 0usize);
            let mut line = String::new();
            let result = loop {
                match rx.recv().await {
                    Ok(msg) => {
                        if let Some((d, t)) = msg
                            .strip_prefix('@')
                            .and_then(|s| s.split_once('/'))
                            .and_then(|(d, t)| Some((d.parse().ok()?, t.parse().ok()?)))
                        {
                            tick = (d, t);
                        } else {
                            line = msg;
                        }
                        let _ = this.update(cx, |v, cx| {
                            v.sync_tick = tick;
                            v.sync_line = line.clone().into();
                            cx.notify();
                        });
                    }
                    Err(_) => break work.await,
                }
            };
            match result {
                Ok((stats, store)) => {
                    let _ = this.update(cx, |v, cx| {
                        v.store = Some(store);
                        v.sync = Some(SyncPhase::Done(stats));
                        v.attach_notes_tab(cx);
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |v, cx| {
                        v.sync_line = format!("同步失败：{e}").into();
                        cx.notify();
                    });
                }
            }
        }));
    }
}

/// 桌面 app_shell 的 ChannelProgress 同款拷贝（private 无法复用）。
struct ChannelProgress(Arc<Mutex<smol::channel::Sender<String>>>);

impl SyncProgress for ChannelProgress {
    fn note(&self, msg: String) {
        // ponytail: try_send, dropped lines only cost progress granularity
        let _ = self.0.lock().unwrap().try_send(msg);
    }
    fn tick(&self, done: usize, total: usize) {
        let _ = self.0.lock().unwrap().try_send(format!("@{done}/{total}"));
    }
}

impl Render for MobileApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let syncing = self.sync.is_some();
        match (&self.account, syncing) {
            (None, _) => self.login.clone().into_any_element(),
            (Some(_), true) => self.sync_screen(cx).into_any_element(),
            (Some(_), false) => self.shell.clone().into_any_element(),
        }
    }
}

// ── 底部 Tab 壳 ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TabId {
    #[default]
    Notes,
    Search,
    Settings,
}

impl TabId {
    fn label(self) -> &'static str {
        match self {
            TabId::Notes => t("tab_notes"),
            TabId::Search => t("tab_search"),
            TabId::Settings => t("tab_settings"),
        }
    }
}

#[derive(Default)]
pub struct TabShell {
    tab: TabId,
    /// 笔记 Tab（有 store 才存在；issue 03 的 note_count 占位已由真列表取代）。
    pub notes: Option<Entity<crate::notes_tab::NotesTab>>,
    /// 搜索 Tab（实体常驻，store 后挂；issue 06）。
    pub search: Option<Entity<crate::search_tab::SearchTab>>,
    /// 设置 Tab（issue 07）。
    pub settings: Option<Entity<crate::settings_tab::SettingsTab>>,
}

impl TabShell {
    fn placeholder(&self, title: &str, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                Label::new(title)
                    .text_size(rems(1.2))
                    .text_color(cx.theme().muted_foreground),
            )
    }
}

impl Render for TabShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe_bottom = window.insets().safe_area.bottom;
        let content = match self.tab {
            TabId::Notes => match &self.notes {
                Some(n) => n.clone().into_any_element(),
                None => self.placeholder(t("not_synced"), cx).into_any_element(),
            },
            TabId::Search => match &self.search {
                Some(s) => s.clone().into_any_element(),
                None => self.placeholder(t("not_synced"), cx).into_any_element(),
            },
            TabId::Settings => match &self.settings {
                Some(s) => s.clone().into_any_element(),
                None => self.placeholder(t("not_synced"), cx).into_any_element(),
            },
        };
        // iOS 惯例 Tab 栏：图标 + 小字号标签，选中着 primary、未选 muted，
        // 顶缘发丝线；不用整格底色块。高度内容驱动（固定 h 会把 pb 计入、
        // 内容区被压到 22px，图标越过上边线）。
        let bar = div()
            .flex()
            .w_full()
            .pb(safe_bottom)
            .bg(cx.theme().sidebar)
            .border_t_1()
            .border_color(cx.theme().border.opacity(0.6))
            .children([TabId::Notes, TabId::Search, TabId::Settings].map(|id| {
                let selected = self.tab == id;
                // accent 是浅蓝容器色（accent.background），选中 tint 用 primary。
                let color = if selected {
                    cx.theme().primary
                } else {
                    cx.theme().muted_foreground
                };
                let icon = match id {
                    TabId::Notes => IconName::FileText,
                    TabId::Search => IconName::Search,
                    TabId::Settings => IconName::Settings,
                };
                div()
                    .id(SharedString::new(format!("tab-{}", id.label())))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    // 固定 cell 高 + 垂直居中：图标/标签在可视栏内居中，
                    // 底部只留 safe area 背景（不留内容空白）。
                    .h(px(52.))
                    .justify_center()
                    .gap(px(2.))
                    .child(Icon::new(icon).size(px(22.)).text_color(color))
                    .child(
                        Label::new(id.label())
                            .text_size(rems(0.65))
                            .text_color(color),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.tab = id;
                        cx.notify();
                    }))
            }));

        // 阅读屏全屏沉浸：隐藏 Tab 栏，内容落到 safe bottom 之上。
        let chrome_hidden = self.tab == TabId::Notes
            && self
                .notes
                .as_ref()
                .map(|n| n.read(cx).is_reader())
                .unwrap_or(false);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            // min_h_0：flex item 的自动最小尺寸 = 内容 min-content，长列表会把
            // wrapper 撑到整列表高——Tab 栏被推出屏、滚动容器等于内容高（无溢出
            // 可滚）。三个 Tab 的滚动都路由经过这里，修一处。
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when(chrome_hidden, |el| el.pb(safe_bottom))
                    .child(content),
            )
            .when(!chrome_hidden, |el| el.child(bar))
    }
}
