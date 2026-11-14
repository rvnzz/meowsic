use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Эквалайзер: восемь полос в децибелах и имя пресета.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct EqConfig {
    pub enabled: bool,
    pub preset: String,
    pub gains_db: Vec<f32>,
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            preset: "flat".to_string(),
            gains_db: vec![0.0; EQ_BANDS],
        }
    }
}

pub const EQ_BANDS: usize = 8;
pub const EQ_FREQUENCIES: [f32; EQ_BANDS] =
    [40.0, 100.0, 250.0, 600.0, 1500.0, 3500.0, 7000.0, 14000.0];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    Normal,
    High,
    Max,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::Low => "128 кбит/с",
            Quality::Normal => "192 кбит/с",
            Quality::High => "лучшее доступное",
            Quality::Max => "lossless",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub token: String,
    pub theme_dark: bool,
    pub accent: Option<String>,
    pub quality: Quality,
    pub volume: f32,
    pub restore_session: bool,
    pub autoplay_restored: bool,
    pub eq: EqConfig,
    pub lyrics_source: String,
    pub sidebar_covers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            token: String::new(),
            theme_dark: true,
            accent: None,
            quality: Quality::High,
            volume: 0.8,
            restore_session: true,
            autoplay_restored: false,
            eq: EqConfig::default(),
            lyrics_source: "lrclib".to_string(),
            sidebar_covers: true,
        }
    }
}

/// Точка останова после перезапуска: очередь, позиция, режимы.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SavedSession {
    pub queue: Vec<crate::yandex::TrackDto>,
    pub order: Vec<usize>,
    pub order_pos: usize,
    pub position_ms: u64,
    pub shuffle: bool,
    pub repeat: u8,
    pub wave_station: Option<String>,
    pub saved_at: u64,
}

pub struct Store {
    root: PathBuf,
    pub settings: Settings,
}

impl Store {
    pub fn open() -> Self {
        let root = config_root();
        let settings = read_json(&root.join("settings.json")).unwrap_or_default();
        Self { root, settings }
    }

    pub fn save_settings(&self, settings: &Settings) {
        write_json(&self.root.join("settings.json"), settings);
    }

    pub fn load_session(&self) -> Option<SavedSession> {
        let session: SavedSession = read_json(&self.root.join("session.json"))?;
        if session.queue.is_empty() {
            return None;
        }
        Some(session)
    }

    pub fn save_session(&self, session: &SavedSession) {
        write_json(&self.root.join("session.json"), session);
    }

    pub fn clear_caches(&self) {
        // Пока чистим только сохранённую сессию: картинный кеш живёт в GPUI.
        let _ = fs::remove_file(self.root.join("session.json"));
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn config_root() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("meowsic")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, text);
    }
}
