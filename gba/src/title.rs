//! タイトル画面 (仕様書 §14.1)。
//!
//! - 「RETRIS」ロゴ: 3×5 のブロック文字をミノ色パレットで描く。
//! - 開始レベル 1〜15 を十字キー上下で選択 (エッジ検出は agb の
//!   `is_just_pressed`。アプリ画面の入力は core を通らない)。
//! - 毎 VBlank にフレームカウンタを進め、START 押下時のカウンタ値から
//!   シードを作る (§5.3: Knuth 乗算ハッシュ + `|1` で 0 回避)。

use agb::display::{
    GraphicsFrame, Priority,
    tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileFormat},
};
use agb::input::{Button, ButtonController};
use retris_core::Tetromino;

use crate::font::Font;
use crate::render::{make_block_tile, piece_effect};

/// 選択可能な開始レベル範囲 (§11, §14.1)。
const LEVEL_MIN: u32 = 1;
const LEVEL_MAX: u32 = 15;

/// ロゴの左上 (タイル座標)。文字幅 3 + 隙間 1 で 6 文字 = 23 タイル幅。
const LOGO_X: i32 = 3;
const LOGO_Y: i32 = 2;

/// レベル選択表示: "LEVEL" ラベルと数値・上下矢印。
const LEVEL_LABEL_X: i32 = 10;
const LEVEL_LABEL_Y: i32 = 11;
const LEVEL_VALUE_X: i32 = 16;

/// 「PUSH START」の位置と点滅周期 (フレーム)。
const PUSH_START_X: i32 = 10;
const PUSH_START_Y: i32 = 15;
const BLINK_FRAMES: u8 = 30;

/// §5.3 のシード拡散乗数 (Knuth 乗算ハッシュ)。
const SEED_MULTIPLIER: u32 = 2_654_435_761;

/// ロゴ「RETRIS」の 3×5 ブロック文字 (bit2 が左端)。
const LOGO_LETTERS: [[u8; 5]; 6] = [
    [0b110, 0b101, 0b110, 0b101, 0b101], // R
    [0b111, 0b100, 0b110, 0b100, 0b111], // E
    [0b111, 0b010, 0b010, 0b010, 0b010], // T
    [0b110, 0b101, 0b110, 0b101, 0b101], // R
    [0b111, 0b010, 0b010, 0b010, 0b111], // I
    [0b011, 0b100, 0b010, 0b001, 0b110], // S
];

/// ロゴ各文字の色 (ミノ色パレットの流用)。
const LOGO_COLOURS: [Tetromino; 6] = [
    Tetromino::Z, // 赤
    Tetromino::L, // 橙
    Tetromino::O, // 黄
    Tetromino::S, // 緑
    Tetromino::I, // 水
    Tetromino::J, // 青
];

/// タイトル画面の状態。
pub struct TitleScreen {
    bg: RegularBackground,
    font: Font,
    block_tile: DynamicTile16,
    /// シード収集用フレームカウンタ (§5.3)。
    frame_count: u32,
    /// 選択中の開始レベル。
    level: u32,
    /// 「PUSH START」点滅用の残りフレームと現在の表示状態。
    blink_timer: u8,
    blink_visible: bool,
}

impl TitleScreen {
    /// ロゴ・レベル選択・操作ガイドを初期描画する。
    ///
    /// パレットは [`crate::render::init_palettes`] で登録済みであること。
    pub fn new() -> Self {
        let bg = RegularBackground::new(
            Priority::P0,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );

        let mut title = Self {
            bg,
            font: Font::new(),
            block_tile: make_block_tile(),
            frame_count: 0,
            level: LEVEL_MIN,
            blink_timer: BLINK_FRAMES,
            blink_visible: true,
        };

        title.draw_logo();
        title
            .font
            .write(&mut title.bg, LEVEL_LABEL_X, LEVEL_LABEL_Y, b"LEVEL");
        // レベル数値の上下に選択矢印 (^ / v はフォントの矢印グリフ)。
        title
            .font
            .write(&mut title.bg, LEVEL_VALUE_X, LEVEL_LABEL_Y - 1, b"^");
        title
            .font
            .write(&mut title.bg, LEVEL_VALUE_X, LEVEL_LABEL_Y + 1, b"v");
        title.draw_level();
        title
            .font
            .write(&mut title.bg, PUSH_START_X, PUSH_START_Y, b"PUSH START");
        title
    }

    /// 1 フレーム進める。START 押下フレームで `Some((seed, start_level))` を返す。
    pub fn update(&mut self, input: &ButtonController) -> Option<(u32, u32)> {
        self.frame_count = self.frame_count.wrapping_add(1);

        if input.is_just_pressed(Button::Up) && self.level < LEVEL_MAX {
            self.level += 1;
            self.draw_level();
        }
        if input.is_just_pressed(Button::Down) && self.level > LEVEL_MIN {
            self.level -= 1;
            self.draw_level();
        }

        // 「PUSH START」の点滅。
        self.blink_timer -= 1;
        if self.blink_timer == 0 {
            self.blink_timer = BLINK_FRAMES;
            self.blink_visible = !self.blink_visible;
            let text: &[u8] = if self.blink_visible {
                b"PUSH START"
            } else {
                b"          "
            };
            self.font
                .write(&mut self.bg, PUSH_START_X, PUSH_START_Y, text);
        }

        if input.is_just_pressed(Button::Start) {
            // §5.3: 人間の入力タイミングをエントロピー源にする。
            let seed = self.frame_count.wrapping_mul(SEED_MULTIPLIER) | 1;
            return Some((seed, self.level));
        }
        None
    }

    /// このフレームにタイトル画面を表示する。
    pub fn show(&self, frame: &mut GraphicsFrame<'_>) {
        self.bg.show(frame);
    }

    /// 選択中レベルを左詰め 2 桁で描く (矢印と桁位置を揃えるため左詰め)。
    fn draw_level(&mut self) {
        let text = [
            if self.level >= 10 {
                b'1'
            } else {
                b'0' + (self.level % 10) as u8
            },
            if self.level >= 10 {
                b'0' + (self.level % 10) as u8
            } else {
                b' '
            },
        ];
        self.font
            .write(&mut self.bg, LEVEL_VALUE_X, LEVEL_LABEL_Y, &text);
    }

    /// 「RETRIS」ロゴをブロックタイルで描く。
    fn draw_logo(&mut self) {
        for (i, (letter, &colour)) in LOGO_LETTERS.iter().zip(LOGO_COLOURS.iter()).enumerate() {
            let x0 = LOGO_X + i as i32 * 4;
            for (dy, &row) in letter.iter().enumerate() {
                for dx in 0..3 {
                    if row & (1 << (2 - dx)) != 0 {
                        self.bg.set_tile_dynamic16(
                            (x0 + dx, LOGO_Y + dy as i32),
                            &self.block_tile,
                            piece_effect(colour),
                        );
                    }
                }
            }
        }
    }
}
