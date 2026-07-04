//! SRS (スーパーローテーションシステム) の回転処理 (仕様書 §4)。

use crate::active::ActivePiece;
use crate::board::Board;
use crate::piece::{Rotation, Tetromino};

/// 回転方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateDir {
    /// 時計回り (CW)。
    Cw,
    /// 反時計回り (CCW)。
    Ccw,
}

impl RotateDir {
    /// この方向へ 90° 回した後の回転状態。
    const fn apply(self, rot: Rotation) -> Rotation {
        match self {
            Self::Cw => rot.cw(),
            Self::Ccw => rot.ccw(),
        }
    }
}

/// 回転試行の結果 (仕様書 §4, §4.3, §4.4)。
///
/// 3 値を区別する: 成功はロックディレイのリセット対象、不発と O 無視は対象外 (§8.2-6)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateOutcome {
    /// 回転成功。`kick` は成立したキックテスト番号 (1〜5)。
    /// T-Spin 判定 (§10.3 のキック 5 例外) で参照する。
    Rotated {
        /// 回転後のミノ。
        piece: ActivePiece,
        /// 成立したキックテスト番号 (1〜5)。
        kick: u8,
    },
    /// 5 テストすべて衝突 (回転不発)。状態・位置とも変更なし。
    Blocked,
    /// O ミノへの回転入力 (§4.4)。状態・位置とも不変で、成功とも不発とも扱わない。
    IgnoredO,
}

/// J, L, S, T, Z 用キックテーブル (仕様書 §4.1)。行番号は [`transition_index`]。
const JLSTZ_KICKS: [[(i8, i8); 5]; 8] = [
    // 0→R
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
    // 0→L
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
    // R→2
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    // R→0
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    // 2→L
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
    // 2→R
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
    // L→0
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
    // L→2
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
];

/// I 用キックテーブル (仕様書 §4.2)。行番号は [`transition_index`]。
const I_KICKS: [[(i8, i8); 5]; 8] = [
    // 0→R
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    // 0→L
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    // R→2
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    // R→0
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    // 2→L
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    // 2→R
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    // L→0
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    // L→2
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
];

/// キックテーブルの行番号: 遷移を (回転前状態, 回転方向) で引く。
const fn transition_index(from: Rotation, dir: RotateDir) -> usize {
    from as usize * 2 + dir as usize
}

/// 遷移 `(from, dir)` に対するキックオフセット (テスト 1〜5 の `(dx, dy)`)。
///
/// O は [`try_rotate`] が先に処理するため、ここには到達しない。
const fn kicks(kind: Tetromino, from: Rotation, dir: RotateDir) -> &'static [(i8, i8); 5] {
    let table = match kind {
        Tetromino::I => &I_KICKS,
        _ => &JLSTZ_KICKS,
    };
    &table[transition_index(from, dir)]
}

