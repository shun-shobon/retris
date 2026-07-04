//! ミノの移動・重力・落下の物理 (仕様書 §7, §15)。
//!
//! ロックディレイ (§8)・ロック処理・ライン消去は含まない。

use crate::active::ActivePiece;
use crate::board::Board;

/// ソフトドロップ中の重力倍率 (仕様書 §7.3, §15)。
pub const SOFT_DROP_FACTOR: u32 = 20;

/// Q16 固定小数点の 1 行分 (仕様書 §7.2)。
const Q16_ONE: u32 = 1 << 16;

/// レベル 1〜20 の重力値 G (Q16) (仕様書 §7.2, §15 からの手書き転記)。
const GRAVITY_TABLE: [u32; 20] = [
    1_097, 1_384, 1_776, 2_321, 3_089, 4_188, 5_785, 8_143, 11_687, 17_103, 25_530, 38_884, 60_441,
    95_915, 155_442, 257_345, 435_384, 752_985, 1_331_709, 2_409_332,
];

/// ミノを左右に 1 列移動する (仕様書 §12.1 の左右移動)。
///
/// `dx` は移動方向 (`-1` = 左, `+1` = 右)。移動先が壁・床・既存ブロックと
/// 衝突する場合は `None` (位置は変更しない)。
#[must_use]
pub fn try_shift(board: &Board, piece: &ActivePiece, dx: i8) -> Option<ActivePiece> {
    let moved = ActivePiece {
        x: piece.x + dx,
        ..*piece
    };
    board.fits(&moved).then_some(moved)
}

/// ミノを 1 行落下させる (仕様書 §7)。
///
/// 1 行下が壁・床・既存ブロックと衝突する場合 (= 接地) は `None`。
#[must_use]
pub fn try_fall(board: &Board, piece: &ActivePiece) -> Option<ActivePiece> {
    let fallen = ActivePiece {
        y: piece.y - 1,
        ..*piece
    };
    board.fits(&fallen).then_some(fallen)
}

/// ミノが接地しているか (1 行下に移動できないか) (仕様書 §0)。
#[must_use]
pub fn is_grounded(board: &Board, piece: &ActivePiece) -> bool {
    try_fall(board, piece).is_none()
}

/// ゴーストピース位置 = 現在位置から純落下させた最終位置 (仕様書 §14.2-8)。
///
/// ハードドロップ (§7.4) の着地位置と同一。すでに接地している場合は現在位置。
#[must_use]
pub fn ghost(board: &Board, piece: &ActivePiece) -> ActivePiece {
    let mut current = *piece;
    while let Some(fallen) = try_fall(board, &current) {
        current = fallen;
    }
    current
}

/// レベルに対応する重力値 G (Q16 固定小数点、1 行 = 65536) (仕様書 §7.2, §15)。
///
/// レベル 21 以上はレベル 20 の値で頭打ち。レベル 0 は来ない前提だが、
/// 防御的にレベル 1 として扱う。
#[must_use]
pub fn gravity_q16(level: u32) -> u32 {
    let clamped = level.clamp(1, GRAVITY_TABLE.len() as u32);
    GRAVITY_TABLE[(clamped - 1) as usize]
}

/// 重力アキュムレータ (Q16 固定小数点) (仕様書 §7.2)。
///
/// 呼び出し側 (後工程のゲームループ) の使い方:
/// 毎フレーム `tick(G)` (ソフトドロップ中は `tick(G × SOFT_DROP_FACTOR)`) を呼び、
/// 返った行数だけ [`try_fall`] を繰り返す。途中で接地したら [`Self::reset`] で
/// 0 クリアしてロックディレイ (§8) へ移行する。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GravityAccumulator {
    acc: u32,
}

impl GravityAccumulator {
    /// アキュムレータ 0 で生成する (仕様書 §3.2-4: スポーン時は 0 から開始)。
    #[must_use]
    pub const fn new() -> Self {
        Self { acc: 0 }
    }

