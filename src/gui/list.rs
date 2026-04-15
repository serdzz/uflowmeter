#![allow(dead_code)]

use crate::gui::{UiEvent, Widget};

/// List widget — vertical navigation between child widgets.
/// Only one child is visible at a time. Up/Down cycles through enabled items.
/// Ported from C++ UI::List.
pub struct List<A, const N: usize> {
    items: [Option<A>; N],
    index: usize,
    count: usize,
    invalidate: bool,
}

impl<A: Clone + Default, const N: usize> Default for List<A, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Clone + Default, const N: usize> List<A, N> {
    pub fn new() -> Self {
        Self {
            items: [const { None }; N],
            index: 0,
            count: 0,
            invalidate: true,
        }
    }

    pub fn add(&mut self, item: A) -> bool {
        if self.count < N {
            self.items[self.count] = Some(item);
            self.count += 1;
            true
        } else {
            false
        }
    }

    pub fn selected_index(&self) -> usize {
        self.index % self.count
    }

    pub fn selected(&self) -> Option<&A> {
        if self.count == 0 {
            None
        } else {
            self.items[self.index % self.count].as_ref()
        }
    }

    pub fn selected_mut(&mut self) -> Option<&mut A> {
        if self.count == 0 {
            None
        } else {
            self.items[self.index % self.count].as_mut()
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    fn next(&mut self) {
        if self.count == 0 {
            return;
        }
        self.index = (self.index + 1) % self.count;
        self.invalidate = true;
    }

    fn prev(&mut self) {
        if self.count == 0 {
            return;
        }
        if self.index == 0 {
            self.index = self.count - 1;
        } else {
            self.index -= 1;
        }
        self.invalidate = true;
    }
}

impl<A: Clone + Default + core::fmt::Display, const N: usize> Widget<A, A> for List<A, N> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: A) {}

    fn render(&mut self, display: &mut impl core::fmt::Write) {
        if self.invalidate && self.count > 0 {
            if let Some(ref item) = self.items[self.index] {
                write!(display, "{}", item).ok();
            }
            self.invalidate = false;
        }
    }

    fn event(&mut self, e: UiEvent) -> Option<A> {
        match e {
            UiEvent::Up => {
                self.next();
                None
            }
            UiEvent::Down => {
                self.prev();
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_new() {
        let l: List<u8, 8> = List::new();
        assert_eq!(l.count(), 0);
    }

    #[test]
    fn test_list_add() {
        let mut l: List<u8, 8> = List::new();
        assert!(l.add(10));
        assert!(l.add(20));
        assert!(l.add(30));
        assert_eq!(l.count(), 3);
        assert_eq!(*l.selected().unwrap(), 10);
    }

    #[test]
    fn test_list_navigation() {
        let mut l: List<u8, 8> = List::new();
        l.add(10);
        l.add(20);
        l.add(30);
        let _ = l.event(UiEvent::Up);
        assert_eq!(l.selected_index(), 1);
        let _ = l.event(UiEvent::Up);
        assert_eq!(l.selected_index(), 2);
        let _ = l.event(UiEvent::Up);
        assert_eq!(l.selected_index(), 0); // wraps around
    }

    #[test]
    fn test_list_prev() {
        let mut l: List<u8, 8> = List::new();
        l.add(10);
        l.add(20);
        l.add(30);
        let _ = l.event(UiEvent::Down);
        assert_eq!(l.selected_index(), 2); // wraps backwards
    }

    #[test]
    fn test_list_overflow() {
        let mut l: List<u8, 3> = List::new();
        assert!(l.add(1));
        assert!(l.add(2));
        assert!(l.add(3));
        assert!(!l.add(4)); // overflow
    }
}
