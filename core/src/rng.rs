//! xorshift32 擬似乱数 (仕様書 §5.2)。

/// xorshift32 PRNG (仕様書 §5.2)。
///
/// 内部状態は常に非ゼロ (0 だと全出力が 0 に固定されるため、生成時に保証する)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// シードから生成する。`seed == 0` は 1 に置き換えて非ゼロを保証する。
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        let state = if seed == 0 { 1 } else { seed };
        Self { state }
    }

    /// 次の乱数値 (仕様書 §5.2 の `xorshift32()`)。
    pub const fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// `0 <= 戻り値 < n` の一様乱数 (仕様書 §5.2 の `rand_bounded()`)。
    ///
    /// `n` は 1 以上 65536 以下であること (仕様は n <= 256 程度を想定)。
    pub const fn bounded(&mut self, n: u32) -> u32 {
        debug_assert!(n >= 1 && n <= 65_536);
        ((self.next_u32() >> 16) * n) >> 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 期待値は仕様書 §5.2 の C コードを Python で独立実装して算出した値の転記
    // (x ^= x<<13; x ^= x>>17; x ^= x<<5; を u32 で評価)。

    #[test]
    fn xorshift32_sequence_seed_1() {
        let mut rng = Rng::new(1);
        assert_eq!(rng.next_u32(), 270_369);
        assert_eq!(rng.next_u32(), 67_634_689);
        assert_eq!(rng.next_u32(), 2_647_435_461);
        assert_eq!(rng.next_u32(), 307_599_695);
        assert_eq!(rng.next_u32(), 2_398_689_233);
    }

    #[test]
    fn xorshift32_sequence_seed_0x12345678() {
        let mut rng = Rng::new(0x1234_5678);
        assert_eq!(rng.next_u32(), 0x8798_5AA5);
        assert_eq!(rng.next_u32(), 0x155B_24A3);
        assert_eq!(rng.next_u32(), 0x4820_F4C4);
        assert_eq!(rng.next_u32(), 0x81B3_AC98);
        assert_eq!(rng.next_u32(), 0x703A_0788);
    }

    #[test]
    fn seed_zero_is_replaced_and_still_generates() {
        // 状態 0 の xorshift32 は永久に 0 を返すため、0 シードは非ゼロに置換される。
        let mut rng = Rng::new(0);
        let first = rng.next_u32();
        assert_ne!(first, 0);
        // 置換規則 (0 → 1) により seed=1 と同一列になる。
        assert_eq!(first, 270_369);
    }

    #[test]
    fn nonzero_seed_is_preserved() {
        // 非ゼロシードはそのまま使われる (seed=2 は seed=1 と異なる列)。
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u32(), b.next_u32());
    }

    #[test]
    fn bounded_sequence_matches_spec_formula() {
        // seed=1 の乱数列に ((x >> 16) * 7) >> 16 を適用した期待値 (Python で独立算出)。
        let mut rng = Rng::new(1);
        assert_eq!(rng.bounded(7), 0);
        assert_eq!(rng.bounded(7), 0);
        assert_eq!(rng.bounded(7), 4);
        assert_eq!(rng.bounded(7), 0);
        assert_eq!(rng.bounded(7), 3);
    }

    #[test]
    fn bounded_is_always_in_range() {
        let mut rng = Rng::new(0xDEAD_BEEF);
        for n in 1..=16 {
            for _ in 0..1_000 {
                let v = rng.bounded(n);
                assert!(v < n, "bounded({n}) が {v} を返した");
            }
        }
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }
}
