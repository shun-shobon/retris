//! 7 バッグランダマイザーとネクストキュー (仕様書 §5, §6.1)。

use crate::piece::Tetromino;
use crate::rng::Rng;

/// ネクスト表示数 (仕様書 §6.1)。
pub const NEXT_COUNT: usize = 5;

/// キュー容量。補充前の最大長 (`QUEUE_MIN - 1`) + 袋 1 つ (7) が収まる 2 袋分。
const QUEUE_CAP: usize = 14;

/// 補充後に保証するキュー長 (操作中ミノ + ネクスト 5 個、仕様書 §5.1)。
const QUEUE_MIN: usize = 6;

/// シャッフル前の袋の初期並び (仕様書 §5.1)。
const BAG: [Tetromino; 7] = [
    Tetromino::I,
    Tetromino::O,
    Tetromino::T,
    Tetromino::S,
    Tetromino::Z,
    Tetromino::J,
    Tetromino::L,
];

/// 7 バッグランダマイザー + ネクストキュー。
///
/// 固定長リングバッファで実装 (ヒープ確保なし)。`pop` 後もネクスト
/// [`NEXT_COUNT`] 個が常に `peek` できるよう、袋単位で自動補充する。
#[derive(Debug, Clone)]
pub struct PieceQueue {
    rng: Rng,
    buf: [Tetromino; QUEUE_CAP],
    head: usize,
    len: usize,
}

