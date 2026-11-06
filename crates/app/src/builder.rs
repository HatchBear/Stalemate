//! The manual builder's draft (FR-BUILD-003): sections, item search, reorder, ability order.
//!
//! Plain logic with no UI types, so it can be tested. Item names and IDs come from the item
//! catalog the core supplies from API-15 at runtime.

use crate::view_model::NamedOption;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub items: Vec<NamedOption>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuilderError {
    /// "No duplicate items within a build" (doc 08).
    DuplicateItem,
    NoSuchSection,
    NoHero,
    NoName,
    Empty,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuilderDraft {
    pub hero: Option<NamedOption>,
    pub name: String,
    pub sections: Vec<Section>,
    /// Optional, free text split on commas.
    pub ability_order: Vec<String>,
}

impl BuilderDraft {
    pub fn new() -> Self {
        let mut draft = Self::default();
        draft.sections.push(Section::default());
        draft
    }

    pub fn add_section(&mut self) {
        self.sections.push(Section::default());
    }

    pub fn rename_section(&mut self, section: usize, title: &str) -> Result<(), BuilderError> {
        let s = self
            .sections
            .get_mut(section)
            .ok_or(BuilderError::NoSuchSection)?;
        s.title = title.to_owned();
        Ok(())
    }

    pub fn contains(&self, item_id: u32) -> bool {
        self.sections
            .iter()
            .any(|s| s.items.iter().any(|i| i.id == item_id))
    }

    pub fn add_item(&mut self, section: usize, item: NamedOption) -> Result<(), BuilderError> {
        if self.contains(item.id) {
            return Err(BuilderError::DuplicateItem);
        }
        let s = self
            .sections
            .get_mut(section)
            .ok_or(BuilderError::NoSuchSection)?;
        s.items.push(item);
        Ok(())
    }

    pub fn remove_item(&mut self, section: usize, index: usize) {
        if let Some(s) = self.sections.get_mut(section) {
            if index < s.items.len() {
                s.items.remove(index);
            }
        }
    }

    /// Moves an item up (`delta < 0`) or down (`delta > 0`). Moving past the first or last item
    /// of a section carries it into the neighbouring section, so items can reorder across
    /// sections.
    pub fn move_item(&mut self, section: usize, index: usize, delta: i32) {
        let Some(s) = self.sections.get(section) else {
            return;
        };
        if index >= s.items.len() || delta == 0 {
            return;
        }
        if delta < 0 {
            if index > 0 {
                self.sections[section].items.swap(index, index - 1);
            } else if section > 0 {
                let item = self.sections[section].items.remove(0);
                self.sections[section - 1].items.push(item);
            }
        } else if index + 1 < s.items.len() {
            self.sections[section].items.swap(index, index + 1);
        } else if section + 1 < self.sections.len() {
            let item = self.sections[section]
                .items
                .pop()
                .expect("index is in range");
            self.sections[section + 1].items.insert(0, item);
        }
    }

    pub fn set_ability_order(&mut self, text: &str) {
        self.ability_order = text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
    }

    /// Checks the draft is ready to save.
    pub fn validate(&self) -> Result<(), BuilderError> {
        if self.hero.is_none() {
            return Err(BuilderError::NoHero);
        }
        if self.name.trim().is_empty() {
            return Err(BuilderError::NoName);
        }
        if self.sections.iter().all(|s| s.items.is_empty()) {
            return Err(BuilderError::Empty);
        }
        Ok(())
    }

    pub fn item_count(&self) -> u32 {
        self.sections.iter().map(|s| s.items.len() as u32).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32) -> NamedOption {
        NamedOption {
            id,
            name: format!("Item {id}"),
        }
    }

    fn ids(d: &BuilderDraft) -> Vec<Vec<u32>> {
        d.sections
            .iter()
            .map(|s| s.items.iter().map(|i| i.id).collect())
            .collect()
    }

    #[test]
    fn duplicates_are_rejected_across_sections() {
        let mut d = BuilderDraft::new();
        d.add_section();
        d.add_item(0, item(1)).unwrap();
        assert_eq!(d.add_item(1, item(1)), Err(BuilderError::DuplicateItem));
        assert_eq!(d.add_item(5, item(2)), Err(BuilderError::NoSuchSection));
    }

    #[test]
    fn reorder_within_and_across_sections() {
        let mut d = BuilderDraft::new();
        d.add_section();
        for i in 1..=3 {
            d.add_item(0, item(i)).unwrap();
        }
        d.add_item(1, item(9)).unwrap();
        d.move_item(0, 0, 1);
        assert_eq!(ids(&d), vec![vec![2, 1, 3], vec![9]]);
        d.move_item(0, 2, 1); // last item moves into the next section's front
        assert_eq!(ids(&d), vec![vec![2, 1], vec![3, 9]]);
        d.move_item(1, 0, -1); // and back
        assert_eq!(ids(&d), vec![vec![2, 1, 3], vec![9]]);
        d.move_item(0, 0, -1); // first of first section stays
        assert_eq!(ids(&d), vec![vec![2, 1, 3], vec![9]]);
        d.move_item(1, 0, 1); // last of last section stays
        assert_eq!(ids(&d), vec![vec![2, 1, 3], vec![9]]);
    }

    #[test]
    fn remove_and_ability_order() {
        let mut d = BuilderDraft::new();
        d.add_item(0, item(1)).unwrap();
        d.remove_item(0, 0);
        d.remove_item(0, 7);
        assert_eq!(d.item_count(), 0);
        d.set_ability_order(" Ability 1, ,Ability 2 ");
        assert_eq!(d.ability_order, vec!["Ability 1", "Ability 2"]);
    }

    #[test]
    fn validation_reports_the_first_problem() {
        let mut d = BuilderDraft::new();
        assert_eq!(d.validate(), Err(BuilderError::NoHero));
        d.hero = Some(NamedOption {
            id: 1,
            name: "Hero 1".into(),
        });
        assert_eq!(d.validate(), Err(BuilderError::NoName));
        d.name = "My build".into();
        assert_eq!(d.validate(), Err(BuilderError::Empty));
        d.add_item(0, item(1)).unwrap();
        assert_eq!(d.validate(), Ok(()));
    }
}
