//! Kleiner deterministischer Zufallsgenerator (PCG-XSH-RR, 64/32).
//!
//! Bewusst selbst implementiert statt `rand`: die Simulation braucht nur
//! gleichverteilte Zahlen für Waffenstreuung und Spawnauswahl, und ein
//! festsetzbarer Seed macht Tests reproduzierbar.

#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Seed aus der Systemzeit, für den regulären Serverstart.
    pub fn from_entropy() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x853c_49e6_748f_ea9b);
        Self::from_seed(nanos)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Gleichverteilt in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        // 24 Bit Mantisse: liefert exakt darstellbare Werte ohne Rundung auf 1.0.
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Gleichverteilt in `0..n`. Bei `n == 0` wird `0` geliefert.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u32() as usize) % n
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn gleicher_seed_gleiche_folge() {
        let mut b = Rng::from_seed(7);
        let mut c = Rng::from_seed(7);
        for _ in 0..64 {
            assert_eq!(b.next_u32(), c.next_u32());
        }
    }

    #[test]
    fn verschiedene_seeds_verschiedene_folgen() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(
            (0..8).map(|_| a.next_u32()).collect::<Vec<_>>(),
            (0..8).map(|_| b.next_u32()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unit_bleibt_im_halboffenen_intervall() {
        let mut rng = Rng::from_seed(42);
        for _ in 0..100_000 {
            let v = rng.unit();
            assert!((0.0..1.0).contains(&v), "unit() lieferte {v}");
        }
    }

    #[test]
    fn below_bleibt_im_bereich_und_deckt_ihn_ab() {
        let mut rng = Rng::from_seed(9);
        let mut gesehen = [false; 5];
        for _ in 0..1000 {
            let v = rng.below(5);
            assert!(v < 5);
            gesehen[v] = true;
        }
        assert!(gesehen.iter().all(|&g| g));
        assert_eq!(rng.below(0), 0);
    }
}
