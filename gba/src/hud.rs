//! 右側 HUD: スコア・レベル・消去ライン・ネクスト 5・ホールド (+B2B/コンボ表示)。
//!
//! プレイフィールドとは別の背景レイヤ 1 枚に描く。ラベルと数字は自作 5×7 ピクセル
//! フォント ([`crate::font::Font`])、ミノプレビューはフィールドと同じブロック
//! タイル+パレット切替。値が変化したフレームだけタイルを更新する。
//!
//! フィールド上のオーバーレイ (PAUSE / GAME OVER) もこのレイヤに描く。
//! HUD は [`Priority::P0`]、フィールドは `Priority::P1` のため HUD が常に手前に
//! 出る。フィールド領域 x2..=11 は HUD レイヤでは通常透過で、オーバーレイの
//! 文字タイルだけがフィールドに重なって見える。

use agb::display::{
    GraphicsFrame, Priority,
    tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileFormat},
};
use retris_core::{Game, LockEvent, NEXT_COUNT, Phase, Rotation, Tetromino};

use crate::font::Font;
use crate::render::{make_block_tile, piece_effect, ui_effect};

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

/// フィールド上オーバーレイの行 (フィールド中央付近)。
const OVERLAY_Y: i32 = 9;

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
    font: Font,
    /// ミノプレビュー・ホールド枠用のブロック。
    block_tile: DynamicTile16,
    /// 前フレームに描画した値。`None` なら未描画 (初回に全描画)。
    drawn: Option<Snapshot>,
    /// B2B/コンボ演出行の残り表示フレーム数。
    fx_timer: u8,
}

impl Hud {
    /// タイル生成とラベル・ホールド枠の初期描画を行う。
    ///
    /// パレットは [`crate::render::init_palettes`] で登録済みであること。
    pub fn new() -> Self {
        let bg = RegularBackground::new(
            Priority::P0,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );

        let mut hud = Self {
            bg,
            font: Font::new(),
            block_tile: make_block_tile(),
            drawn: None,
            fx_timer: 0,
        };

        hud.write(HUD_X, HOLD_LABEL_Y, b"HOLD");
        hud.write(HUD_X, SCORE_LABEL_Y, b"SCORE");
        hud.write(HUD_X, LEVEL_LABEL_Y, b"LEVEL");
        hud.write(HUD_X, LINES_LABEL_Y, b"LINES");
        hud.write(NEXT_X, NEXT_LABEL_Y, b"NEXT");
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
            self.font
                .write_number(&mut self.bg, HUD_X, SCORE_Y, SCORE_DIGITS, snapshot.score);
        }
        if changed(|s| s.level) {
            self.font
                .write_number(&mut self.bg, HUD_X, LEVEL_Y, LEVEL_DIGITS, snapshot.level);
        }
        if changed(|s| s.lines) {
            self.font
                .write_number(&mut self.bg, HUD_X, LINES_Y, LINES_DIGITS, snapshot.lines);
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
        // ポーズ中はタイマーも停止する (§14.3)。
        if let Some(lock) = game.events().locked {
            self.draw_lock_fx(&lock);
        } else if self.fx_timer > 0 && !matches!(game.phase(), Phase::Paused) {
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

    /// フィールド中央の「PAUSE」表示を出す/消す (§14.3)。
    pub fn set_pause_overlay(&mut self, shown: bool) {
        let text: &[u8] = if shown { b"PAUSE" } else { b"     " };
        self.write(4, OVERLAY_Y, text);
    }

    /// フィールド中央に「GAME OVER」を表示する (§14.4)。
    pub fn draw_game_over_overlay(&mut self) {
        self.write(2, OVERLAY_Y, b"GAME OVER");
    }

    /// ロック演出行を更新する。B2B・コンボ (1 以上) のどちらも無ければ消す。
    fn draw_lock_fx(&mut self, lock: &LockEvent) {
        if lock.b2b_applied {
            self.write(HUD_X, FX_B2B_Y, b"B2B");
        } else {
            self.write(HUD_X, FX_B2B_Y, b"   ");
        }
        if lock.combo >= 1 {
            self.write(HUD_X, FX_COMBO_Y, b"C");
            self.font
                .write_number(&mut self.bg, HUD_X + 1, FX_COMBO_Y, 2, lock.combo as u32);
        } else {
            self.write(HUD_X, FX_COMBO_Y, b"   ");
        }
        self.fx_timer = if lock.b2b_applied || lock.combo >= 1 {
            FX_FRAMES
        } else {
            0
        };
    }

    /// ロック演出行を消す。
    fn clear_lock_fx(&mut self) {
        self.write(HUD_X, FX_B2B_Y, b"   ");
        self.write(HUD_X, FX_COMBO_Y, b"   ");
    }

    /// [`Font::write`] の self.bg 束縛ショートカット。
    fn write(&mut self, x: i32, y: i32, text: &[u8]) {
        self.font.write(&mut self.bg, x, y, text);
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
                    .set_tile_dynamic16((x0 + dx, y0 + dy), self.font.blank(), ui_effect());
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
}
