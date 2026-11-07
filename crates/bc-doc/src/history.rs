//! Historia cofania (undo/redo).
//!
//! Każde polecenie z rejestru zapisuje jeden krok: etykietę (id polecenia lub
//! „batch”) i stan dokumentu sprzed wykonania (memento). Wsad poleceń zapisuje
//! jeden krok (REQ-01-038). Współdzielenie struktury dla dużych dokumentów
//! (100 tys. obiektów, sekcja 16) — planowane w M1.

use crate::Document;

#[derive(Debug, Clone)]
struct Entry {
    label: String,
    state: Document,
}

#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 200,
        }
    }
}

impl History {
    /// Zapisuje krok: `before` to stan sprzed zmiany.
    pub fn record(&mut self, label: &str, before: Document) {
        self.undo.push(Entry {
            label: label.to_string(),
            state: before,
        });
        self.redo.clear();
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Cofa ostatni krok; zwraca jego etykietę.
    pub fn undo(&mut self, current: &mut Document) -> Option<String> {
        let e = self.undo.pop()?;
        let after = std::mem::replace(current, e.state);
        self.redo.push(Entry {
            label: e.label.clone(),
            state: after,
        });
        Some(e.label)
    }

    /// Ponawia ostatnio cofnięty krok; zwraca jego etykietę.
    pub fn redo(&mut self, current: &mut Document) -> Option<String> {
        let e = self.redo.pop()?;
        let before = std::mem::replace(current, e.state);
        self.undo.push(Entry {
            label: e.label.clone(),
            state: before,
        });
        Some(e.label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Etykiety kroków do cofnięcia, od najstarszego (panel Historia cofania).
    pub fn undo_labels(&self) -> Vec<String> {
        self.undo.iter().map(|e| e.label.clone()).collect()
    }

    /// Etykiety kroków do ponowienia, od najbliższego.
    pub fn redo_labels(&self) -> Vec<String> {
        self.redo.iter().rev().map(|e| e.label.clone()).collect()
    }

    /// Stan odległy o `offset` kroków od bieżącego (−k = po k cofnięciach,
    /// +k = po k ponowieniach) z etykietą kroku, który do niego prowadzi.
    /// `None` dla 0 i dla kroków spoza historii.
    pub fn state(&self, offset: i64) -> Option<(&Document, &str)> {
        let k = offset.unsigned_abs() as usize;
        let list = match offset {
            0 => return None,
            o if o < 0 => &self.undo,
            _ => &self.redo,
        };
        let e = list.get(list.len().checked_sub(k)?)?;
        Some((&e.state, e.label.as_str()))
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
