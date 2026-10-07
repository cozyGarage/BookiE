#[derive(Debug, Clone)]
pub struct RecentList<T> {
    newest_first: Vec<T>,
}

impl<T> Default for RecentList<T> {
    fn default() -> Self {
        Self {
            newest_first: Vec::new(),
        }
    }
}

impl<T: PartialEq + Clone> RecentList<T> {
    pub fn touch(&mut self, item: &T) {
        self.forget(item);
        self.newest_first.insert(0, item.clone());
    }

    pub fn forget(&mut self, item: &T) {
        self.newest_first.retain(|entry| entry != item);
    }

    pub fn previous(&self, current: &T) -> Option<T> {
        self.newest_first.iter().find(|entry| *entry != current).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(order: &[u8]) -> RecentList<u8> {
        let mut recent = RecentList::default();
        for item in order.iter().rev() {
            recent.touch(item);
        }
        recent
    }

    #[test]
    fn the_previous_tab_is_the_one_used_just_before_the_current() {
        let mut recent = RecentList::default();
        for tab in [1u8, 2, 3] {
            recent.touch(&tab);
        }
        assert_eq!(recent.previous(&3), Some(2));
        recent.touch(&1);
        assert_eq!(recent.previous(&1), Some(3));
    }

    #[test]
    fn switching_back_and_forth_alternates_between_two_tabs() {
        let mut recent = list(&[1, 2, 3]);
        let mut current = 1u8;
        for expected in [2u8, 1, 2, 1] {
            let next = recent.previous(&current).unwrap();
            assert_eq!(next, expected);
            recent.touch(&next);
            current = next;
        }
    }

    #[test]
    fn a_closed_tab_is_never_offered() {
        let mut recent = list(&[1, 2, 3]);
        recent.forget(&2);
        assert_eq!(recent.previous(&1), Some(3));
    }

    #[test]
    fn a_single_tab_has_no_previous() {
        let recent = list(&[7]);
        assert_eq!(recent.previous(&7), None);
        assert_eq!(RecentList::<u8>::default().previous(&1), None);
    }
}
