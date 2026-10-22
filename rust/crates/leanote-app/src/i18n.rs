//! zh/en interface strings, following the system language (Q12: only zh/en).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

/// `zh*` → Zh, everything else (incl. unknown) → En.
pub fn lang_from_locale(locale: &str) -> Lang {
    if locale
        .trim_start_matches(['_', '-'])
        .to_ascii_lowercase()
        .starts_with("zh")
    {
        Lang::Zh
    } else {
        Lang::En
    }
}

pub fn system_lang() -> Lang {
    sys_locale::get_locale()
        .as_deref()
        .map(lang_from_locale)
        .unwrap_or(Lang::En)
}

macro_rules! keys {
    ($($key:ident => ($zh:literal, $en:literal)),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Key { $($key),* }

        pub fn t(lang: Lang, key: Key) -> &'static str {
            match key { $(Key::$key => match lang { Lang::Zh => $zh, Lang::En => $en }),* }
        }
    };
}

keys! {
    AppName => ("Leanote", "Leanote"),
    // Login form (right pane, spec §1/§4).
    LoginTitle => ("登录到 Leanote", "Sign in to Leanote"),
    LoginSubtitle => ("使用您的服务器账户", "Use your leanote server account"),
    Host => ("服务器", "Server"),
    Email => ("邮箱", "Email"),
    Password => ("密码", "Password"),
    SignIn => ("登录", "Sign In"),
    SigningIn => ("登录中…", "Signing in…"),
    Register => ("注册 leanote.com 账户", "Create a leanote.com account"),
    LoginFailed => ("登录失败", "Login failed"),
    // Sync progress.
    SyncingTitle => ("正在全量同步", "Full sync in progress"),
    SyncingSubtitle => ("正在拉取您的笔记本、笔记与标签", "Pulling your notebooks, notes and tags"),
    SyncDone => ("同步完成", "Sync complete"),
    SyncFailed => ("同步失败", "Sync failed"),
    // Shell: left nav (ADR-0003).
    NavRecent => ("最近", "Recent"),
    NavStarred => ("星标", "Starred"),
    NavTrash => ("废纸篓", "Trash"),
    NavNotebooks => ("笔记本", "Notebooks"),
    NavTags => ("标签", "Tags"),
    // Shell: middle list + right pane.
    NoSelection => ("选择一条笔记", "Select a note"),
    EmptyNotes => ("没有笔记", "No notes yet"),
    // Ticket 09: reader toolbar + trash actions.
    Restore => ("恢复", "Restore"),
    HardDelete => ("彻底删除", "Delete forever"),
    EmptyTrash => ("清空废纸篓", "Empty trash"),
    // Ticket 13: ⌘K search overlay.
    SearchPlaceholder => ("搜索笔记…", "Search notes…"),
    SearchNoResults => ("没有匹配的笔记", "No matching notes"),
    SearchEmptyHint => ("输入关键词搜索标题、内容与标签", "Type to search titles, content and tags"),
    SearchFootnote => ("↑↓ 选择   ↩ 打开   esc 关闭", "↑↓ navigate   ↩ open   esc close"),
    // Ticket 10: editor toolbar + history.
    NewNote => ("新建笔记", "New note"),
    Done => ("完成", "Done"),
    EditorPreview => ("预览", "Preview"),
    EditorSource => ("源码", "Source"),
    History => ("历史版本", "History"),
    RestoreVersion => ("恢复此版本", "Restore this version"),
    HistoryEmpty => ("暂无历史版本", "No history yet"),
    Saved => ("已保存", "Saved"),
    // Ticket 12: rich-text notes render read-only as converted Markdown.
    RichTextReadonly => ("富文本（只读）", "Rich text (read only)"),
    ConvertToMarkdown => ("转换为 Markdown", "Convert to Markdown"),
    ConvertedToMarkdown => ("已转换为 Markdown", "Converted to Markdown"),
    // Ticket 14: conflict adjudication bar.
    ConflictTitle => ("检测到同步冲突", "Sync conflict detected"),
    ConflictLocal => ("本地版本", "Local version"),
    ConflictServer => ("服务器版本", "Server version"),
    UseLocal => ("用本地", "Use local"),
    UseServer => ("用服务器", "Use server"),
    KeepBoth => ("两边保留", "Keep both"),
    // Ticket 11: nav context actions.
    NewNotebook => ("新建笔记本", "New Notebook"),
    RenameNotebook => ("重命名笔记本", "Rename Notebook"),
    DeleteNotebook => ("删除笔记本", "Delete Notebook"),
    MoveNotebookTo => ("移动到…", "Move to…"),
    RootLevel => ("顶层", "Top level"),
    RenameTag => ("重命名标签", "Rename Tag"),
    DeleteTag => ("删除标签", "Delete Tag"),
    // Issue 18: note-row context menu.
    MoveNoteTo => ("移动笔记到…", "Move note to…"),
    DeleteNote => ("删除笔记", "Delete note"),
    Confirm => ("确定", "OK"),
    Cancel => ("取消", "Cancel"),
    // Ticket 15: export actions + PDF-fallback reason.
    ExportHtml => ("导出 HTML", "Export HTML"),
    ExportMarkdown => ("导出 Markdown", "Export Markdown"),
    ExportPdf => ("导出 PDF", "Export PDF"),
    PdfFellBack => ("服务器导出 PDF 失败，已导出 HTML", "Server PDF export failed, exported HTML instead"),
    NotSynced => ("笔记尚未同步到服务器", "Note is not synced to the server"),
    // Ticket 16 tray menu.
    TrayShow => ("显示", "Show"),
    TrayQuit => ("退出", "Quit"),
    // Review item 2: sync-now/logout controls.
    SyncNow => ("立即同步", "Sync now"),
    Logout => ("登出", "Log out"),
    // Issue 17: ⌘K quick actions + recents.
    QuickActions => ("快捷操作", "Quick actions"),
    Recents => ("最近打开", "Recents"),
    ActionExportBackup => ("导出备份", "Export backup"),
    // Issue 17: settings dialog.
    SettingsTitle => ("设置", "Settings"),
    Appearance => ("外观", "Appearance"),
    AppearanceFollow => ("跟随系统", "Follow system"),
    AppearanceLight => ("浅色", "Light"),
    AppearanceDark => ("深色", "Dark"),
    LanguageSetting => ("语言", "Language"),
    EditorPalette => ("编辑器配色", "Editor palette"),
    PaletteDefault => ("默认", "Default"),
    PaletteWarm => ("暖色", "Warm"),
    PaletteSolarized => ("Solarized", "Solarized"),
    VimOn => ("Vim", "Vim"),
    AccountInfo => ("账户", "Account"),
    ServerLabel => ("服务器", "Server"),
    SearchNotesAction => ("搜索笔记", "Search notes"),
    Attachments => ("附件", "Attachments"),
    AttachmentsEmpty => ("此笔记没有附件", "No attachments"),
    Download => ("下载", "Download"),
    DownloadFailed => ("下载失败", "Download failed"),
    MarkdownHelpTitle => ("Markdown 语法", "Markdown syntax"),
    Fullscreen => ("全屏", "Fullscreen"),
    AccentColor => ("强调色", "Accent color"),
    // MarkdownHelpBody: see `markdown_help_body` (multi-line raw strings).
    // Issue 17: logout confirmation.
    LogoutConfirmTitle => ("退出登录？", "Log out?"),
    LogoutConfirmBody => (
        "退出后将清除本地数据，需要重新登录。",
        "Local data is cleared; you will need to sign in again."
    ),
}

