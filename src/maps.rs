//! Список карт для выбора. Пока размечена только Лаборатория.

pub struct MapInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub available: bool,
}

pub const MAPS: &[MapInfo] = &[
    MapInfo { id: "the-lab", name: "Лаборатория", available: true },
    MapInfo { id: "customs", name: "Таможня", available: false },
    MapInfo { id: "woods", name: "Лес", available: false },
    MapInfo { id: "shoreline", name: "Берег", available: false },
    MapInfo { id: "interchange", name: "Развязка", available: false },
    MapInfo { id: "reserve", name: "Резерв", available: false },
    MapInfo { id: "lighthouse", name: "Маяк", available: false },
    MapInfo { id: "streets-of-tarkov", name: "Улицы Таркова", available: false },
    MapInfo { id: "factory", name: "Завод", available: false },
    MapInfo { id: "ground-zero", name: "Эпицентр", available: false },
    MapInfo { id: "the-labyrinth", name: "Лабиринт", available: false },
    MapInfo { id: "icebreaker", name: "Ледокол", available: false },
];
