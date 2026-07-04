//! retris-core: テトリスガイドライン準拠のゲームロジック (`no_std`)。
//!
//! GBAバイナリ (`gba/`) から利用される。ホスト上で `cargo test` 可能。
//!
//! 座標系 (仕様書 §1.1): x は左端 0・右端 9 (右が正)、y は最下行 0・最上行 39 (上が正)。

#![cfg_attr(not(test), no_std)]

pub mod active;
pub mod bag;
pub mod board;
pub mod lock_delay;
pub mod physics;
pub mod piece;
pub mod rng;
pub mod srs;

pub use active::ActivePiece;
pub use bag::{NEXT_COUNT, PieceQueue};
pub use board::{Board, FIELD_HEIGHT, FIELD_WIDTH, VISIBLE_HEIGHT};
pub use lock_delay::{LOCK_DELAY_FRAMES, LOCK_RESET_MAX, LockDelay};
pub use physics::{
    GravityAccumulator, SOFT_DROP_FACTOR, ghost, gravity_q16, is_grounded, try_fall, try_shift,
};
pub use piece::{Rotation, Tetromino};
pub use rng::Rng;
pub use srs::{RotateDir, RotateOutcome, try_rotate};
