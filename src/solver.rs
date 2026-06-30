use crate::dict;
use flipperzero_sys as sys;

const BITSET_WORDS: usize = (dict::WORD_COUNT + 63) / 64;

pub struct BitSet {
    bits: [u64; BITSET_WORDS],
}

impl BitSet {
    pub const fn new() -> Self {
        BitSet {
            bits: [0u64; BITSET_WORDS],
        }
    }

    pub fn set_all(&mut self) {
        for b in &mut self.bits {
            *b = !0u64;
        }
        let rem = dict::WORD_COUNT % 64;
        if rem > 0 {
            self.bits[BITSET_WORDS - 1] &= (1u64 << rem) - 1;
        }
    }

    fn get(&self, idx: u16) -> bool {
        let i = idx as usize;
        (self.bits[i / 64] >> (i % 64)) & 1 == 1
    }

    fn clear(&mut self, idx: u16) {
        let i = idx as usize;
        self.bits[i / 64] &= !(1u64 << (i % 64));
    }

    pub fn count(&self) -> usize {
        let mut n = 0;
        for &b in &self.bits {
            n += b.count_ones() as usize;
        }
        n
    }

    pub fn first(&self) -> Option<u16> {
        for (wi, &w) in self.bits.iter().enumerate() {
            if w != 0 {
                let pos = w.trailing_zeros() as usize;
                return Some((wi * 64 + pos) as u16);
            }
        }
        None
    }
}

pub struct Solver {
    pub remaining: BitSet,
}

impl Solver {
    pub const fn new() -> Self {
        Solver {
            remaining: BitSet::new(),
        }
    }

    pub fn init(&mut self) {
        self.remaining.set_all();
    }

    pub fn apply_feedback(&mut self, guess_word: u32, marks: &[u8; 5]) {
        dict::for_each_word(|idx, word| {
            if self.remaining.get(idx) {
                if !dict::word_matches(word, guess_word, marks) {
                    self.remaining.clear(idx);
                }
            }
            true
        });
    }

    pub fn best_candidate(&self) -> Option<u16> {
        let total = self.count();
        if total == 0 {
            return None;
        }
        if total == 1 {
            return self.remaining.first();
        }

        let mut best = 0u16;
        let mut best_score = 0u32;
        let mut seen = 0u32;

        dict::for_each_word(|idx, word| {
            if !self.remaining.get(idx) {
                return true;
            }
            let score = dict::unique_count(word);
            if score > best_score {
                best_score = score;
                best = idx;
                seen = 1;
            } else if score == best_score {
                seen += 1;
                let r = unsafe { sys::furi_hal_random_get() } % seen;
                if r == 0 {
                    best = idx;
                }
            }
            true
        });

        Some(best)
    }

    pub fn first_word(&self) -> Option<u16> {
        self.remaining.first()
    }

    pub fn count(&self) -> usize {
        self.remaining.count()
    }
}
