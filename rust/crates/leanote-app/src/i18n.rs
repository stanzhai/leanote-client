//! zh/en interface strings, following the system language (Q12: only zh/en).



#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

/// `zh*` → Zh, everything else (incl. unknown) → En.
pub fn lang_from_locale(locale: &str) -> Lang {
    if locale.trim_start_matches(['_', '-']).to_ascii_lowercase().starts_with("zh") {
        Lang::Zh
    } else {
        Lang::En
    }
}

pub fn system_lang() -> Lang {
    sys_locale::get_locale().as_deref().map(lang_from_locale).unwrap_or(Lang::En)
}

macro_rules! keys {
    ($($key:ident => ($zh:expr, $en:expr)),* $(,)?) => {
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
    NavAllNotes => ("全部笔记", "All Notes"),
    NavStarred => ("星标", "Starred"),
    NavTrash => ("废纸篓", "Trash"),
    NavNotebooks => ("笔记本", "Notebooks"),
    NavTags => ("标签", "Tags"),
    // Shell: middle list + right pane.
    NotesListTitle => ("笔记", "Notes"),
    NoSelection => ("选择一条笔记", "Select a note"),
    EmptyNotes => ("没有笔记", "No notes yet"),
    NotesCount => ("条笔记", "notes"),
    NotebooksCount => ("个笔记本", "notebooks"),
    TagsCount => ("个标签", "tags"),
    Updated => ("更新于", "Updated"),
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
        assert_eq!(t(Lang::Zh, Key::NavAllNotes), "全部笔记");
        assert_eq!(t(Lang::En, Key::NavAllNotes), "All Notes");
    }

    // Exhaustive key list, kept next to the tables so a new key must be added here too.
    const ALL_KEYS: &[Key] = &[
        Key::AppName, Key::LoginTitle, Key::LoginSubtitle, Key::Host, Key::Email, Key::Password,
        Key::SignIn, Key::SigningIn, Key::Register, Key::LoginFailed, Key::SyncingTitle,
        Key::SyncingSubtitle, Key::SyncDone, Key::SyncFailed, Key::NavAllNotes, Key::NavStarred,
        Key::NavTrash, Key::NavNotebooks, Key::NavTags, Key::NotesListTitle, Key::NoSelection,
        Key::EmptyNotes, Key::NotesCount, Key::NotebooksCount, Key::TagsCount, Key::Updated,
    ];
}
