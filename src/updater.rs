//! Обновление приложения из GitHub Releases (cargo-packager-updater).
//! Манифест latest.json и подписанные сборки выкладывает CI при пуше тега.

use anyhow::Result;
use cargo_packager_updater::{check_update, Config, Update};

pub const REPO: &str = "bobisdacool1/TarkovHelper";
const PUBKEY: &str = include_str!("../assets/updater.pub");

/// Есть ли новая версия. В debug-сборке не проверяем.
pub fn check() -> Result<Option<Update>> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let config = Config {
        endpoints: vec![format!("https://github.com/{REPO}/releases/latest/download/latest.json").parse()?],
        pubkey: PUBKEY.trim().to_string(),
        ..Default::default()
    };
    Ok(check_update(env!("CARGO_PKG_VERSION").parse()?, config)?)
}
