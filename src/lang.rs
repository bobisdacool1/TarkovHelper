//! Язык игровых названий: предметы, задания и их цели, карты, выходы, торговцы, убежище —
//! как в клиенте игры (переводы из дампа tarkov.dev: `*_ru.json` / `*_en.json`).
//! Интерфейс самого приложения остаётся русским.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    /// Суффикс файлов переводов в дампе и значение в settings.json.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Self {
        if code == "en" { Lang::En } else { Lang::Ru }
    }

    /// Русский или английский вариант.
    pub fn pick(self, ru: &'static str, en: &'static str) -> &'static str {
        match self {
            Lang::Ru => ru,
            Lang::En => en,
        }
    }
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Текущий язык названий (выбран в настройках).
pub fn get() -> Lang {
    if ENGLISH.load(Ordering::Relaxed) { Lang::En } else { Lang::Ru }
}

pub fn set(lang: Lang) {
    ENGLISH.store(lang == Lang::En, Ordering::Relaxed);
}

/// Вариант названия на текущем языке.
pub fn pick(ru: &'static str, en: &'static str) -> &'static str {
    get().pick(ru, en)
}
