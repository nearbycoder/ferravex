//! Dense dirty/visibility masks with checked indices and ascending iteration.
//! The caller chooses the index bound; bits outside that bound are never exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitSet {
    words: Vec<u64>,
    bits: usize,
}
impl BitSet {
    pub fn new(bits: usize) -> Self {
        Self {
            words: vec![0; bits.div_ceil(64)],
            bits,
        }
    }
    pub fn set(&mut self, index: usize, value: bool) -> bool {
        if index >= self.bits {
            return false;
        }
        let mask = 1_u64 << (index % 64);
        if value {
            self.words[index / 64] |= mask;
        } else {
            self.words[index / 64] &= !mask;
        }
        true
    }
    pub fn contains(&self, index: usize) -> bool {
        index < self.bits && self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
    pub fn clear(&mut self) {
        self.words.fill(0);
    }
    pub fn union_with(&mut self, other: &Self) -> bool {
        if self.bits != other.bits {
            return false;
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
        true
    }
    pub fn intersection_with(&mut self, other: &Self) -> bool {
        if self.bits != other.bits {
            return false;
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= *b;
        }
        true
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(word, &bits)| {
            let mut remaining = bits;
            std::iter::from_fn(move || {
                if remaining == 0 {
                    return None;
                }
                let bit = remaining.trailing_zeros() as usize;
                remaining &= remaining - 1;
                Some(word * 64 + bit)
            })
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_edges_set_algebra_and_no_padding_bits() {
        let mut a = BitSet::new(65);
        for i in [0, 63, 64] {
            assert!(a.set(i, true));
        }
        assert!(!a.set(65, true));
        assert!(!a.contains(usize::MAX));
        assert_eq!(a.iter().collect::<Vec<_>>(), [0, 63, 64]);
        assert_eq!(a.count(), 3);
        let mut b = BitSet::new(65);
        b.set(64, true);
        assert!(a.intersection_with(&b));
        assert_eq!(a.iter().collect::<Vec<_>>(), [64]);
        b.set(1, true);
        a.union_with(&b);
        assert_eq!(a.iter().collect::<Vec<_>>(), [1, 64]);
        let before = a.clone();
        assert!(!a.union_with(&BitSet::new(64)));
        assert_eq!(a, before);
        a.clear();
        assert_eq!(a.count(), 0);
        assert_eq!(BitSet::new(0).iter().count(), 0);
    }
}
