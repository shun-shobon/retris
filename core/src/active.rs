//! 操作中ミノ (仕様書 §2.1, §3.1)。

use crate::piece::{Rotation, Tetromino};

/// 操作中のミノ。
///
/// `(x, y)` はバウンディングボックス左下のフィールド座標 (仕様書 §2.1 の `(bx, by)`)。
/// キックにより一時的に負値をとりうるため符号付き。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivePiece {
    pub kind: Tetromino,
    pub rot: Rotation,
    pub x: i8,
    pub y: i8,
}

impl ActivePiece {
    /// 仕様書 §3.1 のスポーン位置・状態 0 でミノを生成する。
    #[must_use]
    pub fn spawn(kind: Tetromino) -> Self {
        let (x, y) = kind.spawn_pos();
        Self {
            kind,
            rot: Rotation::Spawn,
            x,
            y,
        }
    }

    /// 占有する 4 セルの絶対フィールド座標。
    #[must_use]
    pub fn cells(&self) -> [(i8, i8); 4] {
        self.kind
            .cells(self.rot)
            .map(|(cx, cy)| (self.x + cx, self.y + cy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::{Rotation, Tetromino};

    fn sorted(mut cells: [(i8, i8); 4]) -> [(i8, i8); 4] {
        cells.sort_unstable();
        cells
    }

    #[track_caller]
    fn assert_spawn_cells(kind: Tetromino, expected: [(i8, i8); 4]) {
        assert_eq!(
            sorted(ActivePiece::spawn(kind).cells()),
            sorted(expected),
            "{kind:?} のスポーン占有セルが仕様 §3.1 と不一致"
        );
    }

    #[test]
    fn spawn_starts_in_spawn_rotation_at_spec_position() {
        let piece = ActivePiece::spawn(Tetromino::T);
        assert_eq!(piece.kind, Tetromino::T);
        assert_eq!(piece.rot, Rotation::Spawn);
        assert_eq!((piece.x, piece.y), (3, 19));

        let piece = ActivePiece::spawn(Tetromino::I);
        assert_eq!((piece.x, piece.y), (3, 18));

        let piece = ActivePiece::spawn(Tetromino::O);
        assert_eq!((piece.x, piece.y), (4, 20));
    }

    #[test]
    fn spawn_cells_match_spec() {
        // 期待値は仕様書 §3.1 の「占有セル (フィールド座標)」列からの手書き転記。
        assert_spawn_cells(Tetromino::I, [(3, 20), (4, 20), (5, 20), (6, 20)]);
        assert_spawn_cells(Tetromino::O, [(4, 20), (5, 20), (4, 21), (5, 21)]);
        assert_spawn_cells(Tetromino::T, [(3, 20), (4, 20), (5, 20), (4, 21)]);
        assert_spawn_cells(Tetromino::S, [(3, 20), (4, 20), (4, 21), (5, 21)]);
        assert_spawn_cells(Tetromino::Z, [(4, 20), (5, 20), (3, 21), (4, 21)]);
        assert_spawn_cells(Tetromino::J, [(3, 20), (4, 20), (5, 20), (3, 21)]);
        assert_spawn_cells(Tetromino::L, [(3, 20), (4, 20), (5, 20), (5, 21)]);
    }

    #[test]
    fn cells_translate_box_coords_by_position() {
        // T の R 状態はボックス内 (1,0) (1,1) (2,1) (1,2) (§2.2 手書き転記)。
        // ボックス左下 (2,5) なら実セルは各座標 +(2,5)。
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Cw,
            x: 2,
            y: 5,
        };
        assert_eq!(
            sorted(piece.cells()),
            sorted([(3, 5), (3, 6), (4, 6), (3, 7)])
        );
    }

    #[test]
    fn cells_allow_negative_box_origin() {
        // I の L 状態はボックス内 (1,0) (1,1) (1,2) (1,3) (§2.2 手書き転記)。
        // キックで bx=-1 になっても実セルは x=0 列に収まる。
        let piece = ActivePiece {
            kind: Tetromino::I,
            rot: Rotation::Ccw,
            x: -1,
            y: 0,
        };
        assert_eq!(
            sorted(piece.cells()),
            sorted([(0, 0), (0, 1), (0, 2), (0, 3)])
        );
    }
}