impl PieceQueue {
    /// シードから生成し、袋を補充してキューを満たす。
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut queue = Self {
            rng: Rng::new(seed),
            buf: [Tetromino::I; QUEUE_CAP],
            head: 0,
            len: 0,
        };
        queue.refill();
        queue
    }

    /// 次のミノを取り出す。取り出し後もネクスト 5 個が見える状態を維持する。
    pub fn pop(&mut self) -> Tetromino {
        let piece = self.buf[self.head];
        self.head = (self.head + 1) % QUEUE_CAP;
        self.len -= 1;
        self.refill();
        piece
    }

    /// ネクスト `i` 番目 (`0..NEXT_COUNT`) を非消費で先読みする。
    ///
    /// # Panics
    ///
    /// `i >= NEXT_COUNT` のとき。
    #[must_use]
    pub fn peek(&self, i: usize) -> Tetromino {
        assert!(i < NEXT_COUNT, "peek の添字は NEXT_COUNT 未満であること");
        self.buf[(self.head + i) % QUEUE_CAP]
    }

    /// キュー長が [`QUEUE_MIN`] 以上になるまで袋を補充する。
    fn refill(&mut self) {
        while self.len < QUEUE_MIN {
            self.push_bag();
        }
    }

    /// 新しい袋を Fisher–Yates でシャッフルし、末尾に 7 個追加する (仕様書 §5.1)。
    fn push_bag(&mut self) {
        debug_assert!(self.len + BAG.len() <= QUEUE_CAP);
        let mut bag = BAG;
        // for i = 6 downto 1: j = rand_bounded(i + 1); swap(bag[i], bag[j])
        for i in (1..bag.len()).rev() {
            let j = self.rng.bounded(i as u32 + 1) as usize;
            bag.swap(i, j);
        }
        for piece in bag {
            self.buf[(self.head + self.len) % QUEUE_CAP] = piece;
            self.len += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(queue: &mut PieceQueue, n: usize) -> std::vec::Vec<Tetromino> {
        (0..n).map(|_| queue.pop()).collect()
    }

    #[test]
    fn first_bag_matches_independent_fisher_yates_seed_1() {
        // 期待値は仕様書 §5.1-5.2 の疑似コード (Fisher–Yates + xorshift32 +
        // rand_bounded) を Python で独立実装して得た seed=1 の最初の袋。
        use Tetromino::{I, J, L, O, S, T, Z};
        let mut queue = PieceQueue::new(1);
        assert_eq!(take(&mut queue, 7), [T, Z, O, J, S, L, I]);
    }

    #[test]
    fn first_bag_matches_independent_fisher_yates_seed_0x12345678() {
        // 同上 (seed=0x12345678)。
        use Tetromino::{I, J, L, O, S, T, Z};
        let mut queue = PieceQueue::new(0x1234_5678);
        assert_eq!(take(&mut queue, 7), [L, J, Z, T, O, I, S]);
    }

    #[test]
    fn first_two_bags_seed_1() {
        // 袋の境界をまたいでも乱数状態が連続することの確認 (Python 独立算出)。
        use Tetromino::{I, J, L, O, S, T, Z};
        let mut queue = PieceQueue::new(1);
        assert_eq!(
            take(&mut queue, 14),
            [T, Z, O, J, S, L, I, S, J, L, Z, T, I, O]
        );
    }

    #[test]
    fn every_bag_window_contains_each_tetromino_once() {
        // 袋不変条件: 7 個ごとの各ウィンドウに 7 種がちょうど 1 個ずつ。
        for seed in [0, 1, 2, 0x1234_5678, 0xDEAD_BEEF, u32::MAX] {
            let mut queue = PieceQueue::new(seed);
            let pieces = take(&mut queue, 700);
            for (bag_index, window) in pieces.chunks(7).enumerate() {
                let mut counts = [0u32; 7];
                for &piece in window {
                    counts[piece as usize] += 1;
                }
                assert_eq!(
                    counts, [1; 7],
                    "seed={seed:#x} の袋 {bag_index} が 7 種 1 個ずつでない: {window:?}"
                );
            }
        }
    }

    #[test]
    fn no_three_consecutive_identical_pieces() {
        // 7 バッグでは同一ミノは最大 2 連続 (袋境界のみ、仕様書 §5.1)。
        for seed in [0, 1, 0x1234_5678, 0xDEAD_BEEF] {
            let mut queue = PieceQueue::new(seed);
            let pieces = take(&mut queue, 700);
            for window in pieces.windows(3) {
                assert!(
                    !(window[0] == window[1] && window[1] == window[2]),
                    "seed={seed:#x} で 3 連続: {window:?}"
                );
            }
        }
    }

    #[test]
    fn same_seed_yields_same_sequence() {
        let mut a = PieceQueue::new(42);
        let mut b = PieceQueue::new(42);
        assert_eq!(take(&mut a, 100), take(&mut b, 100));
    }

    #[test]
    fn different_seeds_yield_different_sequences() {
        // 代表例: seed 1 と 2 で最初の 21 個が一致しないこと。
        let mut a = PieceQueue::new(1);
        let mut b = PieceQueue::new(2);
        assert_ne!(take(&mut a, 21), take(&mut b, 21));
    }

    #[test]
    fn peek_is_non_consuming() {
        let queue = PieceQueue::new(1);
        let first = queue.peek(0);
        for _ in 0..10 {
            assert_eq!(queue.peek(0), first);
        }
    }

    #[test]
    fn pop_returns_previous_peek_head() {
        let mut queue = PieceQueue::new(0x1234_5678);
        for _ in 0..100 {
            let expected = queue.peek(0);
            assert_eq!(queue.pop(), expected);
        }
    }

    #[test]
    fn queue_slides_after_pop() {
        // pop 直前の peek(1..5) が pop 後の peek(0..4) になる。
        let mut queue = PieceQueue::new(0xDEAD_BEEF);
        for _ in 0..100 {
            let before: [Tetromino; NEXT_COUNT] = core::array::from_fn(|i| queue.peek(i));
            queue.pop();
            for i in 0..NEXT_COUNT - 1 {
                assert_eq!(queue.peek(i), before[i + 1]);
            }
        }
    }

    #[test]
    fn peek_always_shows_five_pieces() {
        // 何回 pop してもネクスト 5 個が peek できる (パニックしない)。
        let mut queue = PieceQueue::new(7);
        for _ in 0..700 {
            for i in 0..NEXT_COUNT {
                let _ = queue.peek(i);
            }
            queue.pop();
        }
    }

    #[test]
    #[should_panic(expected = "NEXT_COUNT")]
    fn peek_out_of_range_panics() {
        let queue = PieceQueue::new(1);
        let _ = queue.peek(NEXT_COUNT);
    }
}
