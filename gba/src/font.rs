//! 自作 5×7 ピクセルフォント (コード生成の [`DynamicTile16`])。
//!
//! HUD・タイトル・ゲームオーバー表示で共有する。パレット 0 の
//! [`crate::render::IDX_TEXT`] (白) で描画し、背景は透過。
//! `^` / `v` は上下矢印グリフ (レベル選択表示用)。

use agb::display::tiled::{DynamicTile16, RegularBackground};

use crate::render::{IDX_TEXT, ui_effect};

/// 収録グリフ (この順で [`GLYPH_BITMAPS`] と対応)。先頭 10 個は数字 0..=9。
const GLYPH_CHARS: &[u8; 31] = b"0123456789ABCDEGHILMNOPRSTUVX^v";

/// 5×7 ビットマップ。各行の bit4 が左端ピクセル。8 行目 (最下行) は空き。
const GLYPH_BITMAPS: [[u8; 7]; 31] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ], // 0
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // 1
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ], // 2
    [
        0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
    ], // 3
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ], // 4
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ], // 5
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ], // 6
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ], // 7
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ], // 8
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ], // 9
    [
        0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
    ], // A
    [
        0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
    ], // B
    [
        0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
    ], // C
    [
        0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
    ], // D
    [
        0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
    ], // E
    [
        0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
    ], // G
    [
        0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
    ], // H
    [
        0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // I
    [
        0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
    ], // L
    [
        0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
    ], // M
    [
        0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
    ], // N
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ], // O
    [
        0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
    ], // P
    [
        0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
    ], // R
    [
        0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
    ], // S
    [
        0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
    ], // T
    [
        0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ], // U
    [
        0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
    ], // V
    [
        0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
    ], // X
    [
        0b00000, 0b00000, 0b00100, 0b01110, 0b11111, 0b00000, 0b00000,
    ], // ^ (上矢印)
    [
        0b00000, 0b00000, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000,
    ], // v (下矢印)
];

/// グリフ文字 → [`GLYPH_BITMAPS`] の添字。
fn glyph_index(c: u8) -> Option<usize> {
    GLYPH_CHARS.iter().position(|&g| g == c)
}

/// フォント一式。生成時に全グリフを VRAM 上のタイルとして展開する。
pub struct Font {
    glyphs: [DynamicTile16; GLYPH_CHARS.len()],
    /// 全透過タイル (空白・クリア用)。
    blank: DynamicTile16,
}

impl Font {
    pub fn new() -> Self {
        Self {
            glyphs: core::array::from_fn(|i| make_glyph_tile(&GLYPH_BITMAPS[i])),
            blank: DynamicTile16::new().fill_with(0),
        }
    }

    /// 全透過タイル。文字以外のクリア用途にも使える。
    pub fn blank(&self) -> &DynamicTile16 {
        &self.blank
    }

    /// グリフ文字列を描く。空白と未収録文字は透過タイルになる。
    pub fn write(&self, bg: &mut RegularBackground, x: i32, y: i32, text: &[u8]) {
        for (i, &c) in text.iter().enumerate() {
            let pos = (x + i as i32, y);
            match glyph_index(c) {
                Some(g) => bg.set_tile_dynamic16(pos, &self.glyphs[g], ui_effect()),
                None => bg.set_tile_dynamic16(pos, &self.blank, ui_effect()),
            };
        }
    }

    /// 右詰め・先頭空白の 10 進数を描く。`width` 桁を超える値は 9 埋めで飽和。
    pub fn write_number(&self, bg: &mut RegularBackground, x: i32, y: i32, width: u32, value: u32) {
        let max = 10u32.pow(width) - 1;
        let mut rest = value.min(max);
        for i in 0..width {
            let pos = (x + (width - 1 - i) as i32, y);
            if i == 0 || rest > 0 {
                let digit = (rest % 10) as usize;
                bg.set_tile_dynamic16(pos, &self.glyphs[digit], ui_effect());
                rest /= 10;
            } else {
                bg.set_tile_dynamic16(pos, &self.blank, ui_effect());
            }
        }
    }
}

/// 5×7 ビットマップからテキストタイルを生成する (背景は透過)。
fn make_glyph_tile(bitmap: &[u8; 7]) -> DynamicTile16 {
    let mut tile = DynamicTile16::new().fill_with(0);
    for (y, &row) in bitmap.iter().enumerate() {
        for x in 0..5 {
            if row & (1 << (4 - x)) != 0 {
                tile.set_pixel(x, y, IDX_TEXT);
            }
        }
    }
    tile
}
