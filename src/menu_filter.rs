//! Type-to-filter state for a selection menu, independent of the terminal.

/// One edit to the filter or the selection.
#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Type(char),
    Erase,
    Up,
    Down,
}

/// The typed query and the selected row among the visible labels. The last
/// `pinned` labels are always visible, after the matches.
pub struct Filter<'a> {
    labels: &'a [String],
    pinned: usize,
    query: String,
    selected: usize,
}

impl<'a> Filter<'a> {
    pub fn new(labels: &'a [String], pinned: usize) -> Self {
        Self {
            labels,
            pinned: pinned.min(labels.len()),
            query: String::new(),
            selected: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Positions in `labels` of every visible row, in display order.
    pub fn visible(&self) -> Vec<usize> {
        let split = self.labels.len() - self.pinned;
        let needle = self.query.to_lowercase();
        let matches =
            (0..split).filter(|&index| self.labels[index].to_lowercase().contains(&needle));
        matches.chain(split..self.labels.len()).collect()
    }

    /// The selected row's position among the visible rows.
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// The selected row's position in `labels`, if any row is visible.
    pub fn choice(&self) -> Option<usize> {
        self.visible().get(self.selected).copied()
    }

    pub fn apply(&mut self, edit: Edit) {
        let rows = self.visible().len().max(1);
        match edit {
            Edit::Type(character) => {
                self.query.push(character);
                self.selected = 0;
            }
            Edit::Erase => {
                self.query.pop();
                self.selected = 0;
            }
            Edit::Up => self.selected = self.selected.checked_sub(1).unwrap_or(rows - 1),
            Edit::Down => self.selected = (self.selected + 1) % rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> Vec<String> {
        ["Alpha-1", "beta-2", "ALPHA-3", "Enter manually"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn typing_narrows_by_case_insensitive_substring_and_keeps_pinned_rows() {
        let labels = labels();
        let mut filter = Filter::new(&labels, 1);
        assert_eq!(filter.visible(), vec![0, 1, 2, 3]);
        for character in "aLp".chars() {
            filter.apply(Edit::Type(character));
        }
        assert_eq!(filter.query(), "aLp");
        assert_eq!(filter.visible(), vec![0, 2, 3]);
        filter.apply(Edit::Type('z'));
        assert_eq!(filter.visible(), vec![3]);
        assert_eq!(filter.choice(), Some(3));
        filter.apply(Edit::Erase);
        assert_eq!(filter.visible(), vec![0, 2, 3]);
    }

    #[test]
    fn arrows_wrap_within_the_visible_rows_and_edits_reset_the_selection() {
        let labels = labels();
        let mut filter = Filter::new(&labels, 1);
        filter.apply(Edit::Type('l'));
        filter.apply(Edit::Up);
        assert_eq!(filter.selected(), 2);
        assert_eq!(filter.choice(), Some(3));
        filter.apply(Edit::Down);
        filter.apply(Edit::Down);
        assert_eq!(filter.choice(), Some(2));
        filter.apply(Edit::Erase);
        assert_eq!(filter.selected(), 0);
        filter.apply(Edit::Erase);
        assert_eq!(filter.query(), "");
    }

    #[test]
    fn an_empty_menu_has_no_choice() {
        let labels = Vec::new();
        let mut filter = Filter::new(&labels, 3);
        filter.apply(Edit::Down);
        filter.apply(Edit::Up);
        assert_eq!(filter.choice(), None);
    }
}