/// Issue 19: Markdown cheat-sheet (multi-line raw strings — kept out of the
/// single-line `keys!` table).
pub fn markdown_help_body(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => {
            r"# 标题 1  …  ###### 标题 6
**粗体**  *斜体*  ~~删除线~~
`行内代码` 与 ``` 代码块
- 列表 / 1. 有序列表
> 引用
[链接](url)  ![图片](url)
--- 分隔线"
        }
        Lang::En => {
            r"# Heading 1  …  ###### Heading 6
**bold**  *italic*  ~~strike~~
`inline code` and ``` fenced block
- list / 1. ordered
> quote
[link](url)  ![image](url)
--- rule"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection() {
        assert_eq!(lang_from_locale("zh_CN"), Lang::Zh);
        assert_eq!(lang_from_locale("zh-Hans-CN"), Lang::Zh);
        assert_eq!(lang_from_locale("zh_TW"), Lang::Zh);
        assert_eq!(lang_from_locale("ZH"), Lang::Zh);
        assert_eq!(lang_from_locale("en_US"), Lang::En);
        assert_eq!(lang_from_locale("de_DE"), Lang::En);
        assert_eq!(lang_from_locale(""), Lang::En);
        assert_eq!(lang_from_locale("xx"), Lang::En);
        // zh buried in the middle must NOT match (leading separator stripped only).
        assert_eq!(lang_from_locale("en-zh"), Lang::En);
    }

    #[test]
    fn every_key_has_both_langs() {
        // Every key resolves to a non-empty string in both languages.
        for key in ALL_KEYS {
            assert!(!t(Lang::Zh, *key).is_empty(), "{key:?} zh empty");
            assert!(!t(Lang::En, *key).is_empty(), "{key:?} en empty");
        }
        // Spot-check a few so a table swap cannot silently pass.
        assert_eq!(t(Lang::Zh, Key::NavRecent), "最近");
        assert_eq!(t(Lang::En, Key::NavRecent), "Recent");
    }

    // Exhaustive key list, kept next to the tables so a new key must be added here too.
    const ALL_KEYS: &[Key] = &[
        Key::AppName,
        Key::LoginTitle,
        Key::LoginSubtitle,
        Key::Host,
        Key::Email,
        Key::Password,
        Key::SignIn,
        Key::SigningIn,
        Key::Register,
        Key::LoginFailed,
        Key::SyncingTitle,
        Key::SyncingSubtitle,
        Key::SyncDone,
        Key::SyncFailed,
        Key::NavRecent,
        Key::NavStarred,
        Key::NavTrash,
        Key::NavNotebooks,
        Key::NavTags,
        Key::NoSelection,
        Key::EmptyNotes,
        Key::Restore,
        Key::HardDelete,
        Key::EmptyTrash,
        Key::NewNote,
        Key::Done,
        Key::EditorPreview,
        Key::EditorSource,
        Key::History,
        Key::RestoreVersion,
        Key::HistoryEmpty,
        Key::Saved,
        Key::RichTextReadonly,
        Key::ConvertToMarkdown,
        Key::ConvertedToMarkdown,
        Key::ExportHtml,
        Key::ExportMarkdown,
        Key::ExportPdf,
        Key::PdfFellBack,
        Key::NotSynced,
        Key::ConflictTitle,
        Key::ConflictLocal,
        Key::ConflictServer,
        Key::UseLocal,
        Key::UseServer,
        Key::KeepBoth,
        Key::NewNotebook,
        Key::RenameNotebook,
        Key::DeleteNotebook,
        Key::MoveNotebookTo,
        Key::RootLevel,
        Key::RenameTag,
        Key::DeleteTag,
        Key::MoveNoteTo,
        Key::DeleteNote,
        Key::Confirm,
        Key::Cancel,
        Key::SearchPlaceholder,
        Key::SearchNoResults,
        Key::SearchEmptyHint,
        Key::SearchFootnote,
        Key::TrayShow,
        Key::TrayQuit,
        Key::SyncNow,
        Key::Logout,
        Key::QuickActions,
        Key::Recents,
        Key::ActionExportBackup,
        Key::SettingsTitle,
        Key::Appearance,
        Key::AppearanceFollow,
        Key::AppearanceLight,
        Key::AppearanceDark,
        Key::LanguageSetting,
        Key::EditorPalette,
        Key::PaletteDefault,
        Key::PaletteWarm,
        Key::VimOn,
        Key::AccountInfo,
        Key::ServerLabel,
        Key::SearchNotesAction,
        Key::Attachments,
        Key::AttachmentsEmpty,
        Key::Download,
        Key::DownloadFailed,
        Key::MarkdownHelpTitle,
        Key::Fullscreen,
        Key::AccentColor,
        Key::LogoutConfirmTitle,
        Key::LogoutConfirmBody,
    ];
}
