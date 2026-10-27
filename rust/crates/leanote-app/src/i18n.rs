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
    // Login form (right pane, spec §1/§4).
    LoginTitle => ("登录到 Leanote", "Sign in to Leanote"),
    LoginSubtitle => ("使用您的服务器账户", "Use your leanote server account"),
    Host => ("服务器", "Server"),
    Email => ("邮箱", "Email"),
    Password => ("密码", "Password"),
    SignIn => ("登录", "Sign In"),
    SigningIn => ("登录中…", "Signing in…"),
    SyncingContents => ("正在拉取笔记内容", "Downloading note contents"),
    LoginFailed => ("登录失败", "Login failed"),
    // Sync progress.
    SyncingTitle => ("正在全量同步", "Full sync in progress"),
    SyncingSubtitle => ("正在拉取您的笔记本、笔记与标签", "Pulling your notebooks, notes and tags"),
    SyncDone => ("同步完成", "Sync complete"),
    SyncSummaryStart => ("即刻开始", "Start now"),
    SyncSummaryNotebooks => ("笔记本", "Notebooks"),
    SyncSummaryNotes => ("笔记", "Notes"),
    SyncSummaryContents => ("笔记内容", "Note contents"),
    SyncSummaryTags => ("标签", "Tags"),
    SyncSummaryFailures => (
        "篇笔记内容拉取失败，已记录，下次同步自动重试",
        "note contents failed; recorded and retried next sync"
    ),
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
    // Title placeholder + empty-reader hint + empty-trash confirm.
    TitlePlaceholder => ("请输入笔记名称", "Enter note title"),
    EmptyReaderHint => (
        "还没有笔记，选择左侧笔记本开始创作",
        "No notes yet — pick a notebook to start writing"
    ),
    EmptyTrashConfirmBody => (
        "废纸篓中的所有笔记将被彻底删除，同步后不可恢复。",
        "All notes in trash will be permanently deleted. This cannot be undone after sync."
    ),
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
    DeleteNotebookTitle => ("删除笔记本？", "Delete notebook?"),
    DeleteNotebookEmptyBody => ("删除后不可恢复。", "This cannot be undone."),
    DeleteNotebookHasSubNotebooks => (
        "该笔记本下有子笔记本，请先删除子笔记本。",
        "This notebook has sub-notebooks. Delete them first."
    ),
    DeleteNotebookHasNotesBody => (
        "该笔记本下有 {n} 篇笔记，请先删除笔记。",
        "This notebook has {n} notes. Delete them first."
    ),
    MoveNotebookTo => ("移动到…", "Move to…"),
    NotebookNamePlaceholder => ("请输入笔记本名称", "Enter notebook name"),
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
    // Issue 21: row context-menu items.
    StarNote => ("星标", "Star"),
    UnstarNote => ("取消星标", "Unstar"),
    NotSynced => ("笔记尚未同步到服务器", "Note is not synced to the server"),
    // Review item 2: sync-now/logout controls.
    SyncNow => ("立即同步", "Sync now"),
    Logout => ("登出", "Log out"),
    PendingSync => ("有更新待同步", "Changes waiting to sync"),
    // Issue 17: ⌘K quick actions + recents.
    QuickActions => ("快捷操作", "Quick actions"),
    Recents => ("最近打开", "Recents"),
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
    AccountInfo => ("账户", "Account"),
    ServerLabel => ("服务器", "Server"),
    Attachments => ("附件", "Attachments"),
    AttachmentsEmpty => ("此笔记没有附件", "No attachments"),
    Download => ("下载", "Download"),
    DownloadFailed => ("下载失败", "Download failed"),
    MarkdownHelpTitle => ("Markdown 语法", "Markdown syntax"),
    AccentColor => ("强调色", "Accent color"),
    // Issue 23: settings + editor status bar.
    InterfaceZoom => ("界面缩放", "Interface zoom"),
    EditorToolbar => ("编辑器工具栏", "Editor toolbar"),
    VimMode => ("Vim 模式", "Vim mode"),
    AddTagPlaceholder => ("添加标签…", "Add tag…"),
    // UI round 10: settings re-grouped into a two-column dialog.
    SectionAppearance => ("外观", "Appearance"),
    SectionEditor => ("编辑器", "Editor"),
    SectionData => ("数据", "Data"),
    UIFont => ("界面字体", "UI font"),
    FontDefault => ("默认", "Default"),
    FontWenKai => ("霞鹜文楷", "LXGW WenKai"),
    // UI round 12: editor font size steps.
    EditorFontSize => ("编辑器字号", "Editor font size"),
    FontSizeRegular => ("常规", "Regular"),
    FontSizeMedium => ("中", "Medium"),
    FontSizeLarge => ("大", "Large"),
    FontDownloading => ("正在下载霞鹜文楷…", "Downloading LXGW WenKai…"),
    FontInstallFailed => ("下载失败，已用系统默认字体", "Download failed, using system default"),
    FontRestartHint => ("字体已安装，重启应用后生效", "Installed — restart to apply"),
    ExportBackupButton => ("导出备份…", "Export backup…"),
    // UI round 10: tooltips on previously bare icon buttons.
    TooltipSearch => ("搜索 (⌘K)", "Search (⌘K)"),
    TooltipSettings => ("设置 (⌘,)", "Settings (⌘,)"),
    TooltipNewNote => ("新建笔记", "New note"),
    TooltipEmptyTrash => ("清空垃圾篓", "Empty trash"),
    // Issue 17: logout confirmation.
    LogoutConfirmTitle => ("退出登录？", "Log out?"),
    LogoutConfirmBody => (
        "本地缓存将保留，重新登录后会重新同步。",
        "Local cache is kept; signing back in will re-sync."
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
        Key::LoginTitle,
        Key::LoginSubtitle,
        Key::Host,
        Key::Email,
        Key::Password,
        Key::SignIn,
        Key::SigningIn,
        Key::SyncingContents,
        Key::LoginFailed,
        Key::SyncingTitle,
        Key::SyncingSubtitle,
        Key::SyncDone,
        Key::SyncSummaryStart,
        Key::SyncSummaryNotebooks,
        Key::SyncSummaryNotes,
        Key::SyncSummaryContents,
        Key::SyncSummaryTags,
        Key::SyncSummaryFailures,
        Key::SyncFailed,
        Key::NavRecent,
        Key::NavStarred,
        Key::NavTrash,
        Key::NavNotebooks,
        Key::NavTags,
        Key::NoSelection,
        Key::EmptyNotes,
        Key::TitlePlaceholder,
        Key::EmptyReaderHint,
        Key::EmptyTrashConfirmBody,
        Key::Restore,
        Key::HardDelete,
        Key::EmptyTrash,
        Key::NewNote,
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
        Key::StarNote,
        Key::UnstarNote,
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
        Key::DeleteNotebookTitle,
        Key::DeleteNotebookEmptyBody,
        Key::DeleteNotebookHasSubNotebooks,
        Key::DeleteNotebookHasNotesBody,
        Key::MoveNotebookTo,
        Key::NotebookNamePlaceholder,
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
        Key::SyncNow,
        Key::Logout,
        Key::QuickActions,
        Key::Recents,
        Key::SettingsTitle,
        Key::Appearance,
        Key::AppearanceFollow,
        Key::AppearanceLight,
        Key::AppearanceDark,
        Key::LanguageSetting,
        Key::EditorPalette,
        Key::PaletteDefault,
        Key::PaletteWarm,
        Key::AccountInfo,
        Key::ServerLabel,
        Key::Attachments,
        Key::AttachmentsEmpty,
        Key::Download,
        Key::DownloadFailed,
        Key::MarkdownHelpTitle,
        Key::AccentColor,
        Key::InterfaceZoom,
        Key::EditorToolbar,
        Key::VimMode,
        Key::SectionAppearance,
        Key::SectionEditor,
        Key::SectionData,
        Key::UIFont,
        Key::FontDefault,
        Key::FontWenKai,
        Key::EditorFontSize,
        Key::FontSizeRegular,
        Key::FontSizeMedium,
        Key::FontSizeLarge,
        Key::FontDownloading,
        Key::FontInstallFailed,
        Key::FontRestartHint,
        Key::ExportBackupButton,
        Key::TooltipSearch,
        Key::TooltipSettings,
        Key::TooltipNewNote,
        Key::TooltipEmptyTrash,
        Key::AddTagPlaceholder,
        Key::PaletteSolarized,
        Key::LogoutConfirmTitle,
        Key::LogoutConfirmBody,
    ];
}
