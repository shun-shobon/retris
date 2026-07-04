//! テトロミノと回転状態の定義 (仕様書 §2, §3.1)。

/// テトロミノの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tetromino {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

/// 回転状態 (仕様書 §0: `0`=Spawn, `R`=Cw, `2`=Flip, `L`=Ccw)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// 状態 0 (スポーン向き)。
    Spawn,
    /// 状態 R (時計回り 90°)。
    Cw,
    /// 状態 2 (180°)。
    Flip,
    /// 状態 L (反時計回り 90°)。
    Ccw,
}

impl Rotation {
    /// 時計回りに 90° 回転した状態。
    #[must_use]
    pub const fn cw(self) -> Self {
        match self {
            Self::Spawn => Self::Cw,
            Self::Cw => Self::Flip,
            Self::Flip => Self::Ccw,
            Self::Ccw => Self::Spawn,
        }
    }

    /// 反時計回りに 90° 回転した状態。
    #[must_use]
    pub const fn ccw(self) -> Self {
        match self {
            Self::Spawn => Self::Ccw,
            Self::Ccw => Self::Flip,
            Self::Flip => Self::Cw,
            Self::Cw => Self::Spawn,
        }
    }
}

/// 形状テーブル (仕様書 §2.2)。
///
/// `SHAPES[ミノ][回転状態]` = ボックス内セル座標 `(cx, cy)` ×4 (左下原点・y 上向き)。
/// 添字は各 enum の宣言順 (`Tetromino`: I,O,T,S,Z,J,L / `Rotation`: Spawn,Cw,Flip,Ccw)。
const SHAPES: [[[(i8, i8); 4]; 4]; 7] = [
    // I (4×4)
    [
        [(0, 2), (1, 2), (2, 2), (3, 2)],
        [(2, 0), (2, 1), (2, 2), (2, 3)],
        [(0, 1), (1, 1), (2, 1), (3, 1)],
        [(1, 0), (1, 1), (1, 2), (1, 3)],
    ],
    // O (2×2、全状態同一)
    [
        [(0, 0), (1, 0), (0, 1), (1, 1)],
        [(0, 0), (1, 0), (0, 1), (1, 1)],
        [(0, 0), (1, 0), (0, 1), (1, 1)],
        [(0, 0), (1, 0), (0, 1), (1, 1)],
    ],
    // T (3×3)
    [
        [(0, 1), (1, 1), (2, 1), (1, 2)],
        [(1, 0), (1, 1), (2, 1), (1, 2)],
        [(0, 1), (1, 1), (2, 1), (1, 0)],
        [(1, 0), (0, 1), (1, 1), (1, 2)],
    ],
    // S (3×3)
    [
        [(0, 1), (1, 1), (1, 2), (2, 2)],
        [(1, 2), (1, 1), (2, 1), (2, 0)],
        [(0, 0), (1, 0), (1, 1), (2, 1)],
        [(0, 2), (0, 1), (1, 1), (1, 0)],
    ],
    // Z (3×3)
    [
        [(1, 1), (2, 1), (0, 2), (1, 2)],
        [(2, 2), (1, 1), (2, 1), (1, 0)],
        [(0, 1), (1, 1), (1, 0), (2, 0)],
        [(1, 2), (0, 1), (1, 1), (0, 0)],
    ],
    // J (3×3)
    [
        [(0, 1), (1, 1), (2, 1), (0, 2)],
        [(1, 2), (2, 2), (1, 1), (1, 0)],
        [(0, 1), (1, 1), (2, 1), (2, 0)],
        [(1, 2), (1, 1), (0, 0), (1, 0)],
    ],
    // L (3×3)
    [
        [(0, 1), (1, 1), (2, 1), (2, 2)],
        [(1, 2), (1, 1), (1, 0), (2, 0)],
        [(0, 1), (1, 1), (2, 1), (0, 0)],
        [(0, 2), (1, 2), (1, 1), (1, 0)],
    ],
];

impl Tetromino {
    /// 回転状態 `rot` におけるボックス内セル座標 (仕様書 §2.2)。
    #[must_use]
    pub const fn cells(self, rot: Rotation) -> [(i8, i8); 4] {
        SHAPES[self as usize][rot as usize]
    }