    /// 重力値を加算し、今フレーム落下すべき行数を返す。
    ///
    /// 65536 (= 1 行) ごとに 1 行。65536 未満の余りは次フレームへ持ち越す。
    pub fn tick(&mut self, gravity: u32) -> u32 {
        self.acc += gravity;
        let rows = self.acc / Q16_ONE;
        self.acc %= Q16_ONE;
        rows
    }

    /// 接地時に 0 クリアする (仕様書 §7.2)。
    pub fn reset(&mut self) {
        self.acc = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::{Rotation, Tetromino};

    const ALL_KINDS: [Tetromino; 7] = [
        Tetromino::I,
        Tetromino::O,
        Tetromino::T,
        Tetromino::S,
        Tetromino::Z,
        Tetromino::J,
        Tetromino::L,
    ];

    fn piece(kind: Tetromino, rot: Rotation, x: i8, y: i8) -> ActivePiece {
        ActivePiece { kind, rot, x, y }
    }

    /// 指定した行 `y` の全 10 セルを埋める (テスト用の地形構築)。
    fn fill_row(board: &mut Board, y: i8) {
        for x in 0..10 {
            board.set(x, y, Some(Tetromino::J));
        }
    }

    // ---- gravity_q16 ----

    #[test]
    fn gravity_q16_matches_spec_table_for_levels_1_to_20() {
        // 期待値は仕様書 §15 GRAVITY_Q16[1..20] からの手書き転記。
        let expected: [(u32, u32); 20] = [
            (1, 1097),
            (2, 1384),
            (3, 1776),
            (4, 2321),
            (5, 3089),
            (6, 4188),
            (7, 5785),
            (8, 8143),
            (9, 11687),
            (10, 17103),
            (11, 25530),
            (12, 38884),
            (13, 60441),
            (14, 95915),
            (15, 155442),
            (16, 257345),
            (17, 435384),
            (18, 752985),
            (19, 1331709),
            (20, 2409332),
        ];
        for (level, g) in expected {
            assert_eq!(gravity_q16(level), g, "レベル {level} の G が仕様と不一致");
        }
    }

    #[test]
    fn gravity_q16_caps_at_level_20_value() {
        // レベル 21 以上はレベル 20 の値 (仕様書 §7.2)。
        assert_eq!(gravity_q16(21), 2_409_332);
        assert_eq!(gravity_q16(100), 2_409_332);
        assert_eq!(gravity_q16(u32::MAX), 2_409_332);
    }

    #[test]
    fn gravity_q16_treats_level_0_as_level_1() {
        // レベル 0 は来ない前提だが防御的にレベル 1 扱い。
        assert_eq!(gravity_q16(0), 1097);
    }

    // ---- GravityAccumulator ----

    #[test]
    fn level_1_first_drop_happens_on_frame_60() {
        // 仕様書 §7.2 の例: レベル 1 (G=1097) では 59 フレーム目で累計 64,723 < 65,536、
        // 60 フレーム目で 65,820 >= 65,536 となり初回落下。
        let g = gravity_q16(1);
        let mut acc = GravityAccumulator::new();
        for frame in 1..=59 {
            assert_eq!(acc.tick(g), 0, "フレーム {frame} で落下してはならない");
        }
        assert_eq!(acc.tick(g), 1, "60 フレーム目に 1 行落下すべき");
        // 余りは 65,820 - 65,536 = 284。次の落下は 120 フレーム目
        // (284 + 60×1097 = 66,104 >= 65,536)。
        for frame in 61..=119 {
            assert_eq!(acc.tick(g), 0, "フレーム {frame} で落下してはならない");
        }
        assert_eq!(acc.tick(g), 1, "120 フレーム目に 1 行落下すべき");
    }

    #[test]
    fn soft_drop_at_level_1_first_drops_on_frame_3() {
        // レベル 1 の G×20 = 21,940。3 フレーム目に累計 65,820 >= 65,536 (仕様書 §7.3)。
        let g_soft = gravity_q16(1) * SOFT_DROP_FACTOR;
        let mut acc = GravityAccumulator::new();
        assert_eq!(acc.tick(g_soft), 0); // フレーム 1: 21,940
        assert_eq!(acc.tick(g_soft), 0); // フレーム 2: 43,880
        assert_eq!(acc.tick(g_soft), 1); // フレーム 3: 65,820 → 1 行、余り 284
        // 以後もおおむね 3 フレームごとに 1 行 (次の 4 サイクル分を検証)。
        for cycle in 0..4 {
            assert_eq!(acc.tick(g_soft), 0, "サイクル {cycle} の 1 フレーム目");
            assert_eq!(acc.tick(g_soft), 0, "サイクル {cycle} の 2 フレーム目");
            assert_eq!(acc.tick(g_soft), 1, "サイクル {cycle} の 3 フレーム目");
        }
    }

    #[test]
    fn level_20_drops_36_rows_in_one_frame() {
        // レベル 20 の G = 2,409,332 = 36×65,536 + 50,036 → 1 フレームで 36 行。
        let mut acc = GravityAccumulator::new();
        assert_eq!(acc.tick(gravity_q16(20)), 36);
        // 余り 50,036 が保持され、次フレームは 2,459,368 = 37×65,536 + 34,536 → 37 行。
        assert_eq!(acc.tick(gravity_q16(20)), 37);
    }

    #[test]
    fn accumulator_keeps_remainder_across_frames() {
        // 端数がフレームをまたいで蓄積されること (リテラル期待値)。
        // gravity = 30,000 の場合:
        //   フレーム 1: 30,000 → 0 行
        //   フレーム 2: 60,000 → 0 行
        //   フレーム 3: 90,000 → 1 行、余り 24,464
        //   フレーム 4: 54,464 → 0 行
        //   フレーム 5: 84,464 → 1 行、余り 18,928
        //   フレーム 6: 48,928 → 0 行
        //   フレーム 7: 78,928 → 1 行、余り 13,392
        let mut acc = GravityAccumulator::new();
        let results: [u32; 7] = core::array::from_fn(|_| acc.tick(30_000));
        assert_eq!(results, [0, 0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn reset_clears_accumulated_remainder() {
        // 接地時の 0 クリア (仕様書 §7.2)。
        let mut acc = GravityAccumulator::new();
        assert_eq!(acc.tick(60_000), 0); // 余り 60,000
        acc.reset();
        // クリア後は新規と同じ挙動: 60,000 + 60,000 = 120,000 → 1 行。
        assert_eq!(acc.tick(60_000), 0);
        assert_eq!(acc.tick(60_000), 1);
    }

    #[test]
    fn new_and_default_start_at_zero() {
        assert_eq!(GravityAccumulator::new(), GravityAccumulator::default());
        // 0 開始なら gravity=65,535 の初回 tick は 0 行。
        let mut acc = GravityAccumulator::new();
        assert_eq!(acc.tick(65_535), 0);
    }

    // ---- try_shift ----

    #[test]
    fn shift_moves_one_column_in_open_space() {
        let board = Board::new();
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 5);
        assert_eq!(
            try_shift(&board, &t, 1),
            Some(piece(Tetromino::T, Rotation::Spawn, 4, 5))
        );
        assert_eq!(
            try_shift(&board, &t, -1),
            Some(piece(Tetromino::T, Rotation::Spawn, 2, 5))
        );
    }

    #[test]
    fn shift_blocked_by_walls() {
        let board = Board::new();
        // T 状態 0 はボックス内 x=0..2 を占有: bx=0 で左壁、bx=7 で右壁に密着。
        let left = piece(Tetromino::T, Rotation::Spawn, 0, 5);
        assert_eq!(try_shift(&board, &left, -1), None);
        let right = piece(Tetromino::T, Rotation::Spawn, 7, 5);
        assert_eq!(try_shift(&board, &right, 1), None);
        // I 状態 0 (幅 4): bx=0 で左壁、bx=6 で右壁に密着。
        let left = piece(Tetromino::I, Rotation::Spawn, 0, 5);
        assert_eq!(try_shift(&board, &left, -1), None);
        let right = piece(Tetromino::I, Rotation::Spawn, 6, 5);
        assert_eq!(try_shift(&board, &right, 1), None);
    }

    #[test]
    fn shift_blocked_by_existing_block() {
        let mut board = Board::new();
        // T 状態 0 @ (3,5) の占有セルは (3,6) (4,6) (5,6) (4,7)。
        // 右移動後の (6,6) にブロックを置くと右は不可、左は可。
        board.set(6, 6, Some(Tetromino::L));
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 5);
        assert_eq!(try_shift(&board, &t, 1), None);
        assert_eq!(
            try_shift(&board, &t, -1),
            Some(piece(Tetromino::T, Rotation::Spawn, 2, 5))
        );
    }

    #[test]
    fn shift_preserves_kind_and_rotation() {
        let board = Board::new();
        let z = piece(Tetromino::Z, Rotation::Cw, 4, 10);
        let shifted = try_shift(&board, &z, 1).expect("空間では移動できるはず");
        assert_eq!(shifted.kind, Tetromino::Z);
        assert_eq!(shifted.rot, Rotation::Cw);
        assert_eq!((shifted.x, shifted.y), (5, 10));
    }

    // ---- try_fall / is_grounded ----

    #[test]
    fn fall_moves_one_row_down_in_open_space() {
        let board = Board::new();
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 5);
        assert_eq!(
            try_fall(&board, &t),
            Some(piece(Tetromino::T, Rotation::Spawn, 3, 4))
        );
        assert!(!is_grounded(&board, &t));
    }

    #[test]
    fn fall_blocked_by_floor() {
        let board = Board::new();
        // T 状態 0 の最下段セルは cy=1 なので by=-1 が床上の最終位置。
        let t = piece(Tetromino::T, Rotation::Spawn, 3, -1);
        assert_eq!(try_fall(&board, &t), None);
        assert!(is_grounded(&board, &t));
        // 1 行上ならまだ落下できる。
        let above = piece(Tetromino::T, Rotation::Spawn, 3, 0);
        assert_eq!(try_fall(&board, &above), Some(t));
        assert!(!is_grounded(&board, &above));
    }

    #[test]
    fn fall_blocked_by_stack() {
        let mut board = Board::new();
        // y=3 を埋める。T 状態 0 の最下段は cy=1 なので by=3 (実セル y=4) で接地。
        fill_row(&mut board, 3);
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 3);
        assert_eq!(try_fall(&board, &t), None);
        assert!(is_grounded(&board, &t));
        let above = piece(Tetromino::T, Rotation::Spawn, 3, 4);
        assert_eq!(try_fall(&board, &above), Some(t));
    }

