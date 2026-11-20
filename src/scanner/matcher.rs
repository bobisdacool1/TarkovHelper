//! Сопоставление прочитанного ярлыка с предметами дампа: точно → по началу → похожее,
//! затем фильтр по размеру квадрата.

use std::collections::HashMap;

use crate::data::Item;

/// Ключ для сравнения: нижний регистр, только буквы и цифры, кириллица, похожая на латиницу, — латиницей
/// (OCR путает «Р» и «P», а английская игра пишет латиницей).
pub fn key(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c {
            'а' => 'a',
            'в' => 'b',
            'е' | 'ё' => 'e',
            'к' => 'k',
            'м' => 'm',
            'н' => 'h',
            'о' => 'o',
            'р' => 'p',
            'с' => 'c',
            'т' => 't',
            'у' => 'y',
            'х' => 'x',
            'ø' => '0', // «0.2BTC»: OCR читает перечёркнутый ноль
            c => c,
        })
        .collect()
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub struct Matcher<'a> {
    items: &'a HashMap<String, Item>,
    /// ключ короткого имени (ру и англ) → id предметов
    index: HashMap<String, Vec<&'a str>>,
    /// ключ полного имени (ру и англ) → id: подсказка игры при наведении
    names: HashMap<String, Vec<&'a str>>,
}

/// Что нашлось по ярлыку.
pub struct Found<'a> {
    pub item: &'a Item,
    /// Сколько ещё предметов подошло (одинаковые ярлыки у разных предметов).
    pub others: usize,
}

impl<'a> Matcher<'a> {
    pub fn new(items: &'a HashMap<String, Item>) -> Self {
        let mut index: HashMap<String, Vec<&'a str>> = HashMap::new();
        let mut names: HashMap<String, Vec<&'a str>> = HashMap::new();
        for it in items.values() {
            for (full, name) in [(false, &it.short), (false, &it.short_en), (true, &it.name), (true, &it.name_en)] {
                let map = if full { &mut names } else { &mut index };
                let k = key(name);
                if k.is_empty() {
                    continue;
                }
                let ids = map.entry(k).or_default();
                if !ids.contains(&it.id.as_str()) {
                    ids.push(it.id.as_str());
                }
            }
        }
        Self { items, index, names }
    }

    fn candidates(&self, q: &str) -> Vec<&'a str> {
        lookup(&self.index, q, true, 0.75)
    }

    /// Предмет по тексту ярлыка и размеру квадрата (`exact` — размеру можно верить).
    pub fn find(&self, text: &str, w: u32, h: u32, exact: bool) -> Option<Found<'a>> {
        let words: Vec<&str> = text.split_whitespace().collect();
        // весь текст, потом правые слова: левее ярлыка OCR иногда читает куски картинки
        let mut queries = vec![key(text)];
        if words.len() > 1 {
            queries.push(key(words[words.len() - 1]));
            queries.push(key(&words[words.len() - 2..].join("")));
        }
        for q in queries.iter().filter(|q| !q.is_empty()) {
            if let Some(f) = self.pick(self.candidates(q), w, h, exact) {
                return Some(f);
            }
        }
        None
    }

    /// Предмет по полному имени из подсказки игры. Имена длинные — опечатки OCR не страшны,
    /// но и совпадение требуем почти полное.
    pub fn find_name(&self, text: &str, w: u32, h: u32, exact: bool) -> Option<Found<'a>> {
        let q = key(text);
        if q.chars().count() < 4 {
            return None;
        }
        let cands = lookup(&self.names, &q, false, 0.8);
        self.pick(cands, w, h, exact)
    }

    /// Из кандидатов — подходящие по размеру, из них самый дорогой.
    fn pick(&self, cands: Vec<&'a str>, w: u32, h: u32, exact: bool) -> Option<Found<'a>> {
        if cands.is_empty() {
            return None;
        }
        let sized: Vec<&'a str> = cands
            .iter()
            .copied()
            .filter(|id| {
                let it = &self.items[*id];
                (it.width, it.height) == (w, h) || (it.width, it.height) == (h, w)
            })
            .collect();
        // размер не совпал ни у кого: оружие в сборе больше базового, или квадрат склеился неверно
        let pool = if sized.is_empty() || !exact { cands } else { sized };
        let best = pool.iter().map(|id| &self.items[*id]).max_by_key(|it| it.price())?;
        Some(Found { item: best, others: pool.len() - 1 })
    }
}

