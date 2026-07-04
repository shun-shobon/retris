//! T-Spin 判定 — 3 コーナールール (仕様書 §10)。
//!
//! 「最後に成功した操作が回転か」と「使用したキックテスト番号」は
//! ゲームステート側 (後工程) が保持し、本判定には引数で渡す。

use crate::active::ActivePiece;
use crate::board::Board;
use crate::piece::{Rotation, Tetromino};

/// T-Spin 判定結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSpin {
    /// T-Spin ではない (通常の設置・消去)。
    None,
    /// T-Spin Mini。
    Mini,
    /// フル T-Spin。
    Full,
}

/// ロック時の T-Spin 判定 (仕様書 §10)。
///
/// - `last_action_was_rotation`: 最後に成功した操作が回転か (§10.1-2)。
///   回転後に左右移動・落下が 1 セルでも起これば `false`。
/// - `last_kick`: 最後の回転で成立したキックテスト番号 1〜5
///   ([`crate::srs::RotateOutcome::Rotated`] の `kick`)。5 なら Mini 条件でも
///   フル T-Spin に格上げする (§10.3 のキック 5 例外、TST / Fin 型)。
#[must_use]
pub fn detect_tspin(
    board: &Board,
    piece: &ActivePiece,
    last_action_was_rotation: bool,
    last_kick: u8,
) -> TSpin {
    // 前提条件 (§10.1): T ミノであり、最後に成功した操作が回転であること。
    if !matches!(piece.kind, Tetromino::T) || !last_action_was_rotation {
        return TSpin::None;
    }

    // 3×3 ボックスの 4 コーナー (§10.2)。フィールド外 (壁・床) は占有扱い。
    let a = board.is_occupied(piece.x, piece.y + 2); // A: 左上
    let b = board.is_occupied(piece.x + 2, piece.y + 2); // B: 右上
    let c = board.is_occupied(piece.x, piece.y); // C: 左下
    let d = board.is_occupied(piece.x + 2, piece.y); // D: 右下

    // 前面 = T の尖りが向いている側の 2 コーナー (§10.2 の表)。
    let (front, back) = match piece.rot {
        Rotation::Spawn => ([a, b], [c, d]),
        Rotation::Cw => ([b, d], [a, c]),
        Rotation::Flip => ([c, d], [a, b]),
        Rotation::Ccw => ([a, c], [b, d]),
    };
    let front_count = front.iter().filter(|&&occupied| occupied).count();
    let back_count = back.iter().filter(|&&occupied| occupied).count();

    // 判定 (§10.3)。コーナー 3 つ以上 (§10.1-3) は各条件に含意される。
    if front_count == 2 && back_count >= 1 {
        TSpin::Full
    } else if front_count == 1 && back_count == 2 {
        // キック 5 例外: TST / Fin 型は Mini 条件でも常にフル T-Spin。
        if last_kick == 5 {
            TSpin::Full
        } else {
            TSpin::Mini
        }
    } else {
        TSpin::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t_piece(rot: Rotation, x: i8, y: i8) -> ActivePiece {
        ActivePiece {
            kind: Tetromino::T,
            rot,
            x,
            y,
        }
    }

    /// TSD 型の T スロット地形。T (状態 2 = 尖り下) がボックス左下 (3,0) に収まる。
    ///
    /// ```text
    /// y=2: . . . S . . . . . .   ← (3,2) = 背面コーナー A のオーバーハング
    /// y=1: L L L _ _ _ L L L L   ← T の横棒が入る
    /// y=0: J J J J _ J J J J J   ← T の尖りが入る
    /// ```
    ///
    /// コーナー: A=(3,2) 占有, B=(5,2) 空, C=(3,0) 占有, D=(5,0) 占有。
    /// 状態 2 の前面 = C, D (2 つ)、背面 = A, B (1 つ) → フル T-Spin 条件。
    fn tsd_setup() -> (Board, ActivePiece) {
        let mut board = Board::new();
        for x in 0..10 {
            if x != 4 {
                board.set(x, 0, Some(Tetromino::J));
            }
            if !(3..=5).contains(&x) {
                board.set(x, 1, Some(Tetromino::L));
            }
        }
        board.set(3, 2, Some(Tetromino::S));
        (board, t_piece(Rotation::Flip, 3, 0))
    }

    /// 左壁 Mini 型地形。T (状態 R = 尖り右) がボックス左下 (-1,0)。
    ///
    /// コーナー: A=(-1,2), C=(-1,0) は左壁の外 = 占有扱い (背面 2 つ)、
    /// D=(1,0) にブロック、B=(1,2) は空 (前面 1 つ) → Mini 条件。
    fn wall_mini_setup() -> (Board, ActivePiece) {
        let mut board = Board::new();
        board.set(1, 0, Some(Tetromino::J));
        (board, t_piece(Rotation::Cw, -1, 0))
    }

    #[test]
    fn tsd_shape_after_rotation_is_full_tspin() {
        let (board, piece) = tsd_setup();
        // 前面 2 + 背面 1 → フル T-Spin (§10.3)。キック 1〜4 は判定に影響しない。
        for kick in 1..=4 {
            assert_eq!(
                detect_tspin(&board, &piece, true, kick),
                TSpin::Full,
                "kick={kick}"
            );
        }
    }

    #[test]
    fn all_four_corners_is_full_tspin() {
        let (mut board, piece) = tsd_setup();
        board.set(5, 2, Some(Tetromino::S)); // B も埋めて 4 コーナー
        assert_eq!(detect_tspin(&board, &piece, true, 1), TSpin::Full);
    }

    #[test]
    fn front1_back2_is_mini() {
        let (board, piece) = wall_mini_setup();
        assert_eq!(detect_tspin(&board, &piece, true, 2), TSpin::Mini);
    }

    #[test]
    fn kick5_upgrades_mini_condition_to_full() {
        // キック 5 例外 (§10.3): Mini 条件でも常にフル T-Spin (TST / Fin 型)。
        let (board, piece) = wall_mini_setup();
        assert_eq!(detect_tspin(&board, &piece, true, 5), TSpin::Full);
    }

    #[test]
    fn no_rotation_is_none_even_on_tspin_shape() {
        let (board, piece) = tsd_setup();
        assert_eq!(detect_tspin(&board, &piece, false, 1), TSpin::None);
    }

    #[test]
    fn non_t_piece_is_none() {
        let (board, t) = tsd_setup();
        for kind in [
            Tetromino::I,
            Tetromino::O,
            Tetromino::S,
            Tetromino::Z,
            Tetromino::J,
            Tetromino::L,
        ] {
            let piece = ActivePiece { kind, ..t };
            assert_eq!(
                detect_tspin(&board, &piece, true, 1),
                TSpin::None,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn fewer_than_three_corners_is_none() {
        // 前面 C, D のみ占有 (2 コーナー) → 前提条件 §10.1-3 を満たさない。
        let mut board = Board::new();
        board.set(3, 0, Some(Tetromino::J));
        board.set(5, 0, Some(Tetromino::J));
        let piece = t_piece(Rotation::Flip, 3, 0);
        assert_eq!(detect_tspin(&board, &piece, true, 1), TSpin::None);
    }

    #[test]
    fn floor_counts_as_occupied_corners() {
        // 床上の T (状態 0)。背面 C=(3,-1), D=(5,-1) は床の外 = 占有扱い。
        // 前面は A=(3,1) のブロック 1 つのみ → Mini。
        let mut board = Board::new();
        board.set(3, 1, Some(Tetromino::J));
        let piece = t_piece(Rotation::Spawn, 3, -1);
        assert_eq!(detect_tspin(&board, &piece, true, 1), TSpin::Mini);
    }

    #[test]
    fn right_wall_counts_as_occupied_corners() {
        // 右壁際の T (状態 L = 尖り左)。背面 B=(10,2), D=(10,0) は右壁の外 = 占有扱い。
        // 前面は C=(8,0) のブロック 1 つのみ → Mini。
        let mut board = Board::new();
        board.set(8, 0, Some(Tetromino::J));
        let piece = t_piece(Rotation::Ccw, 8, 0);
        assert_eq!(detect_tspin(&board, &piece, true, 1), TSpin::Mini);
    }
}
