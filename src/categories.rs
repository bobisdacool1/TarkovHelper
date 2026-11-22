//! Подкатегории предметов для меню (по itemCategories из дампа tarkov.dev).

pub struct Group {
    pub name: &'static str,
    pub icon: &'static str,
}

/// Номера хранятся в settings.json и пресетах: новые категории — только в конец.
/// Порядок в меню — `menu_order`.
pub const GROUPS: &[Group] = &[
    Group { name: "Ключ-карты", icon: "💳" },
    Group { name: "Ключи", icon: "🔑" },
    Group { name: "Электроника", icon: "🔌" },
    Group { name: "Ценности", icon: "💎" },
    Group { name: "Информация", icon: "📁" },
    Group { name: "Стимуляторы", icon: "💉" },
    Group { name: "Медицина", icon: "💊" },
    Group { name: "Топливо", icon: "⛽" },
    Group { name: "Плакаты", icon: "🖼" },
    Group { name: "Прочее", icon: "📦" },
    Group { name: "Контейнеры", icon: "🧳" },
];

pub const OTHER: usize = 9;
/// Кейсы, ключницы, бумажники — предметы лута (не путать с местами обыска на карте).
pub const CASES: usize = 10;

/// Порядок категорий в меню: «Прочее» всегда последнее.
pub fn menu_order() -> impl Iterator<Item = usize> {
    (0..GROUPS.len()).filter(|&g| g != OTHER).chain([OTHER])
}

/// Группа по самой конкретной категории предмета (normalizedName).
pub fn group_of(category: Option<&str>) -> usize {
    match category.unwrap_or("") {
        "keycard" => 0,
        "mechanical-key" | "key" => 1,
        "electronics" | "battery" | "building-material" | "tool" => 2,
        "jewelry" => 3,
        "info" => 4,
        "stimulant" => 5,
        "medical-supplies" | "medikit" | "drug" | "meds" | "medical-item" => 6,
        "fuel" => 7,
        "flyer" | "poster" => 8,
        "common-container" => CASES,
        _ => OTHER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_before_other() {
        assert_eq!(group_of(Some("common-container")), CASES);
        assert_eq!(GROUPS[CASES].name, "Контейнеры");
        assert_eq!(GROUPS[OTHER].name, "Прочее");
        let order: Vec<usize> = menu_order().collect();
        assert_eq!(order.len(), GROUPS.len());
        assert_eq!(&order[order.len() - 2..], &[CASES, OTHER]);
    }
}
