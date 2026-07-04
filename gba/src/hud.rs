//! 右側 HUD: スコア・レベル・消去ライン・ネクスト 5・ホールド (+B2B/コンボ表示)。
//!
//! プレイフィールドとは別の背景レイヤ 1 枚に描く。ラベルと数字は自作 5×7 ピクセル
//! フォント (コード生成の [`DynamicTile16`])、ミノプレビューはフィールドと同じ
//! ブロックタイル+パレット切替。値が変化したフレームだけタイルを更新する。

use agb::display::{
    GraphicsFrame, Priority,
    tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileFormat},
};
use retris_core::{Game, LockEvent, NEXT_COUNT, Rotation, Tetromino};

use crate::render::{IDX_TEXT, make_block_tile, piece_effect, ui_effect};

// ---- レイアウト (タイル座標)。フィールドは x1..=12、HUD は x14 以降 ----

/// 左カラム (ホールド・スコア・レベル・ライン・演出) の左端。
const HUD_X: i32 = 14;
/// "HOLD" ラベルの行。
const HOLD_LABEL_Y: i32 = 0;
/// ホールド枠 (6×4、内側 4×2 がプレビュー) の左上。
const HOLD_FRAME_Y: i32 = 1;
/// "SCORE" ラベルの行と数値行。
const SCORE_LABEL_Y: i32 = 6;
const SCORE_Y: i32 = 7;
const SCORE_DIGITS: u32 = 8;
/// "LEVEL" ラベルの行と数値行。
const LEVEL_LABEL_Y: i32 = 9;
const LEVEL_Y: i32 = 10;
const LEVEL_DIGITS: u32 = 2;
/// "LINES" ラベルの行と数値行。
const LINES_LABEL_Y: i32 = 12;
const LINES_Y: i32 = 13;
const LINES_DIGITS: u32 = 4;
/// B2B / コンボの演出行 (§9.3, §9.4)。ロック時のみ数秒表示。
const FX_B2B_Y: i32 = 15;
const FX_COMBO_Y: i32 = 16;
/// 演出行の表示フレーム数 (約 2 秒)。
const FX_FRAMES: u8 = 120;

/// 右カラム (ネクスト) の左端。プレビューは 4×2 セル。
const NEXT_X: i32 = 24;
const NEXT_LABEL_Y: i32 = 0;
/// ネクスト i 番目のスロット上端 = `NEXT_SLOT_Y0 + i * NEXT_SLOT_PITCH`。
const NEXT_SLOT_Y0: i32 = 2;
const NEXT_SLOT_PITCH: i32 = 3;

// ---- 5×7 ピクセルフォント ----

/// 収録グリフ (この順で `GLYPH_BITMAPS` と対応)。先頭 10 個は数字 0..=9。
const GLYPH_CHARS: &[u8; 24] = b"0123456789BCDEHILNORSTVX";

/// 5×7 ビットマップ。各行の bit4 が左端ピクセル。8 行目 (最下行) は空き。
const GLYPH_BITMAPS: [[u8; 7]; 24] = [
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
        0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
    ], // H
    [
        0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // I
    [
        0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
    ], // L
    [
        0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
    ], // N
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ], // O
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
        0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
    ], // V
    [
        0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
    ], // X
];

/// グリフ文字 → `GLYPH_BITMAPS` の添字。
fn glyph_index(c: u8) -> Option<usize> {
    GLYPH_CHARS.iter().position(|&g| g == c)
}

/// 前フレームに描画した HUD の値。変化検出用。
#[derive(Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    score: u32,
    level: u32,
    lines: u32,
    hold: Option<Tetromino>,
    next: [Tetromino; NEXT_COUNT],
}

impl Snapshot {
    fn of(game: &Game) -> Self {
        Self {
            score: game.score(),
            level: game.level(),
            lines: game.total_lines(),
            hold: game.hold_piece(),
            next: core::array::from_fn(|i| game.next(i)),
        }
    }
}

