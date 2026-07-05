//! プレイフィールド描画 (仕様書 §1.1, §2.3)。
//!
//! 8×8 タイルの通常背景 1 枚で構成する。タイル画素はコード生成 ([`DynamicTile16`])
//! した 3 種 (ブロック / ゴースト輪郭 / 空セル) のみで、ミノ 7 色はタイルごとの
//! パレット切替 (パレット 1..=7) で表現する。
//!
//! 描画は差分方式: 前フレームに描いたフィールド内容を保持し、変化したセルの
//! タイルだけを更新する (通常フレームはアクティブミノ+ゴーストの十数セル)。

use agb::display::{
    Graphics, GraphicsFrame, Palette16, Priority, Rgb15,
    tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileEffect, TileFormat},
};
use retris_core::{ActivePiece, FIELD_WIDTH, Game, Tetromino, VISIBLE_HEIGHT};

/// フィールド左端セル (x=0) のタイル x 座標。
///
/// 画面は 30×20 タイル。左壁 1 + フィールド 10 + 右壁 1 の計 12 列を左寄りに
/// 置き、右側 (タイル x=13..=29, 約 136px) をネクスト 5・ホールド・スコア表示用に
/// 空けておく (後工程の HUD)。可視 20 行は画面の縦 20 タイルを丁度使い切る。
const FIELD_ORIGIN_TX: i32 = 2;
/// 左壁のタイル x 座標。
const WALL_LEFT_TX: i32 = FIELD_ORIGIN_TX - 1;
/// 右壁のタイル x 座標。
const WALL_RIGHT_TX: i32 = FIELD_ORIGIN_TX + FIELD_WIDTH as i32;

// パレット内の色番号割当 (全パレット共通レイアウト)。
/// 基本色 (ミノ色 / 壁のグレー)。
const IDX_BASE: u8 = 1;
/// ハイライト (タイル左上辺)。
const IDX_LIGHT: u8 = 2;
/// 影 (タイル右下辺)。
const IDX_DARK: u8 = 3;
/// フィールド内の空セル地色 (全パレット同色にしてゴースト輪郭の内側にも使う)。
const IDX_FIELD_BG: u8 = 4;
/// 空セルのグリッド線 (パレット 0 のみ)。
const IDX_GRID: u8 = 5;
/// HUD テキスト色 (パレット 0 のみ)。
pub(crate) const IDX_TEXT: u8 = 6;

/// フィールド空セルの地色 (暗い青灰)。
const FIELD_BG_COLOUR: u16 = rgb555(1, 1, 3);
/// 仕様書 §2.3 の GBA RGB555 推奨値。[`Tetromino`] の宣言順 (I, O, T, S, Z, J, L)。
const PIECE_COLOURS: [u16; 7] = [0x7FE0, 0x03FF, 0x4010, 0x03E0, 0x001F, 0x7C00, 0x029F];

/// 5bit チャンネル値から RGB555 を組む (`(B<<10)|(G<<5)|R`、§2.3)。
const fn rgb555(r: u16, g: u16, b: u16) -> u16 {
    (b << 10) | (g << 5) | r
}

/// 各チャンネルを白側へ半分寄せたハイライト色。
const fn lighten(colour: u16) -> u16 {
    let r = colour & 31;
    let g = (colour >> 5) & 31;
    let b = (colour >> 10) & 31;
    rgb555(r + (31 - r) / 2, g + (31 - g) / 2, b + (31 - b) / 2)
}

/// 各チャンネルを半分にした影色。
const fn darken(colour: u16) -> u16 {
    let r = (colour & 31) / 2;
    let g = ((colour >> 5) & 31) / 2;
    let b = ((colour >> 10) & 31) / 2;
    rgb555(r, g, b)
}

/// 基本色 + 自動生成のハイライト/影 + 空セル地色を持つパレットを組む。
const fn block_palette(colour: u16) -> Palette16 {
    let mut colours = [Rgb15(0); 16];
    colours[IDX_BASE as usize] = Rgb15(colour);
    colours[IDX_LIGHT as usize] = Rgb15(lighten(colour));
    colours[IDX_DARK as usize] = Rgb15(darken(colour));
    colours[IDX_FIELD_BG as usize] = Rgb15(FIELD_BG_COLOUR);
    Palette16::new(colours)
}

/// パレット 0: 色 0 = バックドロップ (フィールド外の画面地色)、壁グレー、グリッド線、
/// HUD テキスト。
const fn ui_palette() -> Palette16 {
    let mut palette = block_palette(rgb555(14, 14, 15));
    palette.update_colour(0, Rgb15(rgb555(2, 2, 3)));
    palette.update_colour(IDX_GRID as usize, Rgb15(rgb555(3, 3, 6)));
    palette.update_colour(IDX_TEXT as usize, Rgb15(rgb555(29, 29, 31)));
    palette
}

