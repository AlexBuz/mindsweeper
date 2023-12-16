use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub enum Flag {
    Tentative,
    Permanent,
}

impl Flag {
    pub fn is_tentative(&self) -> bool {
        matches!(self, Flag::Tentative)
    }
}

pub struct FlagStore {
    flags: BTreeMap<usize, Flag>,
}

impl FlagStore {
    pub fn new() -> Self {
        Self {
            flags: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.flags.len()
    }

    pub fn clear(&mut self) {
        self.flags.clear();
    }

    pub fn get(&self, tile_id: usize) -> Option<&Flag> {
        self.flags.get(&tile_id)
    }

    pub fn contains(&self, tile_id: usize) -> bool {
        self.flags.contains_key(&tile_id)
    }

    pub fn insert_tentative(&mut self, tile_id: usize) {
        self.flags.insert(tile_id, Flag::Tentative);
    }

    pub fn insert_permanent(&mut self, tile_id: usize) {
        self.flags.insert(tile_id, Flag::Permanent);
    }

    pub fn remove(&mut self, tile_id: usize) {
        self.flags.remove(&tile_id);
    }

    pub fn toggle(&mut self, tile_id: usize) {
        match self.get(tile_id) {
            Some(Flag::Tentative) => {
                self.flags.remove(&tile_id);
            }
            None => {
                self.flags.insert(tile_id, Flag::Tentative);
            }
            _ => {}
        }
    }
}
