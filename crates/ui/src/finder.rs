use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Clone, Default)]
pub struct FuzzyFinder<T> {
    pub query: String,
    pub items: Vec<T>,
    pub filtered: Vec<(usize, u32)>,
    pub selected: usize,
    pub active: bool,
}

impl<T> FuzzyFinder<T> {
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.filtered.clear();
        self.selected = 0;
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn item_at_filtered_index(&self, index: usize) -> Option<&T> {
        let (item_idx, _) = self.filtered.get(index)?;
        self.items.get(*item_idx)
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.item_at_filtered_index(self.selected)
    }

    pub fn filtered_items(&self) -> impl Iterator<Item = &T> {
        self.filtered.iter().filter_map(|(index, _)| self.items.get(*index))
    }
}

impl<T: Clone> FuzzyFinder<T> {
    pub fn activate_with_items(&mut self, items: Vec<T>) {
        self.active = true;
        self.query.clear();
        self.selected = 0;
        self.items = items;
    }

    pub fn refresh<F>(&mut self, key: F, limit: usize)
    where
        F: Fn(&T) -> String,
    {
        self.filtered = fuzzy_match_items(&self.query, &self.items, limit, key);
        self.clamp_selection();
    }
}

pub fn fuzzy_match_items<T, F>(query: &str, candidates: &[T], limit: usize, key: F) -> Vec<(usize, u32)>
where
    F: Fn(&T) -> String,
{
    if query.trim().is_empty() {
        return candidates
            .iter()
            .enumerate()
            .take(limit)
            .map(|(idx, _)| (idx, u32::MAX))
            .collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut utf32_buf = Vec::new();

    let mut ranked = candidates
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let candidate = key(item);
            let score = pattern.score(Utf32Str::new(&candidate, &mut utf32_buf), &mut matcher)?;
            Some((idx, score))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.1.cmp(&left.1));
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::{FuzzyFinder, fuzzy_match_items};

    #[test]
    fn empty_query_returns_prefix_limited_items() {
        let items = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let results = fuzzy_match_items("", &items, 2, |value| value.clone());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 1);
    }

    #[test]
    fn refresh_orders_by_score_and_clamps_selection() {
        let mut finder = FuzzyFinder::default();
        finder.activate_with_items(vec!["src/main.rs".to_string(), "src/lib.rs".to_string()]);
        finder.selected = 10;
        finder.query = "main".to_string();
        finder.refresh(|value| value.clone(), 10);

        assert_eq!(finder.filtered.len(), 1);
        assert_eq!(finder.selected, 0);
        assert_eq!(finder.selected_item().map(String::as_str), Some("src/main.rs"));
    }
}
