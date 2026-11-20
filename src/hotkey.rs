//! Сочетания клавиш для сканера: запись из нажатия и отображение.
//! Формат хранения: "ctrl-shift-f9" — модификаторы в порядке ctrl, alt, shift, win, затем клавиша.

use gpui::Keystroke;

const MODIFIER_KEYS: &[&str] = &[
    "shift", "control", "ctrl", "alt", "option", "platform", "cmd", "super", "win", "fn", "function", "capslock",
];

/// Сочетание из нажатия. None — нажат только модификатор.
pub fn from_keystroke(k: &Keystroke) -> Option<String> {
    let key = k.key.to_lowercase();
    if key.is_empty() || MODIFIER_KEYS.contains(&key.as_str()) {
        return None;
    }
    let key = if key == "-" { "minus".to_string() } else { key };
    let m = &k.modifiers;
    let mut parts: Vec<&str> = Vec::new();
    if m.control {
        parts.push("ctrl");
    }
    if m.alt {
        parts.push("alt");
    }
    if m.shift {
        parts.push("shift");
    }
    if m.platform {
        parts.push("win");
    }
    let mut combo = parts.join("-");
    if !combo.is_empty() {
        combo.push('-');
    }
    combo.push_str(&key);
    Some(combo)
}

/// Клавиши сочетания для показа: ["Ctrl", "Shift", "F9"].
pub fn display(combo: &str) -> Vec<String> {
    combo
        .split('-')
        .filter(|p| !p.is_empty())
        .map(|p| match p.to_lowercase().as_str() {
            "ctrl" => "Ctrl".into(),
            "alt" => if cfg!(target_os = "macos") { "⌥ Option".into() } else { "Alt".into() },
            "shift" => "Shift".into(),
            "win" => if cfg!(target_os = "macos") { "⌘ Cmd".into() } else { "Win".into() },
            "minus" => "-".into(),
            "space" => "Пробел".into(),
            "enter" => "Enter".into(),
            "tab" => "Tab".into(),
            "backspace" => "Backspace".into(),
            "delete" => "Delete".into(),
            "insert" => "Insert".into(),
            "home" => "Home".into(),
            "end" => "End".into(),
            "pageup" => "PgUp".into(),
            "pagedown" => "PgDn".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            other => other.to_uppercase(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_combo_with_modifiers() {
        let k = Keystroke::parse("ctrl-shift-f9").unwrap();
        assert_eq!(from_keystroke(&k).as_deref(), Some("ctrl-shift-f9"));
        assert_eq!(display("ctrl-shift-f9"), vec!["Ctrl", "Shift", "F9"]);
    }

    #[test]
    fn plain_key_and_old_format() {
        let k = Keystroke::parse("q").unwrap();
        assert_eq!(from_keystroke(&k).as_deref(), Some("q"));
        assert_eq!(display("F9"), vec!["F9"]);
    }

    #[test]
    fn modifier_only_is_ignored() {
        let k = Keystroke::parse("shift").unwrap();
        assert_eq!(from_keystroke(&k), None);
    }
}
