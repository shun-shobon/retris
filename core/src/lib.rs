//! retris-core: テトリスガイドライン準拠のゲームロジック (`no_std`)。
//!
//! GBAバイナリ (`gba/`) から利用される。ホスト上で `cargo test` 可能。

#![cfg_attr(not(test), no_std)]

/// 盤面の幅(セル数)。
pub const BOARD_WIDTH: usize = 10;

/// 盤面の可視領域の高さ(セル数)。
pub const BOARD_HEIGHT: usize = 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_dimensions() {
        assert_eq!(BOARD_WIDTH, 10);
        assert_eq!(BOARD_HEIGHT, 20);
    }
}