/// 背景パレット一式。0 = UI (壁・空セル)、1..=7 = 各ミノ色 (§2.3)。
static PALETTES: [Palette16; 8] = [
    ui_palette(),
    block_palette(PIECE_COLOURS[0]),
    block_palette(PIECE_COLOURS[1]),
    block_palette(PIECE_COLOURS[2]),
    block_palette(PIECE_COLOURS[3]),
    block_palette(PIECE_COLOURS[4]),
    block_palette(PIECE_COLOURS[5]),
    block_palette(PIECE_COLOURS[6]),
];

/// 背景パレット一式 ([`PALETTES`]) を登録する。全画面 (フィールド・HUD・タイトル)
/// で共有するため、起動時に一度だけ呼ぶこと。
pub(crate) fn init_palettes(gfx: &mut Graphics<'_>) {
    gfx.set_background_palettes(&PALETTES);
}

/// ミノ種別 → 背景パレット番号 (1..=7)。
const fn palette_of(kind: Tetromino) -> u8 {
    match kind {
        Tetromino::I => 1,
        Tetromino::O => 2,
        Tetromino::T => 3,
        Tetromino::S => 4,
        Tetromino::Z => 5,
        Tetromino::J => 6,
        Tetromino::L => 7,
    }
}

/// ミノ色パレットを選ぶ [`TileEffect`]。
pub(crate) const fn piece_effect(kind: Tetromino) -> TileEffect {
    TileEffect::new(false, false, palette_of(kind))
}

/// パレット 0 (UI 色) を選ぶ [`TileEffect`]。
pub(crate) const fn ui_effect() -> TileEffect {
    TileEffect::new(false, false, 0)
}

/// フィールド 1 セルの表示内容。差分検出のため前フレーム分を保持する。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    /// 空セル。
    Empty,
    /// 固定ブロックまたはアクティブミノ。
    Block(Tetromino),
    /// ゴーストピース (輪郭タイル、§14.2-8)。
    Ghost(Tetromino),
}

/// プレイフィールドの描画状態。
pub struct Renderer {
    bg: RegularBackground,
    /// ベベル付きの塗りつぶしブロック (壁はパレット 0、ミノはパレット 1..=7)。
    block_tile: DynamicTile16,
    /// ミノ色 1px 輪郭 + 空セル地色のゴーストタイル。
    ghost_tile: DynamicTile16,
    /// 空セル (地色 + 右下グリッド線)。
    empty_tile: DynamicTile16,
    /// 前フレームに描画したフィールド内容 (`[y][x]`、フィールド座標 §1.1)。
    drawn: [[Cell; FIELD_WIDTH]; VISIBLE_HEIGHT],
}

impl Renderer {
    /// タイル生成と静的部分 (壁と空フィールド) の初期描画を行う。
    ///
    /// パレットは [`init_palettes`] で登録済みであること。
    pub fn new() -> Self {
        // P1: HUD (P0) より奥に置く。同 Priority だと BG 番号 (show 順) 依存の
        // z 順序になり、HUD レイヤに描く GAME OVER オーバーレイがフィールドの
        // 不透明タイルに隠れるため、明示的に一段下げる。
        let mut bg = RegularBackground::new(
            Priority::P1,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );

        let block_tile = make_block_tile();
        let ghost_tile = make_ghost_tile();
        let empty_tile = make_empty_tile();

        // 左右の壁と空フィールド。可視 20 行で画面の縦を使い切るため床の壁は無い。
        for ty in 0..VISIBLE_HEIGHT as i32 {
            bg.set_tile_dynamic16((WALL_LEFT_TX, ty), &block_tile, ui_effect());
            bg.set_tile_dynamic16((WALL_RIGHT_TX, ty), &block_tile, ui_effect());
            for x in 0..FIELD_WIDTH as i32 {
                bg.set_tile_dynamic16((FIELD_ORIGIN_TX + x, ty), &empty_tile, ui_effect());
            }
        }

        Self {
            bg,
            block_tile,
            ghost_tile,
            empty_tile,
            drawn: [[Cell::Empty; FIELD_WIDTH]; VISIBLE_HEIGHT],
        }
    }