    #[test]
    fn is_grounded_agrees_with_try_fall_for_all_kinds() {
        let mut board = Board::new();
        fill_row(&mut board, 0);
        board.set(5, 1, Some(Tetromino::S));
        for kind in ALL_KINDS {
            for y in -2..8 {
                let p = piece(kind, Rotation::Spawn, 3, y);
                if !board.fits(&p) {
                    continue;
                }
                assert_eq!(
                    is_grounded(&board, &p),
                    try_fall(&board, &p).is_none(),
                    "{kind:?} @ y={y} で is_grounded と try_fall が不整合"
                );
            }
        }
    }

    // ---- ghost ----

    #[test]
    fn ghost_reaches_floor_on_empty_board() {
        let board = Board::new();
        for kind in ALL_KINDS {
            let spawned = ActivePiece::spawn(kind);
            let g = ghost(&board, &spawned);
            // 最下段セルの cy が床上の最終 by を決める:
            // I 状態 0 は cy=2 → by=-2、O は cy=0 → by=0、他は cy=1 → by=-1。
            let expected_y = match kind {
                Tetromino::I => -2,
                Tetromino::O => 0,
                _ => -1,
            };
            assert_eq!(
                (g.x, g.y),
                (spawned.x, expected_y),
                "{kind:?} のゴースト位置"
            );
            assert_eq!(g.kind, kind);
            assert_eq!(g.rot, spawned.rot);
        }
    }