    /// スポーン時のボックス左下フィールド座標 `(bx, by)` (仕様書 §3.1)。
    #[must_use]
    pub const fn spawn_pos(self) -> (i8, i8) {
        match self {
            Self::I => (3, 18),
            Self::O => (4, 20),
            Self::T | Self::S | Self::Z | Self::J | Self::L => (3, 19),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut cells: [(i8, i8); 4]) -> [(i8, i8); 4] {
        cells.sort_unstable();
        cells
    }

    #[track_caller]
    fn assert_shape(kind: Tetromino, rot: Rotation, expected: [(i8, i8); 4]) {
        assert_eq!(
            sorted(kind.cells(rot)),
            sorted(expected),
            "{kind:?} の {rot:?} 状態が仕様 §2.2 と不一致"
        );
    }

    // 期待値はすべて仕様書 §2.2 の表からの手書き転記 (ボックス内座標、左下原点)。

    #[test]
    fn shape_i() {
        assert_shape(
            Tetromino::I,
            Rotation::Spawn,
            [(0, 2), (1, 2), (2, 2), (3, 2)],
        );
        assert_shape(Tetromino::I, Rotation::Cw, [(2, 0), (2, 1), (2, 2), (2, 3)]);
        assert_shape(
            Tetromino::I,
            Rotation::Flip,
            [(0, 1), (1, 1), (2, 1), (3, 1)],
        );
        assert_shape(
            Tetromino::I,
            Rotation::Ccw,
            [(1, 0), (1, 1), (1, 2), (1, 3)],
        );
    }

    #[test]
    fn shape_o() {
        let cells = [(0, 0), (1, 0), (0, 1), (1, 1)];
        assert_shape(Tetromino::O, Rotation::Spawn, cells);
        assert_shape(Tetromino::O, Rotation::Cw, cells);
        assert_shape(Tetromino::O, Rotation::Flip, cells);
        assert_shape(Tetromino::O, Rotation::Ccw, cells);
    }

    #[test]
    fn shape_t() {
        assert_shape(
            Tetromino::T,
            Rotation::Spawn,
            [(0, 1), (1, 1), (2, 1), (1, 2)],
        );
        assert_shape(Tetromino::T, Rotation::Cw, [(1, 0), (1, 1), (2, 1), (1, 2)]);
        assert_shape(
            Tetromino::T,
            Rotation::Flip,
            [(0, 1), (1, 1), (2, 1), (1, 0)],
        );
        assert_shape(
            Tetromino::T,
            Rotation::Ccw,
            [(1, 0), (0, 1), (1, 1), (1, 2)],
        );
    }

    #[test]
    fn shape_s() {
        assert_shape(
            Tetromino::S,
            Rotation::Spawn,
            [(0, 1), (1, 1), (1, 2), (2, 2)],
        );
        assert_shape(Tetromino::S, Rotation::Cw, [(1, 2), (1, 1), (2, 1), (2, 0)]);
        assert_shape(
            Tetromino::S,
            Rotation::Flip,
            [(0, 0), (1, 0), (1, 1), (2, 1)],
        );
        assert_shape(
            Tetromino::S,
            Rotation::Ccw,
            [(0, 2), (0, 1), (1, 1), (1, 0)],
        );
    }

    #[test]
    fn shape_z() {
        assert_shape(
            Tetromino::Z,
            Rotation::Spawn,
            [(1, 1), (2, 1), (0, 2), (1, 2)],
        );
        assert_shape(Tetromino::Z, Rotation::Cw, [(2, 2), (1, 1), (2, 1), (1, 0)]);
        assert_shape(
            Tetromino::Z,
            Rotation::Flip,
            [(0, 1), (1, 1), (1, 0), (2, 0)],
        );
        assert_shape(
            Tetromino::Z,
            Rotation::Ccw,
            [(1, 2), (0, 1), (1, 1), (0, 0)],
        );
    }

    #[test]
    fn shape_j() {
        assert_shape(
            Tetromino::J,
            Rotation::Spawn,
            [(0, 1), (1, 1), (2, 1), (0, 2)],
        );
        assert_shape(Tetromino::J, Rotation::Cw, [(1, 2), (2, 2), (1, 1), (1, 0)]);
        assert_shape(
            Tetromino::J,
            Rotation::Flip,
            [(0, 1), (1, 1), (2, 1), (2, 0)],
        );
        assert_shape(
            Tetromino::J,
            Rotation::Ccw,
            [(1, 2), (1, 1), (0, 0), (1, 0)],
        );
    }

    #[test]
    fn shape_l() {
        assert_shape(
            Tetromino::L,
            Rotation::Spawn,
            [(0, 1), (1, 1), (2, 1), (2, 2)],
        );
        assert_shape(Tetromino::L, Rotation::Cw, [(1, 2), (1, 1), (1, 0), (2, 0)]);
        assert_shape(
            Tetromino::L,
            Rotation::Flip,
            [(0, 1), (1, 1), (2, 1), (0, 0)],
        );
        assert_shape(
            Tetromino::L,
            Rotation::Ccw,
            [(0, 2), (1, 2), (1, 1), (1, 0)],
        );
    }

    #[test]
    fn rotation_cw_cycles_through_four_states() {
        assert_eq!(Rotation::Spawn.cw(), Rotation::Cw);
        assert_eq!(Rotation::Cw.cw(), Rotation::Flip);
        assert_eq!(Rotation::Flip.cw(), Rotation::Ccw);
        assert_eq!(Rotation::Ccw.cw(), Rotation::Spawn);
    }

    #[test]
    fn rotation_ccw_cycles_through_four_states() {
        assert_eq!(Rotation::Spawn.ccw(), Rotation::Ccw);
        assert_eq!(Rotation::Ccw.ccw(), Rotation::Flip);
        assert_eq!(Rotation::Flip.ccw(), Rotation::Cw);
        assert_eq!(Rotation::Cw.ccw(), Rotation::Spawn);
    }

    #[test]
    fn rotation_cw_then_ccw_is_identity() {
        for rot in [Rotation::Spawn, Rotation::Cw, Rotation::Flip, Rotation::Ccw] {
            assert_eq!(rot.cw().ccw(), rot);
            assert_eq!(rot.ccw().cw(), rot);
        }
    }

    #[test]
    fn spawn_positions_match_spec() {
        // 仕様書 §3.1 の (bx, by) を手書き転記。
        assert_eq!(Tetromino::I.spawn_pos(), (3, 18));
        assert_eq!(Tetromino::O.spawn_pos(), (4, 20));
        assert_eq!(Tetromino::T.spawn_pos(), (3, 19));
        assert_eq!(Tetromino::S.spawn_pos(), (3, 19));
        assert_eq!(Tetromino::Z.spawn_pos(), (3, 19));
        assert_eq!(Tetromino::J.spawn_pos(), (3, 19));
        assert_eq!(Tetromino::L.spawn_pos(), (3, 19));
    }
}