    /// 現在のゲーム状態をタイルマップへ反映する (変化セルのみ更新)。
    pub fn render(&mut self, game: &Game) {
        let mut cells = [[Cell::Empty; FIELD_WIDTH]; VISIBLE_HEIGHT];

        // 固定ブロック (可視領域 y=0..19 のみ、§1.1)。
        for (y, row) in cells.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                if let Some(kind) = game.board().get(x as i8, y as i8) {
                    *cell = Cell::Block(kind);
                }
            }
        }

        // ゴースト → アクティブの順に重ねる (重なったらアクティブ優先)。
        if let Some(ghost) = game.ghost_piece() {
            overlay(&mut cells, &ghost, Cell::Ghost(ghost.kind));
        }
        if let Some(active) = game.active_piece() {
            overlay(&mut cells, active, Cell::Block(active.kind));
        }

        for y in 0..VISIBLE_HEIGHT {
            for x in 0..FIELD_WIDTH {
                let cell = cells[y][x];
                if cell == self.drawn[y][x] {
                    continue;
                }
                // §1.1: 可視セル (x, y) はタイル座標 (x, 19 - y) に描画する。
                let pos = (FIELD_ORIGIN_TX + x as i32, (VISIBLE_HEIGHT - 1 - y) as i32);
                match cell {
                    Cell::Empty => self
                        .bg
                        .set_tile_dynamic16(pos, &self.empty_tile, ui_effect()),
                    Cell::Block(kind) => {
                        self.bg
                            .set_tile_dynamic16(pos, &self.block_tile, piece_effect(kind))
                    }
                    Cell::Ghost(kind) => {
                        self.bg
                            .set_tile_dynamic16(pos, &self.ghost_tile, piece_effect(kind))
                    }
                };
                self.drawn[y][x] = cell;
            }
        }
    }

    /// 盤面全体をグレー (壁と同じパレット 0) で塗り直す。ゲームオーバー演出用 (§14.4)。
    ///
    /// 以後 [`Self::render`] を呼ばないこと (差分キャッシュとずれるため)。
    /// 呼ぶ場面はシーン遷移で作り直すまでの静止表示に限る。
    pub fn greyout(&mut self, game: &Game) {
        for y in 0..VISIBLE_HEIGHT {
            for x in 0..FIELD_WIDTH {
                let pos = (FIELD_ORIGIN_TX + x as i32, (VISIBLE_HEIGHT - 1 - y) as i32);
                if game.board().get(x as i8, y as i8).is_some() {
                    self.bg
                        .set_tile_dynamic16(pos, &self.block_tile, ui_effect());
                } else {
                    self.bg
                        .set_tile_dynamic16(pos, &self.empty_tile, ui_effect());
                }
            }
        }
    }

    /// このフレームに背景を表示する。
    pub fn show(&self, frame: &mut GraphicsFrame<'_>) {
        self.bg.show(frame);
    }
}

/// ミノの 4 セルを可視領域 (y=0..19) の範囲でだけ `cells` に書き込む。
/// y>=20 (バッファ領域) のセルは描画しない (§1.1)。
fn overlay(cells: &mut [[Cell; FIELD_WIDTH]; VISIBLE_HEIGHT], piece: &ActivePiece, value: Cell) {
    for (x, y) in piece.cells() {
        if (0..FIELD_WIDTH as i8).contains(&x) && (0..VISIBLE_HEIGHT as i8).contains(&y) {
            cells[y as usize][x as usize] = value;
        }
    }
}

/// ベベル付き塗りつぶしブロック: 左上辺ハイライト・右下辺影・中央は基本色。
/// HUD のミノプレビューにも同じ形状を使う。
pub(crate) fn make_block_tile() -> DynamicTile16 {
    let mut tile = DynamicTile16::new().fill_with(IDX_BASE);
    for i in 0..8 {
        tile.set_pixel(i, 0, IDX_LIGHT);
        tile.set_pixel(0, i, IDX_LIGHT);
    }
    for i in 0..8 {
        tile.set_pixel(i, 7, IDX_DARK);
        tile.set_pixel(7, i, IDX_DARK);
    }
    tile
}

/// ゴースト: ミノ色 1px の輪郭。内側は空セル地色 (IDX_FIELD_BG は全パレット同色)。
fn make_ghost_tile() -> DynamicTile16 {
    let mut tile = DynamicTile16::new().fill_with(IDX_FIELD_BG);
    for i in 0..8 {
        tile.set_pixel(i, 0, IDX_BASE);
        tile.set_pixel(i, 7, IDX_BASE);
        tile.set_pixel(0, i, IDX_BASE);
        tile.set_pixel(7, i, IDX_BASE);
    }
    tile
}

/// 空セル: 地色 + 右辺/下辺の薄いグリッド線。
fn make_empty_tile() -> DynamicTile16 {
    let mut tile = DynamicTile16::new().fill_with(IDX_FIELD_BG);
    for i in 0..8 {
        tile.set_pixel(i, 7, IDX_GRID);
        tile.set_pixel(7, i, IDX_GRID);
    }
    tile
}
