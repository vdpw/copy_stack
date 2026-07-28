#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    English,
    SimplifiedChinese,
    TraditionalChinese,
}

impl Language {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
        }
    }

    fn from_locale_tag(locale: &str) -> Option<Self> {
        let normalized = locale.replace('_', "-").to_ascii_lowercase();
        let subtags = normalized.split('-').collect::<Vec<_>>();

        match subtags.first().copied() {
            Some("en") => Some(Self::English),
            Some("zh") => {
                if subtags.contains(&"hant") {
                    return Some(Self::TraditionalChinese);
                }
                if subtags.contains(&"hans") {
                    return Some(Self::SimplifiedChinese);
                }
                if subtags
                    .iter()
                    .any(|subtag| matches!(*subtag, "tw" | "hk" | "mo"))
                {
                    return Some(Self::TraditionalChinese);
                }

                Some(Self::SimplifiedChinese)
            }
            _ => None,
        }
    }

    pub(crate) fn detect_system() -> Self {
        sys_locale::get_locales()
            .find_map(|locale| Self::from_locale_tag(&locale))
            .unwrap_or(Self::English)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LanguagePreference {
    System,
    English,
    SimplifiedChinese,
    TraditionalChinese,
}

impl LanguagePreference {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => Language::English.code(),
            Self::SimplifiedChinese => Language::SimplifiedChinese.code(),
            Self::TraditionalChinese => Language::TraditionalChinese.code(),
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<Self> {
        match code {
            "system" => Some(Self::System),
            "en" => Some(Self::English),
            "zh-CN" => Some(Self::SimplifiedChinese),
            "zh-TW" => Some(Self::TraditionalChinese),
            _ => None,
        }
    }

    pub(crate) fn resolve(self) -> Language {
        match self {
            Self::System => Language::detect_system(),
            Self::English => Language::English,
            Self::SimplifiedChinese => Language::SimplifiedChinese,
            Self::TraditionalChinese => Language::TraditionalChinese,
        }
    }
}

pub(crate) struct NativeStrings {
    pub(crate) settings: &'static str,
    pub(crate) settings_ellipsis: &'static str,
    pub(crate) recent_clipboard_items: &'static str,
    pub(crate) no_clipboard_items: &'static str,
    pub(crate) open_history: &'static str,
    pub(crate) open_settings: &'static str,
    pub(crate) clear_history: &'static str,
    pub(crate) quit_copy_stack: &'static str,
    pub(crate) file: &'static str,
    pub(crate) folder: &'static str,
    pub(crate) files: &'static str,
    pub(crate) folders: &'static str,
    pub(crate) files_and_folders: &'static str,
    pub(crate) video: &'static str,
    pub(crate) edit: &'static str,
    pub(crate) view: &'static str,
    pub(crate) window: &'static str,
    pub(crate) help: &'static str,
    pub(crate) about_copy_stack: &'static str,
    pub(crate) services: &'static str,
    pub(crate) hide_copy_stack: &'static str,
    pub(crate) hide_others: &'static str,
    pub(crate) close_window: &'static str,
    pub(crate) undo: &'static str,
    pub(crate) redo: &'static str,
    pub(crate) cut: &'static str,
    pub(crate) copy: &'static str,
    pub(crate) paste: &'static str,
    pub(crate) select_all: &'static str,
    pub(crate) enter_full_screen: &'static str,
    pub(crate) minimize: &'static str,
    pub(crate) zoom: &'static str,
}

const ENGLISH_STRINGS: NativeStrings = NativeStrings {
    settings: "Settings",
    settings_ellipsis: "Settings…",
    recent_clipboard_items: "Recent clipboard items",
    no_clipboard_items: "No clipboard items yet",
    open_history: "Open history",
    open_settings: "Open settings",
    clear_history: "Clear history",
    quit_copy_stack: "Quit Copy Stack",
    file: "File",
    folder: "Folder",
    files: "Files",
    folders: "Folders",
    files_and_folders: "Files and folders",
    video: "Video",
    edit: "Edit",
    view: "View",
    window: "Window",
    help: "Help",
    about_copy_stack: "About Copy Stack",
    services: "Services",
    hide_copy_stack: "Hide Copy Stack",
    hide_others: "Hide Others",
    close_window: "Close Window",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    enter_full_screen: "Enter Full Screen",
    minimize: "Minimize",
    zoom: "Zoom",
};

const SIMPLIFIED_CHINESE_STRINGS: NativeStrings = NativeStrings {
    settings: "设置",
    settings_ellipsis: "设置…",
    recent_clipboard_items: "最近的剪贴板项目",
    no_clipboard_items: "暂无剪贴板项目",
    open_history: "打开历史记录",
    open_settings: "打开设置",
    clear_history: "清空历史记录",
    quit_copy_stack: "退出 Copy Stack",
    file: "文件",
    folder: "文件夹",
    files: "多个文件",
    folders: "多个文件夹",
    files_and_folders: "文件和文件夹",
    video: "视频",
    edit: "编辑",
    view: "显示",
    window: "窗口",
    help: "帮助",
    about_copy_stack: "关于 Copy Stack",
    services: "服务",
    hide_copy_stack: "隐藏 Copy Stack",
    hide_others: "隐藏其他",
    close_window: "关闭窗口",
    undo: "撤销",
    redo: "重做",
    cut: "剪切",
    copy: "复制",
    paste: "粘贴",
    select_all: "全选",
    enter_full_screen: "进入全屏幕",
    minimize: "最小化",
    zoom: "缩放",
};

const TRADITIONAL_CHINESE_STRINGS: NativeStrings = NativeStrings {
    settings: "設定",
    settings_ellipsis: "設定…",
    recent_clipboard_items: "最近的剪貼簿項目",
    no_clipboard_items: "尚無剪貼簿項目",
    open_history: "開啟歷史記錄",
    open_settings: "開啟設定",
    clear_history: "清除歷史記錄",
    quit_copy_stack: "結束 Copy Stack",
    file: "檔案",
    folder: "資料夾",
    files: "多個檔案",
    folders: "多個資料夾",
    files_and_folders: "檔案和資料夾",
    video: "影片",
    edit: "編輯",
    view: "顯示",
    window: "視窗",
    help: "輔助說明",
    about_copy_stack: "關於 Copy Stack",
    services: "服務",
    hide_copy_stack: "隱藏 Copy Stack",
    hide_others: "隱藏其他",
    close_window: "關閉視窗",
    undo: "還原",
    redo: "重做",
    cut: "剪下",
    copy: "複製",
    paste: "貼上",
    select_all: "全選",
    enter_full_screen: "進入全螢幕",
    minimize: "縮到最小",
    zoom: "縮放",
};

pub(crate) const fn native_strings(language: Language) -> &'static NativeStrings {
    match language {
        Language::English => &ENGLISH_STRINGS,
        Language::SimplifiedChinese => &SIMPLIFIED_CHINESE_STRINGS,
        Language::TraditionalChinese => &TRADITIONAL_CHINESE_STRINGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_resolve_to_supported_chinese_variants() {
        for locale in ["zh", "zh-CN", "zh-SG", "zh-Hans", "zh_Hans_CN"] {
            assert_eq!(
                Language::from_locale_tag(locale),
                Some(Language::SimplifiedChinese),
                "{locale}"
            );
        }

        for locale in ["zh-TW", "zh-HK", "zh-MO", "zh-Hant", "zh_Hant_HK"] {
            assert_eq!(
                Language::from_locale_tag(locale),
                Some(Language::TraditionalChinese),
                "{locale}"
            );
        }
    }

    #[test]
    fn english_and_unsupported_locale_tags_are_distinguished() {
        assert_eq!(Language::from_locale_tag("en-US"), Some(Language::English));
        assert_eq!(Language::from_locale_tag("fr-FR"), None);
    }

    #[test]
    fn language_preferences_parse_and_resolve_manual_overrides() {
        assert_eq!(
            LanguagePreference::from_code("zh-CN")
                .expect("preference should parse")
                .resolve(),
            Language::SimplifiedChinese
        );
        assert_eq!(
            LanguagePreference::from_code("zh-TW")
                .expect("preference should parse")
                .resolve(),
            Language::TraditionalChinese
        );
        assert!(LanguagePreference::from_code("fr").is_none());
    }

    #[test]
    fn every_native_catalog_has_localized_settings_text() {
        assert_eq!(native_strings(Language::English).settings, "Settings");
        assert_eq!(native_strings(Language::SimplifiedChinese).settings, "设置");
        assert_eq!(
            native_strings(Language::TraditionalChinese).settings,
            "設定"
        );
    }
}
