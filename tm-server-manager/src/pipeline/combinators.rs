//! The building blocks of the leaderboard steps and modes.
//! Everything keeps the order of the entries so the results never depend on hashing.

use std::{collections::HashMap, hash::Hash};

use crate::pipeline::LbEntry;

/// An entry which has a place in a leaderboard.
pub(crate) trait LbRow: Copy {
    fn set_position(&mut self, position: u16);
}

impl LbRow for LbEntry {
    fn set_position(&mut self, position: u16) {
        self.position = position;
    }
}

/// Entries grouped by a key, the groups are in the order their keys first appeared and are never empty.
pub(crate) struct Groups<K, E>(Vec<(K, Vec<E>)>);

impl<K: Hash + Eq + Copy, E> Groups<K, E> {
    fn new(entries: impl IntoIterator<Item = E>, key: impl Fn(&E) -> K) -> Self {
        let mut indices: HashMap<K, usize> = HashMap::new();
        let mut groups: Vec<(K, Vec<E>)> = Vec::new();
        for entry in entries {
            let key = key(&entry);
            match indices.get(&key) {
                Some(&index) => groups[index].1.push(entry),
                None => {
                    indices.insert(key, groups.len());
                    groups.push((key, vec![entry]));
                }
            }
        }
        Groups(groups)
    }
}

impl<K: Copy, E> Groups<K, E> {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// The size of the biggest group.
    pub(crate) fn max_len(&self) -> Option<usize> {
        self.0.iter().map(|(_, entries)| entries.len()).max()
    }

    pub(crate) fn map(self, mut f: impl FnMut(K, Vec<E>) -> Vec<E>) -> Self {
        Groups(
            self.0
                .into_iter()
                .map(|(key, entries)| {
                    let entries = f(key, entries);
                    (key, entries)
                })
                .collect(),
        )
    }

    /// Like [`Groups::map`] but stops at the first error, the groups may become empty.
    pub(crate) fn try_map<Err>(
        self,
        mut f: impl FnMut(K, Vec<E>) -> Result<Vec<E>, Err>,
    ) -> Result<Self, Err> {
        let groups = self
            .0
            .into_iter()
            .map(|(key, entries)| Ok((key, f(key, entries)?)))
            .collect::<Result<_, _>>()?;
        Ok(Groups(groups))
    }

    /// Turns every group into a single entry.
    pub(crate) fn fold(self, mut f: impl FnMut(K, Vec<E>) -> E) -> Vec<E> {
        self.0
            .into_iter()
            .map(|(key, entries)| f(key, entries))
            .collect()
    }

    /// Combines the entries of every group into its first one.
    pub(crate) fn reduce(self, mut f: impl FnMut(E, E) -> E) -> Vec<E> {
        self.0
            .into_iter()
            .filter_map(|(_, entries)| entries.into_iter().reduce(&mut f))
            .collect()
    }

    pub(crate) fn flatten(self) -> Vec<E> {
        self.0
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect()
    }
}

pub(crate) trait LbRows<E>: Sized {
    fn group_by<K: Hash + Eq + Copy>(self, key: impl Fn(&E) -> K) -> Groups<K, E>;
    /// Sorts the entries with the lowest key first, entries with the same key keep their order.
    fn sorted_by_key<K: Ord>(self, key: impl FnMut(&E) -> K) -> Self;
    /// Sets the positions to the order of the entries, starting at 1.
    fn renumbered(self) -> Self
    where
        E: LbRow;
    /// The entries at the places, starting at 1. Places which do not exist are skipped.
    fn places(&self, places: &[u16]) -> Self
    where
        E: Copy;
}

impl<E> LbRows<E> for Vec<E> {
    fn group_by<K: Hash + Eq + Copy>(self, key: impl Fn(&E) -> K) -> Groups<K, E> {
        Groups::new(self, key)
    }

    fn sorted_by_key<K: Ord>(mut self, key: impl FnMut(&E) -> K) -> Self {
        self.sort_by_key(key);
        self
    }

    fn renumbered(mut self) -> Self
    where
        E: LbRow,
    {
        for (index, entry) in self.iter_mut().enumerate() {
            entry.set_position((index + 1) as u16);
        }
        self
    }

    fn places(&self, places: &[u16]) -> Self
    where
        E: Copy,
    {
        places
            .iter()
            .filter_map(|place| self.get(place.checked_sub(1)? as usize).copied())
            .collect()
    }
}
