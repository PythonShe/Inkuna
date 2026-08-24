//! System-font registration records: how a shell describes its
//! platform serif/sans faces to `Bookshelf::register_system_fonts`,
//! and the per-face warnings it gets back for skipped ones.

/// Which reading role a registered system face serves. The engine's
/// fallback stages (CJK/Hebrew/Symbols) always stay the bundled Notos —
/// system faces only replace the Reading stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SystemFontRole {
    Serif,
    Sans,
}

/// One platform face. iOS resolves a CTFont's file URL + PostScript
/// name; Android uses `SystemFonts` (file path + ttcIndex).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SystemFontFace {
    pub role: SystemFontRole,
    pub italic: bool,
    /// Declared CSS weight of a STATIC face (clamped to 1..=1000).
    /// Variable faces ignore it: the core instances the nine standard
    /// weights from their `wght` axis instead.
    pub weight: u16,
    pub file_path: String,
    /// When given, the core scans the file's collection indices for the
    /// face with this PostScript name; otherwise `ttc_hint` (default 0)
    /// picks it.
    pub post_script_name: Option<String>,
    pub ttc_hint: Option<u32>,
}

/// One skipped face: which file and why, for shell-side logging. The
/// registry stays fully usable — the affected role silently falls back
/// to the bundled Noto equivalent at selection time.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SystemFontWarning {
    pub file_path: String,
    pub detail: String,
}

impl From<SystemFontFace> for inkuna_core::SystemFontFace {
    fn from(f: SystemFontFace) -> Self {
        inkuna_core::SystemFontFace {
            role: match f.role {
                SystemFontRole::Serif => inkuna_core::SystemFontRole::Serif,
                SystemFontRole::Sans => inkuna_core::SystemFontRole::Sans,
            },
            italic: f.italic,
            weight: f.weight,
            file_path: f.file_path,
            post_script_name: f.post_script_name,
            ttc_hint: f.ttc_hint,
        }
    }
}

impl From<inkuna_core::SystemFontWarning> for SystemFontWarning {
    fn from(w: inkuna_core::SystemFontWarning) -> Self {
        SystemFontWarning {
            file_path: w.file_path,
            detail: w.detail,
        }
    }
}