    #[test]
    fn ghost_stops_on_stack() {
        let mut board = Board::new();
        // y=5 まで積んだスタック。T 状態 0 の最下段は cy=1 なので by=5 (実セル y=6) で停止。
        for y in 0..=5 {
            fill_row(&mut board, y);
        }
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 19);
        assert_eq!(
            ghost(&board, &t),
            piece(Tetromino::T, Rotation::Spawn, 3, 5)
        );
    }

    #[test]
    fn ghost_stops_on_partial_stack_column() {
        let mut board = Board::new();
        // (4,10) だけ埋まった地形。T 状態 0 @ bx=3 は x=3..5 を占有するため
        // 実セル最下段 y=11 → by=10 で停止 (両脇は空でも柱で止まる)。
        board.set(4, 10, Some(Tetromino::I));
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 19);
        assert_eq!(
            ghost(&board, &t),
            piece(Tetromino::T, Rotation::Spawn, 3, 10)
        );
        // 柱に重ならない位置 (bx=6, x=6..8) なら床まで落ちる。
        let t = piece(Tetromino::T, Rotation::Spawn, 6, 19);
        assert_eq!(
            ghost(&board, &t),
            piece(Tetromino::T, Rotation::Spawn, 6, -1)
        );
    }

    #[test]
    fn ghost_of_grounded_piece_is_itself() {
        let mut board = Board::new();
        fill_row(&mut board, 0);
        let t = piece(Tetromino::T, Rotation::Spawn, 3, 0);
        assert!(is_grounded(&board, &t));
        assert_eq!(ghost(&board, &t), t);
    }

    #[test]
    fn ghost_fits_and_one_below_collides_for_all_kinds_and_terrains() {
        // プロパティ検証: ゴースト位置は必ず fits し、その 1 行下は必ず衝突する。
        // 地形 0: 空盤面
        let empty = Board::new();
        // 地形 1: 平坦なスタック (y=0..=2)
        let mut flat = Board::new();
        for y in 0..=2 {
            fill_row(&mut flat, y);
        }
        // 地形 2: 階段状 (列 x が高いほど高い)
        let mut stairs = Board::new();
        for x in 0..10 {
            for y in 0..=(x / 2) {
                stairs.set(x, y, Some(Tetromino::L));
            }
        }
        // 地形 3: 中央の柱 + 穴あき行
        let mut jagged = Board::new();
        board_column(&mut jagged, 4, 8);
        board_column(&mut jagged, 5, 6);
        for x in [0, 1, 2, 7, 9] {
            jagged.set(x, 2, Some(Tetromino::Z));
        }

        for board in [&empty, &flat, &stairs, &jagged] {
            for kind in ALL_KINDS {
                for rot in [Rotation::Spawn, Rotation::Cw, Rotation::Flip, Rotation::Ccw] {
                    for x in -2..10 {
                        let p = piece(kind, rot, x, 19);
                        if !board.fits(&p) {
                            continue;
                        }
                        let g = ghost(board, &p);
                        assert!(
                            board.fits(&g),
                            "{kind:?} {rot:?} @ x={x} のゴーストが衝突している"
                        );
                        let below = ActivePiece { y: g.y - 1, ..g };
                        assert!(
                            !board.fits(&below),
                            "{kind:?} {rot:?} @ x={x} のゴースト 1 行下が空いている"
                        );
                        assert!(g.y <= p.y, "ゴーストが元位置より上にある");
                        assert_eq!((g.kind, g.rot, g.x), (p.kind, p.rot, p.x));
                    }
                }
            }
        }
    }

    /// 列 `x` を y=0..height まで埋める (テスト用の地形構築)。
    fn board_column(board: &mut Board, x: i8, height: i8) {
        for y in 0..height {
            board.set(x, y, Some(Tetromino::I));
        }
    }
}
