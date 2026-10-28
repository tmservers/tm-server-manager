//! The randomness of a simulation, all of it from one seed.

/// SplitMix64. Small and fixed, so a seed gives the same simulation on every
/// platform and with every version of the dependencies, which the snapshots of
/// the simulations rely on.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// An independent stream for one part of a simulation, e.g. the races of a
    /// player in a round. Drawing more numbers in one part then leaves the others
    /// as they were, so a change in one place does not reshuffle the whole match.
    pub fn fork(&self, key: &str) -> Self {
        // FNV-1a, which unlike the hasher of std is the same everywhere.
        let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        Self(mix(self.0 ^ mix(hash)))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.0)
    }

    /// Below `n`, 0 for 0.
    pub fn below(&mut self, n: u32) -> u32 {
        // Multiplying instead of the remainder keeps it unbiased enough.
        (((self.next_u64() >> 32) * u64::from(n)) >> 32) as u32
    }

    /// From `low` to `high`, both included.
    pub fn between(&mut self, low: u32, high: u32) -> u32 {
        low + self.below(high - low + 1)
    }

    /// True with a probability in per mille.
    pub fn chance(&mut self, per_mille: u32) -> bool {
        self.below(1000) < per_mille
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_numbers() {
        let numbers = |seed| {
            let mut rng = Rng::new(seed);
            (0..4).map(|_| rng.below(100)).collect::<Vec<_>>()
        };
        assert_eq!(numbers(7), numbers(7));
        assert_ne!(numbers(7), numbers(8));
        // Fixed forever, the snapshots depend on it.
        assert_eq!(Rng::new(0).next_u64(), 0xe220_a839_7b1d_cdaf);
    }

    #[test]
    fn forks_are_independent_of_each_other() {
        let rng = Rng::new(1);
        let mut first = rng.fork("a");
        let mut again = rng.fork("a");
        assert_eq!(first.next_u64(), again.next_u64());
        assert_ne!(rng.fork("a").next_u64(), rng.fork("b").next_u64());
    }

    #[test]
    fn numbers_stay_in_their_range() {
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            assert!((5..=9).contains(&rng.between(5, 9)));
        }
        assert_eq!(rng.below(0), 0);
        assert!((0..1000).filter(|_| rng.chance(500)).count().abs_diff(500) < 60);
    }
}
