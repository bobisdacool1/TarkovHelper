//! Сочетания клавиш для сканера: запись из нажатия и отображение.
//! Формат хранения: "ctrl-shift-f9" — модификаторы в порядке ctrl, alt, shift, win, затем клавиша.
//! Кнопки мыши: "mouse4", "mouse5", "mmb", "lmb", "rmb" (например "ctrl-mouse4").

use gpui::{Keystroke, Modifiers, MouseButton, NavigationDirection};

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
    Some(with_modifiers(&k.modifiers, &key))
}

/// Сочетание из клика мышью. ЛКМ и ПКМ — только с модификатором,
/// иначе сканер срабатывал бы на каждый выстрел.
pub fn from_mouse(button: MouseButton, m: &Modifiers) -> Result<String, &'static str> {
    let has_mod = m.control || m.alt || m.shift || m.platform;
    let name = match button {
        MouseButton::Navigate(NavigationDirection::Back) => "mouse4",
        MouseButton::Navigate(NavigationDirection::Forward) => "mouse5",
        MouseButton::Middle => "mmb",
        MouseButton::Left if has_mod => "lmb",
        MouseButton::Right if has_mod => "rmb",
        MouseButton::Left | MouseButton::Right => {
            return Err("ЛКМ и ПКМ — только вместе с Ctrl, Alt или Shift");
        }
    };
    Ok(with_modifiers(m, name))
}

/// Это сочетание с кнопкой мыши? Нужно сканеру: мышь ловим хуком мыши, клавиши — клавиатурным.
#[allow(dead_code)] // используется сканером под Windows
pub fn is_mouse(combo: &str) -> bool {
    matches!(combo.rsplit('-').next(), Some("mouse4" | "mouse5" | "mmb" | "lmb" | "rmb"))
}

fn with_modifiers(m: &Modifiers, key: &str) -> String {
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
    combo.push_str(key);
    combo
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
            "mouse4" => "Mouse 4".into(),
            "mouse5" => "Mouse 5".into(),
            "mmb" => "Колесо".into(),
            "lmb" => "ЛКМ".into(),
            "rmb" => "ПКМ".into(),
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
    fn mouse_buttons() {
        let none = Modifiers::default();
        let ctrl = Modifiers { control: true, ..Default::default() };
        assert_eq!(from_mouse(MouseButton::Navigate(NavigationDirection::Back), &none).as_deref(), Ok("mouse4"));
        assert_eq!(from_mouse(MouseButton::Navigate(NavigationDirection::Forward), &ctrl).as_deref(), Ok("ctrl-mouse5"));
        assert_eq!(from_mouse(MouseButton::Middle, &none).as_deref(), Ok("mmb"));
        assert!(from_mouse(MouseButton::Right, &none).is_err());
        assert_eq!(from_mouse(MouseButton::Right, &ctrl).as_deref(), Ok("ctrl-rmb"));
        assert_eq!(display("ctrl-mouse5"), vec!["Ctrl", "Mouse 5"]);
        assert!(is_mouse("shift-mouse4") && !is_mouse("f9"));
    }

    #[test]
    fn modifier_only_is_ignored() {
        let k = Keystroke::parse("shift").unwrap();
        assert_eq!(from_keystroke(&k), None);
    }
}
