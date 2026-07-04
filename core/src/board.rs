//! フィールド (盤面) の定義 (仕様書 §1)。

use crate::active::ActivePiece;
use crate::piece::Tetromino;

/// フィールドの幅 (列数)。
pub const FIELD_WIDTH: usize = 10;

/// フィールドの高さ (行数)。下 20 行が可視領域、上 20 行がバッファ。
pub const FIELD_HEIGHT: usize = 40;

/// 可視領域の高さ (行数)。
pub const VISIBLE_HEIGHT: usize = 20;

/// 10×40 のフィールド (仕様書 §1)。
///
/// 各セルは空 (`None`) か、固定されたミノの種類 (`Some`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    cells: [[Option<Tetromino>; FIELD_WIDTH]; FIELD_HEIGHT],
}

impl Board {
    /// 空のフィールドを生成する。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cells: [[None; FIELD_WIDTH]; FIELD_HEIGHT],
        }
    }

    /// フィールド内なら `(行, 列)` の配列インデックスを返す。
    fn index(x: i8, y: i8) -> Option<(usize, usize)> {
        if (0..FIELD_WIDTH as i8).contains(&x) && (0..FIELD_HEIGHT as i8).contains(&y) {
            Some((y as usize, x as usize))
        } else {
            None
        }
    }

    /// セルの内容。フィールド外は `None` (占有判定には [`Self::is_occupied`] を使う)。
    #[must_use]
    pub fn get(&self, x: i8, y: i8) -> Option<Tetromino> {
        let (row, col) = Self::index(x, y)?;
        self.cells[row][col]
    }

    /// セルを書き換える。フィールド外への書き込みは何もしない。
    pub fn set(&mut self, x: i8, y: i8, cell: Option<Tetromino>) {
        if let Some((row, col)) = Self::index(x, y) {
            self.cells[row][col] = cell;
        }
    }

    /// セルが占有されているか。フィールド外 (左右壁・床・天井) は占有扱い (仕様書 §1.1)。
    #[must_use]
    pub fn is_occupied(&self, x: i8, y: i8) -> bool {
        match Self::index(x, y) {
            Some((row, col)) => self.cells[row][col].is_some(),
            None => true,
        }
    }

    /// ミノの 4 セルすべてが非占有なら `true`。
    #[must_use]
    pub fn fits(&self, piece: &ActivePiece) -> bool {
        piece.cells().iter().all(|&(x, y)| !self.is_occupied(x, y))
    }

    /// ミノの 4 セルをフィールドに書き込む (固定)。
    pub fn place(&mut self, piece: &ActivePiece) {
        for (x, y) in piece.cells() {
            self.set(x, y, Some(piece.kind));
        }
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active::ActivePiece;
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

    #[test]
    fn field_dimensions_match_spec() {
        assert_eq!(FIELD_WIDTH, 10);
        assert_eq!(FIELD_HEIGHT, 40);
        assert_eq!(VISIBLE_HEIGHT, 20);
    }

    #[test]
    fn new_board_is_empty() {
        let board = Board::new();
        for y in 0..40 {
            for x in 0..10 {
                assert_eq!(board.get(x, y), None, "({x}, {y}) が空でない");
                assert!(!board.is_occupied(x, y), "({x}, {y}) が占有扱い");
            }
        }
    }

    #[test]
    fn outside_field_is_occupied() {
        let board = Board::new();
        // 左右壁
        assert!(board.is_occupied(-1, 0));
        assert!(board.is_occupied(10, 0));
        // 床
        assert!(board.is_occupied(0, -1));
        assert!(board.is_occupied(5, -1));
        // 天井 (y > 39)
        assert!(board.is_occupied(0, 40));
        assert!(board.is_occupied(5, 100));
        // 角の外
        assert!(board.is_occupied(-1, -1));
        assert!(board.is_occupied(10, 40));
    }

    #[test]
    fn get_outside_field_is_none() {
        let board = Board::new();
        assert_eq!(board.get(-1, 0), None);
        assert_eq!(board.get(10, 0), None);
        assert_eq!(board.get(0, -1), None);
        assert_eq!(board.get(0, 40), None);
    }

    #[test]
    fn set_get_roundtrip() {
        let mut board = Board::new();
        board.set(3, 5, Some(Tetromino::T));
        assert_eq!(board.get(3, 5), Some(Tetromino::T));
        assert!(board.is_occupied(3, 5));
        // 周囲は変化しない
        assert_eq!(board.get(2, 5), None);
        assert_eq!(board.get(3, 4), None);
        // クリアも往復できる
        board.set(3, 5, None);
        assert_eq!(board.get(3, 5), None);
        assert!(!board.is_occupied(3, 5));
    }

    #[test]
    fn set_works_at_field_corners() {
        let mut board = Board::new();
        board.set(0, 0, Some(Tetromino::I));
        board.set(9, 39, Some(Tetromino::L));
        assert_eq!(board.get(0, 0), Some(Tetromino::I));
        assert_eq!(board.get(9, 39), Some(Tetromino::L));
    }

    #[test]
    fn all_spawn_pieces_fit_on_empty_board() {
        let board = Board::new();
        for kind in ALL_KINDS {
            assert!(
                board.fits(&ActivePiece::spawn(kind)),
                "{kind:?} が空盤面でスポーンできない"
            );
        }
    }

    #[test]
    fn fits_rejects_left_wall_collision() {
        let board = Board::new();
        // T 状態 0 はボックス内 (0,1) を含むので bx=-1 だと x=-1 に食い込む。
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Spawn,
            x: -1,
            y: 5,
        };
        assert!(!board.fits(&piece));
        // bx=0 なら収まる。
        let piece = ActivePiece { x: 0, ..piece };
        assert!(board.fits(&piece));
    }

    #[test]
    fn fits_rejects_right_wall_collision() {
        let board = Board::new();
        // T 状態 0 はボックス内 (2,1) を含むので bx=8 だと x=10 に食い込む。
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Spawn,
            x: 8,
            y: 5,
        };
        assert!(!board.fits(&piece));
        // bx=7 なら右端 x=9 に収まる。
        let piece = ActivePiece { x: 7, ..piece };
        assert!(board.fits(&piece));
    }

    #[test]
    fn fits_rejects_floor_collision() {
        let board = Board::new();
        // T 状態 0 の最下段セルは cy=1 なので by=-1 までは床上、by=-2 で床に食い込む。
        let piece = ActivePiece {
            kind: Tetromino::T,
            rot: Rotation::Spawn,
            x: 3,
            y: -1,
        };
        assert!(board.fits(&piece));
        let piece = ActivePiece { y: -2, ..piece };
        assert!(!board.fits(&piece));
    }

    #[test]
    fn fits_rejects_above_field_top() {
        let board = Board::new();
        // O はボックス内 cy=0..1 を占有。by=38 なら y=39 までに収まり、by=39 だと y=40 に食い込む。
        let piece = ActivePiece {
            kind: Tetromino::O,
            rot: Rotation::Spawn,
            x: 4,
            y: 38,
        };
        assert!(board.fits(&piece));
        let piece = ActivePiece { y: 39, ..piece };
        assert!(!board.fits(&piece));
    }

    #[test]
    fn fits_rejects_existing_block() {
        let mut board = Board::new();
        // T のスポーン占有セル (§3.1) のひとつ (4,20) にブロックを置く。
        board.set(4, 20, Some(Tetromino::J));
        assert!(!board.fits(&ActivePiece::spawn(Tetromino::T)));
        // 占有セル外 (4,22) なら影響しない。
        let mut board = Board::new();
        board.set(4, 22, Some(Tetromino::J));
        assert!(board.fits(&ActivePiece::spawn(Tetromino::T)));
    }

    #[test]
    fn place_writes_piece_kind_to_all_four_cells() {
        let mut board = Board::new();
        board.place(&ActivePiece::spawn(Tetromino::T));
        // T のスポーン占有セル (§3.1 手書き転記)。
        for (x, y) in [(3, 20), (4, 20), (5, 20), (4, 21)] {
            assert_eq!(board.get(x, y), Some(Tetromino::T), "({x}, {y})");
        }
        // 書き込まれたのは 4 セルだけ。
        let occupied = (0..40)
            .flat_map(|y| (0..10).map(move |x| (x, y)))
            .filter(|&(x, y)| board.is_occupied(x, y))
            .count();
        assert_eq!(occupied, 4);
    }
}