/// HUD の描画状態。
pub struct Hud {
    bg: RegularBackground,
    /// 5×7 フォント (順序は [`GLYPH_CHARS`])。
    glyphs: [DynamicTile16; GLYPH_CHARS.len()],
    /// ミノプレビュー・ホールド枠用のブロック。
    block_tile: DynamicTile16,
    /// 全透過タイル (桁・スロットのクリア用)。
    blank_tile: DynamicTile16,
    /// 前フレームに描画した値。`None` なら未描画 (初回に全描画)。
    drawn: Option<Snapshot>,
    /// B2B/コンボ演出行の残り表示フレーム数。
    fx_timer: u8,
}

impl Hud {
    /// タイル生成とラベル・ホールド枠の初期描画を行う。
    ///
    /// パレットは [`crate::render::Renderer::new`] が登録するものを共有するため、
    /// 先に `Renderer::new` を呼んでおくこと。
    pub fn new() -> Self {
        let bg = RegularBackground::new(
            Priority::P0,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );

        let glyphs = core::array::from_fn(|i| make_glyph_tile(&GLYPH_BITMAPS[i]));
        let block_tile = make_block_tile();
        let blank_tile = DynamicTile16::new().fill_with(0);

        let mut hud = Self {
            bg,
            glyphs,
            block_tile,
            blank_tile,
            drawn: None,
            fx_timer: 0,
        };

        hud.draw_text(HUD_X, HOLD_LABEL_Y, b"HOLD");
        hud.draw_text(HUD_X, SCORE_LABEL_Y, b"SCORE");
        hud.draw_text(HUD_X, LEVEL_LABEL_Y, b"LEVEL");
        hud.draw_text(HUD_X, LINES_LABEL_Y, b"LINES");
        hud.draw_text(NEXT_X, NEXT_LABEL_Y, b"NEXT");
        hud.draw_hold_frame();
        hud
    }

    /// ゲーム状態を HUD へ反映する (変化した項目のみ更新)。
    ///
    /// `Game::events()` は update 冒頭でクリアされるため、`Game::update` と同じ
    /// フレーム内に呼ぶこと。
    pub fn update(&mut self, game: &Game) {
        let snapshot = Snapshot::of(game);
        let force = self.drawn.is_none();
        let prev = self.drawn;
        let changed =
            |get: fn(&Snapshot) -> u32| force || prev.is_some_and(|p| get(&p) != get(&snapshot));

        if changed(|s| s.score) {
            self.draw_number(HUD_X, SCORE_Y, SCORE_DIGITS, snapshot.score);
        }
        if changed(|s| s.level) {
            self.draw_number(HUD_X, LEVEL_Y, LEVEL_DIGITS, snapshot.level);
        }
        if changed(|s| s.lines) {
            self.draw_number(HUD_X, LINES_Y, LINES_DIGITS, snapshot.lines);
        }
        if force || prev.is_some_and(|p| p.hold != snapshot.hold) {
            self.draw_preview(HUD_X + 1, HOLD_FRAME_Y + 1, snapshot.hold);
        }
        if force || prev.is_some_and(|p| p.next != snapshot.next) {
            for (i, &kind) in snapshot.next.iter().enumerate() {
                self.draw_preview(
                    NEXT_X,
                    NEXT_SLOT_Y0 + i as i32 * NEXT_SLOT_PITCH,
                    Some(kind),
                );
            }
        }
        self.drawn = Some(snapshot);

        // B2B / コンボ演出 (§9.3, §9.4): ロックしたフレームだけイベントが立つ。
        if let Some(lock) = game.events().locked {
            self.draw_lock_fx(&lock);
        } else if self.fx_timer > 0 {
            self.fx_timer -= 1;
            if self.fx_timer == 0 {
                self.clear_lock_fx();
            }
        }
    }

    /// このフレームに HUD レイヤを表示する。
    pub fn show(&self, frame: &mut GraphicsFrame<'_>) {
        self.bg.show(frame);
    }

