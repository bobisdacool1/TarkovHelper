//! Подкатегории предметов для меню (по itemCategories из дампа tarkov.dev).

pub struct Group {
    pub name: &'static str,
    pub icon: &'static str,
}

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
];

const OTHER: usize = 9;

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
        _ => OTHER,
    }
}
