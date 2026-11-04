//! 移动端 UI 字符串（中/英，issue 07）。桌面 i18n.rs 在 leanote-app
//! （用户 WIP 中），移动端字符串集小，先本地维护。
#![allow(dead_code)]

static LANG: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // 0=zh 1=en

pub fn set_lang(id: &str) {
    let v = if id == "en" { 1 } else { 0 };
    LANG.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn lang() -> &'static str {
    if LANG.load(std::sync::atomic::Ordering::Relaxed) == 1 { "en" } else { "zh" }
}

pub fn t(key: &str) -> &'static str {
    t_for(lang(), key)
}

fn t_for(lang: &str, key: &str) -> &'static str {
    match (lang, key) {
        ("zh", "tab_notes") => "笔记",
        ("zh", "tab_search") => "搜索",
        ("zh", "tab_settings") => "设置",
        ("zh", "recent") => "最近",
        ("zh", "browse_notebooks") => "浏览笔记本",
        ("zh", "notebooks") => "笔记本",
        ("zh", "tags") => "标签",
        ("zh", "untitled") => "(无标题)",
        ("zh", "back") => "‹ 返回",
        ("zh", "notes_unit") => "篇",
        ("zh", "login_title") => "登录 leanote",
        ("zh", "host") => "服务器",
        ("zh", "email") => "邮箱",
        ("zh", "password") => "密码",
        ("zh", "sign_in") => "登录",
        ("zh", "signing_in") => "登录中…",
        ("zh", "syncing") => "正在同步…",
        ("zh", "sync_done") => "同步完成",
        ("zh", "start") => "开始使用",
        ("zh", "sync_title") => "同步",
        ("zh", "note_content") => "笔记内容",
        ("zh", "search_placeholder") => "搜索笔记…",
        ("zh", "search_empty") => "输入关键词搜索",
        ("zh", "not_synced") => "未同步",
        ("zh", "appearance") => "外观",
        ("zh", "language") => "语言",
        ("zh", "zoom") => "界面缩放",
        ("zh", "theme_light") => "浅色主题",
        ("zh", "theme_dark") => "深色主题",
        ("zh", "account") => "账户",
        ("zh", "sync_now") => "立即同步",
        ("zh", "syncing_now") => "同步中…",
        ("zh", "logout") => "登出",
        ("zh", "note_unavailable") => "（笔记不可用）",
        ("en", "tab_notes") => "Notes",
        ("en", "tab_search") => "Search",
        ("en", "tab_settings") => "Settings",
        ("en", "recent") => "Recent",
        ("en", "browse_notebooks") => "Browse Notebooks",
        ("en", "notebooks") => "Notebooks",
        ("en", "tags") => "Tags",
        ("en", "untitled") => "(Untitled)",
        ("en", "back") => "‹ Back",
        ("en", "notes_unit") => "notes",
        ("en", "login_title") => "Sign in to leanote",
        ("en", "host") => "Server",
        ("en", "email") => "Email",
        ("en", "password") => "Password",
        ("en", "sign_in") => "Sign In",
        ("en", "signing_in") => "Signing in…",
        ("en", "syncing") => "Syncing…",
        ("en", "sync_done") => "Sync complete",
        ("en", "start") => "Get Started",
        ("en", "sync_title") => "Sync",
        ("en", "note_content") => "note contents",
        ("en", "search_placeholder") => "Search notes…",
        ("en", "search_empty") => "Type a keyword to search",
        ("en", "not_synced") => "Not synced",
        ("en", "appearance") => "Appearance",
        ("en", "language") => "Language",
        ("en", "zoom") => "UI Zoom",
        ("en", "theme_light") => "Light Theme",
        ("en", "theme_dark") => "Dark Theme",
        ("en", "account") => "Account",
        ("en", "sync_now") => "Sync Now",
        ("en", "syncing_now") => "Syncing…",
        ("en", "logout") => "Sign Out",
        ("en", "note_unavailable") => "(Note unavailable)",
        _ => "",
    }
}