    /// ロック演出行を更新する。B2B・コンボ (1 以上) のどちらも無ければ消す。
    fn draw_lock_fx(&mut self, lock: &LockEvent) {
        if lock.b2b_applied {
            self.draw_text(HUD_X, FX_B2B_Y, b"B2B");
        } else {
            self.draw_text(HUD_X, FX_B2B_Y, b"   ");
        }
        if lock.combo >= 1 {
            self.draw_text(HUD_X, FX_COMBO_Y, b"C");
            self.draw_number(HUD_X + 1, FX_COMBO_Y, 2, lock.combo as u32);
        } else {
            self.draw_text(HUD_X, FX_COMBO_Y, b"   ");
        }
        self.fx_timer = if lock.b2b_applied || lock.combo >= 1 {
            FX_FRAMES
        } else {
            0
        };
    }

    /// ロック演出行を消す。
    fn clear_lock_fx(&mut self) {
        self.draw_text(HUD_X, FX_B2B_Y, b"   ");
        self.draw_text(HUD_X, FX_COMBO_Y, b"   ");
    }

    /// ホールド枠 (6×4 の縁) をブロックタイルで描く。
    fn draw_hold_frame(&mut self) {
        for dy in 0..4 {
            for dx in 0..6 {
                if dy == 0 || dy == 3 || dx == 0 || dx == 5 {
                    self.bg.set_tile_dynamic16(
                        (HUD_X + dx, HOLD_FRAME_Y + dy),
                        &self.block_tile,
                        ui_effect(),
                    );
                }
            }
        }
    }

    /// 4×2 セルのスロットにミノプレビューを描く (`None` なら空にする)。
    ///
    /// スポーン向き ([`Rotation::Spawn`]) の形をボックス座標から正規化し、
    /// スロット内で中央寄せする。
    fn draw_preview(&mut self, x0: i32, y0: i32, piece: Option<Tetromino>) {
        for dy in 0..2 {
            for dx in 0..4 {
                self.bg
                    .set_tile_dynamic16((x0 + dx, y0 + dy), &self.blank_tile, ui_effect());
            }
        }
        let Some(kind) = piece else { return };

        let cells = kind.cells(Rotation::Spawn);
        let min_cx = cells.iter().map(|&(cx, _)| cx).min().unwrap_or(0);
        let max_cx = cells.iter().map(|&(cx, _)| cx).max().unwrap_or(0);
        let max_cy = cells.iter().map(|&(_, cy)| cy).max().unwrap_or(0);
        let width = i32::from(max_cx - min_cx) + 1;
        let x_offset = (4 - width + 1) / 2;

        for (cx, cy) in cells {
            let pos = (
                x0 + x_offset + i32::from(cx - min_cx),
                y0 + i32::from(max_cy - cy),
            );
            self.bg
                .set_tile_dynamic16(pos, &self.block_tile, piece_effect(kind));
        }
    }

    /// 右詰め・先頭空白の 10 進数を描く。`width` 桁を超える値は 9 埋めで飽和。
    fn draw_number(&mut self, x: i32, y: i32, width: u32, value: u32) {
        let max = 10u32.pow(width) - 1;
        let mut rest = value.min(max);
        for i in 0..width {
            let pos = (x + (width - 1 - i) as i32, y);
            if i == 0 || rest > 0 {
                let digit = (rest % 10) as usize;
                self.bg
                    .set_tile_dynamic16(pos, &self.glyphs[digit], ui_effect());
                rest /= 10;
            } else {
                self.bg
                    .set_tile_dynamic16(pos, &self.blank_tile, ui_effect());
            }
        }
    }

    /// グリフ文字列を描く。空白と未収録文字は透過タイルになる。
    fn draw_text(&mut self, x: i32, y: i32, text: &[u8]) {
        for (i, &c) in text.iter().enumerate() {
            let pos = (x + i as i32, y);
            match glyph_index(c) {
                Some(g) => self
                    .bg
                    .set_tile_dynamic16(pos, &self.glyphs[g], ui_effect()),
                None => self
                    .bg
                    .set_tile_dynamic16(pos, &self.blank_tile, ui_effect()),
            };
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