/// SRS に従って回転を試行する (仕様書 §4)。
///
/// 回転状態を差し替えた上で、キックテスト 1 (0,0) から順に `(bx+dx, by+dy)` を
/// 衝突判定し、最初に収まった位置で確定する。`piece` 自体は変更しない。
#[must_use]
pub fn try_rotate(board: &Board, piece: &ActivePiece, dir: RotateDir) -> RotateOutcome {
    // O は回転しても形・位置が変わらない (§4.4)。キックも行わない。
    if matches!(piece.kind, Tetromino::O) {
        return RotateOutcome::IgnoredO;
    }

    let rot = dir.apply(piece.rot);
    let offsets = kicks(piece.kind, piece.rot, dir);
    for (kick, &(dx, dy)) in (1u8..).zip(offsets) {
        let candidate = ActivePiece {
            rot,
            x: piece.x + dx,
            y: piece.y + dy,
            ..*piece
        };
        if board.fits(&candidate) {
            return RotateOutcome::Rotated {
                piece: candidate,
                kick,
            };
        }
    }
    RotateOutcome::Blocked
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ROTATIONS: [Rotation; 4] =
        [Rotation::Spawn, Rotation::Cw, Rotation::Flip, Rotation::Ccw];
    const BOTH_DIRS: [RotateDir; 2] = [RotateDir::Cw, RotateDir::Ccw];

    /// (回転前状態, 回転方向, テスト 1〜5 のオフセット)。
    type KickRow = (Rotation, RotateDir, [(i8, i8); 5]);

    // 仕様書 §4.1 からの手書き転記。行順も仕様書の表と同じ
    // (0→R, R→0, R→2, 2→R, 2→L, L→2, L→0, 0→L)。
    const JLSTZ_EXPECTED: [KickRow; 8] = [
        // 0→R
        (
            Rotation::Spawn,
            RotateDir::Cw,
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        ),
        // R→0
        (
            Rotation::Cw,
            RotateDir::Ccw,
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
        ),
        // R→2
        (
            Rotation::Cw,
            RotateDir::Cw,
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
        ),
        // 2→R
        (
            Rotation::Flip,
            RotateDir::Ccw,
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        ),
        // 2→L
        (
            Rotation::Flip,
            RotateDir::Cw,
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        ),
        // L→2
        (
            Rotation::Ccw,
            RotateDir::Ccw,
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        ),
        // L→0
        (
            Rotation::Ccw,
            RotateDir::Cw,
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        ),
        // 0→L
        (
            Rotation::Spawn,
            RotateDir::Ccw,
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        ),
    ];

    // 仕様書 §4.2 からの手書き転記。行順は §4.1 と同じ。
    const I_EXPECTED: [KickRow; 8] = [
        // 0→R
        (
            Rotation::Spawn,
            RotateDir::Cw,
            [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
        ),
        // R→0
        (
            Rotation::Cw,
            RotateDir::Ccw,
            [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
        ),
        // R→2
        (
            Rotation::Cw,
            RotateDir::Cw,
            [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
        ),
        // 2→R
        (
            Rotation::Flip,
            RotateDir::Ccw,
            [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
        ),
        // 2→L
        (
            Rotation::Flip,
            RotateDir::Cw,
            [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
        ),
        // L→2
        (
            Rotation::Ccw,
            RotateDir::Ccw,
            [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
        ),
        // L→0
        (
            Rotation::Ccw,
            RotateDir::Cw,
            [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
        ),
        // 0→L
        (
            Rotation::Spawn,
            RotateDir::Ccw,
            [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
        ),
    ];

    #[test]
    fn jlstz_kick_table_matches_spec() {
        // J, L, S, T, Z は全キンドが同一テーブル (§4.1) を使う。
        for kind in [
            Tetromino::J,
            Tetromino::L,
            Tetromino::S,
            Tetromino::T,
            Tetromino::Z,
        ] {
            for (from, dir, expected) in JLSTZ_EXPECTED {
                assert_eq!(
                    *kicks(kind, from, dir),
                    expected,
                    "{kind:?} の {from:?} + {dir:?} が仕様 §4.1 と不一致"
                );
            }
        }
    }

    #[test]
    fn i_kick_table_matches_spec() {
        for (from, dir, expected) in I_EXPECTED {
            assert_eq!(
                *kicks(Tetromino::I, from, dir),
                expected,
                "I の {from:?} + {dir:?} が仕様 §4.2 と不一致"
            );
        }
    }

    #[test]
    fn reverse_transition_is_sign_flipped() {
        // 検算用の性質 (§4.2 末尾): 逆遷移のテーブルは符号反転。
        // T (JLSTZ 表) と I (I 表) の両テーブル・全 8 遷移で確認する。
        for kind in [Tetromino::T, Tetromino::I] {
            for from in ALL_ROTATIONS {
                for dir in BOTH_DIRS {
                    let to = dir.apply(from);
                    let back = match dir {
                        RotateDir::Cw => RotateDir::Ccw,
                        RotateDir::Ccw => RotateDir::Cw,
                    };
                    let forward = kicks(kind, from, dir);
                    let reverse = kicks(kind, to, back);
                    for i in 0..5 {
                        assert_eq!(
                            (reverse[i].0, reverse[i].1),
                            (-forward[i].0, -forward[i].1),
                            "{kind:?} の {from:?}→{to:?} テスト {} が符号反転で戻らない",
                            i + 1,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn open_space_rotation_uses_test1() {
        // 広い空間では全ミノ・全遷移がテスト 1 (0,0) で成功し、位置は不変・状態だけ進む。
        let board = Board::new();
        for kind in [
            Tetromino::I,
            Tetromino::T,
            Tetromino::S,
            Tetromino::Z,
            Tetromino::J,
            Tetromino::L,
        ] {
            for from in ALL_ROTATIONS {
                for dir in BOTH_DIRS {
                    let piece = ActivePiece {
                        kind,
                        rot: from,
                        x: 3,
                        y: 15,
                    };
                    let outcome = try_rotate(&board, &piece, dir);
                    assert_eq!(
                        outcome,
                        RotateOutcome::Rotated {
                            piece: ActivePiece {
                                rot: dir.apply(from),
                                ..piece
                            },
                            kick: 1,
                        },
                        "{kind:?} の {from:?} + {dir:?} が広い空間でテスト 1 成功にならない"
                    );
                }
            }
        }
    }

    #[test]
    fn t_kicks_off_left_wall() {
        // 左壁際: T の R 状態 (軸列 cx=1) を bx=-1 に置くと実セルは x=0..1 に収まる。
        let board = Board::new();
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Cw,
            x: -1,
            y: 5,
        };
        assert!(board.fits(&piece), "前提: 回転前の T が左壁際に収まる");
        // CW 回転 (R→2)。テスト 1 (0,0) は左壁 (x=-1) に食い込み、テスト 2 (+1,0) で成立。
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Cw),
            RotateOutcome::Rotated {
                piece: ActivePiece {
                    rot: Rotation::Flip,
                    x: 0,
                    ..piece
                },
                kick: 2,
            }
        );
    }

    #[test]
    fn i_kicks_off_right_wall() {
        // 右壁際: I の R 状態 (軸列 cx=2) を bx=7 に置くと実セルは x=9 の縦棒。
        let board = Board::new();
        let piece = ActivePiece {
            kind: Tetromino::I,
            rot: Rotation::Cw,
            x: 7,
            y: 5,
        };
        assert!(board.fits(&piece), "前提: 回転前の I が右壁際に収まる");
        // CCW 回転 (R→0)。テスト 1 (0,0) は x=10、テスト 2 (+2,0) は x=12 まで食い込み、
        // テスト 3 (-1,0) で bx=6 (実セル x=6..9) に成立。
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Ccw),
            RotateOutcome::Rotated {
                piece: ActivePiece {
                    rot: Rotation::Spawn,
                    x: 6,
                    ..piece
                },
                kick: 3,
            }
        );
    }

    #[test]
    fn t_floor_kick_uses_test4_drop() {
        // 床上で 2→L 遷移のテスト 4 (0,-2) が使われるケース。
        // 地形 (X=ブロック, T=回転前の T 状態 2):
        //   y=4: . . . . . X . . . .
        //   y=3: . . . . T T T . . .
        //   y=2: . . . . . T X . . .
        //   y=1: . . . . . . . . . .
        //   y=0: . . . . . . . . . .
        // テスト 1 と 3 は (5,4)、テスト 2 は (6,2) で塞がれ、テスト 4 で 2 段落ちて床に成立。
        let mut board = Board::new();
        board.set(5, 4, Some(Tetromino::J));
        board.set(6, 2, Some(Tetromino::J));
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Flip,
            x: 4,
            y: 2,
        };
        assert!(board.fits(&piece), "前提: 回転前の T が収まる");
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Cw),
            RotateOutcome::Rotated {
                piece: ActivePiece {
                    rot: Rotation::Ccw,
                    y: 0,
                    ..piece
                },
                kick: 4,
            }
        );
    }

    #[test]
    fn t_spin_triple_terrain_uses_kick_test5() {
        // T-Spin Triple 型の地形で 0→R 遷移がテスト 5 (-1,-2) で成立するケース (§10.3 の
        // キック 5 例外で重要)。X=ブロック, T=回転前の T (状態 0):
        //   y=4: . . . X T . . . . .   ← (3,4) が庇 (オーバーハング)
        //   y=3: . . . T T T . . . .
        //   y=2: X X X . X X X X X X   ← x=3 のみ空
        //   y=1: X X X . . X X X X X   ← x=3,4 が空
        //   y=0: X X X . X X X X X X   ← x=3 のみ空
        // テスト 1/4 は (4,2)・(4,0)、テスト 2/3 は庇 (3,4) で塞がれ、
        // テスト 5 で bx-1, by-2 のスロット (3,0)(3,1)(4,1)(3,2) に滑り込む。
        let mut board = Board::new();
        for x in 0..10i8 {
            if x != 3 {
                board.set(x, 0, Some(Tetromino::J));
                board.set(x, 2, Some(Tetromino::J));
            }
            if x != 3 && x != 4 {
                board.set(x, 1, Some(Tetromino::J));
            }
        }
        board.set(3, 4, Some(Tetromino::J)); // 庇

        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Spawn,
            x: 3,
            y: 2,
        };
        assert!(board.fits(&piece), "前提: 回転前の T が庇の右に収まる");

        let expected = ActivePiece {
            rot: Rotation::Cw,
            x: 2,
            y: 0,
            ..piece
        };
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Cw),
            RotateOutcome::Rotated {
                piece: expected,
                kick: 5,
            }
        );

        // この位置でロックすれば y=0..2 の 3 ラインが揃う (Triple になる地形であることの確認)。
        let mut after = board.clone();
        after.place(&expected);
        for y in 0..3 {
            for x in 0..10 {
                assert!(
                    after.is_occupied(x, y),
                    "({x}, {y}) が埋まらず Triple にならない"
                );
            }
        }
    }

    #[test]
    fn rotation_blocked_when_all_five_tests_collide() {
        // 0→R の全 5 テストを塞ぐ地形。R 状態の新規セルは rel(1,0) のみなので、
        // rel(1,0)=(4,10) がテスト 1/4、rel(0,2)=(3,12) がテスト 2/3、
        // rel(0,0)=(3,10) がテスト 5 を塞ぐ。
        let mut board = Board::new();
        board.set(4, 10, Some(Tetromino::J));
        board.set(3, 12, Some(Tetromino::J));
        board.set(3, 10, Some(Tetromino::J));
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Spawn,
            x: 3,
            y: 10,
        };
        assert!(board.fits(&piece), "前提: 回転前の T は収まる");
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Cw),
            RotateOutcome::Blocked
        );
    }

    #[test]
    fn o_piece_rotation_is_ignored() {
        // O は回転入力を受けても状態・位置とも不変 (§4.4)。成功 (Rotated) でも
        // 不発 (Blocked) でもない専用の結果を返し、ロックディレイのリセット判断を
        // 呼び出し側で区別できるようにする。
        let board = Board::new();
        let piece = ActivePiece::spawn(Tetromino::O);
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Cw),
            RotateOutcome::IgnoredO
        );
        assert_eq!(
            try_rotate(&board, &piece, RotateDir::Ccw),
            RotateOutcome::IgnoredO
        );
    }
}
