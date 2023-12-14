type Chunk = usize;

const BITS_PER_CHUNK: usize = Chunk::BITS as usize;

#[derive(Debug, Clone, Default)]
pub struct BitSet {
    vec: Vec<Chunk>,
}

#[allow(unused)]
impl BitSet {
    pub fn new() -> Self {
        Self { vec: vec![] }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: vec![0; (capacity + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK],
        }
    }

    fn get_chunk(&self, index: usize) -> Chunk {
        self.vec.get(index).copied().unwrap_or(0)
    }

    fn get_chunk_mut(&mut self, index: usize) -> &mut Chunk {
        if index >= self.vec.len() {
            self.vec.resize(index + 1, 0);
        }
        unsafe { self.vec.get_unchecked_mut(index) }
    }

    fn index_and_mask_for(value: usize) -> (usize, Chunk) {
        let index = value / BITS_PER_CHUNK;
        let offset = value % BITS_PER_CHUNK;
        let mask = 1 << offset;
        (index, mask)
    }

    pub fn insert(&mut self, value: usize) -> bool {
        let (index, mask) = Self::index_and_mask_for(value);
        let chunk = self.get_chunk_mut(index);
        let insertion_was_needed = *chunk & mask == 0;
        *chunk |= mask;
        insertion_was_needed
    }

    pub fn remove(&mut self, value: usize) -> bool {
        let (index, mask) = Self::index_and_mask_for(value);
        let chunk = self.get_chunk_mut(index);
        let removal_was_needed = *chunk & mask != 0;
        *chunk &= !mask;
        removal_was_needed
    }

    pub fn toggle(&mut self, value: usize) -> bool {
        let (index, mask) = Self::index_and_mask_for(value);
        let chunk = self.get_chunk_mut(index);
        *chunk ^= mask;
        *chunk & mask != 0
    }

    pub fn contains(&self, value: usize) -> bool {
        let (index, mask) = Self::index_and_mask_for(value);
        let chunk = self.get_chunk(index);
        chunk & mask != 0
    }

    pub fn is_empty(&self) -> bool {
        self.vec.iter().all(|&chunk| chunk == 0)
    }
}

#[derive(Clone)]
pub struct BitSetIter {
    chunk_iter: <Vec<usize> as IntoIterator>::IntoIter,
    chunk: Chunk,
    bits_remaining: usize,
    next_value: usize,
}

impl Iterator for BitSetIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.chunk == 0 {
                self.next_value += self.bits_remaining;
                self.bits_remaining = 0;
            }
            if self.bits_remaining == 0 {
                match self.chunk_iter.next() {
                    Some(chunk) => self.chunk = chunk,
                    None => return None,
                }
                self.bits_remaining = BITS_PER_CHUNK;
            }
            let chunk = self.chunk;
            self.chunk >>= 1;
            let value = self.next_value;
            self.next_value += 1;
            self.bits_remaining -= 1;
            if chunk & 1 == 1 {
                return Some(value);
            }
        }
    }
}

impl IntoIterator for BitSet {
    type Item = usize;

    type IntoIter = BitSetIter;

    fn into_iter(self) -> Self::IntoIter {
        BitSetIter {
            chunk_iter: self.vec.into_iter(),
            chunk: 0,
            bits_remaining: 0,
            next_value: 0,
        }
    }
}

#[derive(Clone)]
pub struct BorrowedBitSetIter<'a> {
    chunk_iter: <&'a [usize] as IntoIterator>::IntoIter,
    chunk: Chunk,
    bits_remaining: usize,
    next_value: usize,
}

impl Iterator for BorrowedBitSetIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.chunk == 0 {
                self.next_value += self.bits_remaining;
                self.bits_remaining = 0;
            }
            if self.bits_remaining == 0 {
                match self.chunk_iter.next() {
                    Some(&chunk) => self.chunk = chunk,
                    None => return None,
                }
                self.bits_remaining = BITS_PER_CHUNK;
            }
            let chunk = self.chunk;
            self.chunk >>= 1;
            let value = self.next_value;
            self.next_value += 1;
            self.bits_remaining -= 1;
            if chunk & 1 == 1 {
                return Some(value);
            }
        }
    }
}

impl<'a> IntoIterator for &'a BitSet {
    type Item = usize;

    type IntoIter = BorrowedBitSetIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        BorrowedBitSetIter {
            chunk_iter: self.vec.iter(),
            chunk: 0,
            bits_remaining: 0,
            next_value: 0,
        }
    }
}

impl BitSet {
    pub fn iter(&self) -> impl Iterator<Item = usize> + Clone + '_ {
        self.into_iter()
    }
}

impl Extend<usize> for BitSet {
    fn extend<T: IntoIterator<Item = usize>>(&mut self, iter: T) {
        for value in iter {
            self.insert(value);
        }
    }
}

impl FromIterator<usize> for BitSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut set = BitSet::new();
        for value in iter {
            set.insert(value);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteration() {
        let mut set = BitSet::with_capacity(10);
        set.insert(3);
        set.insert(1);
        set.insert(4);
        set.insert(1);
        set.insert(5);
        set.insert(9);
        let vec: Vec<usize> = set.into_iter().collect();
        assert_eq!(vec, [1, 3, 4, 5, 9]);
    }
}