/// Поиск ключа: точно → по началу (`prefix`) → самое похожее с долей совпадения не ниже `min_ratio`.
fn lookup<'a>(index: &HashMap<String, Vec<&'a str>>, q: &str, prefix: bool, min_ratio: f32) -> Vec<&'a str> {
    if let Some(ids) = index.get(q) {
        return ids.clone();
    }
    let qc: Vec<char> = q.chars().collect();
    // игра обрезает длинные имена: «Augmenti»
    if prefix && qc.len() >= 4 {
        let mut v: Vec<&'a str> =
            index.iter().filter(|(k, _)| k.starts_with(q)).flat_map(|(_, ids)| ids.iter().copied()).collect();
        if !v.is_empty() {
            v.sort();
            v.dedup();
            return v;
        }
    }
    if qc.len() < 3 {
        return Vec::new();
    }
    // похожее: ошибки OCR в одной-двух буквах
    let mut best = (0.0f32, Vec::new());
    for (k, ids) in index {
        let kc: Vec<char> = k.chars().collect();
        if kc.len().abs_diff(qc.len()) > 4 {
            continue;
        }
        let ratio = 1.0 - levenshtein(&qc, &kc) as f32 / qc.len().max(kc.len()) as f32;
        if ratio > best.0 + 1e-6 {
            best = (ratio, ids.clone());
        } else if (ratio - best.0).abs() <= 1e-6 {
            best.1.extend(ids.iter().copied());
        }
    }
    if best.0 >= min_ratio { best.1 } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, short: &str, short_en: &str, w: u32, h: u32, price: i64) -> Item {
        Item {
            id: id.into(),
            name: short.into(),
            short: short.into(),
            short_en: short_en.into(),
            name_en: format!("{short_en} full"),
            flea: price,
            trader: 0,
            trader_name: "",
            group: 0,
            width: w,
            height: h,
            search: String::new(),
        }
    }

    fn items() -> HashMap<String, Item> {
        [
            item("btc", "0.2BTC", "0.2BTC", 1, 1, 900_000),
            item("aug", "Аугментин", "Augmentin", 1, 1, 20_000),
            item("svds", "СВДС", "SVDS", 5, 2, 50_000),
            item("svds_stock", "SVDS", "SVDS", 2, 1, 10_000),
            item("roler", "Ролекс", "Roler", 1, 1, 60_000),
        ]
        .into_iter()
        .map(|i| (i.id.clone(), i))
        .collect()
    }

    #[test]
    fn exact_prefix_similar_and_size() {
        let items = items();
        let m = Matcher::new(&items);
        assert_eq!(m.find("0.2BTC", 1, 1, true).unwrap().item.id, "btc");
        assert_eq!(m.find("Augmenti", 1, 1, true).unwrap().item.id, "aug"); // обрезанное имя
        assert_eq!(m.find("Ro1er", 1, 1, true).unwrap().item.id, "roler"); // ошибка OCR
        assert_eq!(m.find("SVDS", 2, 1, true).unwrap().item.id, "svds_stock"); // по размеру
        assert_eq!(m.find("SVDS", 5, 2, true).unwrap().item.id, "svds");
        assert_eq!(m.find("Аугментин", 1, 1, true).unwrap().item.id, "aug"); // русская игра
        assert_eq!(m.find("x Roler", 1, 1, true).unwrap().item.id, "roler"); // мусор слева
        assert!(m.find("FrAzenV2", 1, 1, true).is_none());
        // полное имя из подсказки игры
        assert_eq!(m.find_name("Roler full", 1, 1, true).unwrap().item.id, "roler");
        assert_eq!(m.find_name("Ro1er full", 1, 1, true).unwrap().item.id, "roler");
        assert!(m.find_name("Golden rooster figurine", 1, 1, true).is_none());
    }
}
