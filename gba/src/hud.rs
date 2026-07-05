//! HUD: スコア・レベル・消去ライン・ネクスト 5・ホールド (+B2B/コンボ表示)。
//!
//! ラベルと数字はプレイフィールドとは別の背景レイヤ 1 枚に自作 5×7 ピクセル
//! フォント ([`crate::font::Font`]) で描き、値が変化したフレームだけ更新する。
//! ミノプレビュー (ネクスト 5+ホールド) は 32×16 の動的スプライト (OAM) で、
//! ミノ形状をスプライト内にピクセル単位でセンタリングして置く (タイルマップ
//! では 3 セル幅ミノの半タイル中央合わせができないため)。
//!
//! フィールド上のオーバーレイ (PAUSE / GAME OVER) は HUD レイヤに描く。
//! HUD は [`Priority::P0`]、フィールドは `Priority::P1` のため HUD が常に手前に
//! 出る。フィールド領域 x10..=19 は HUD レイヤでは通常透過で、オーバーレイの
//! 文字タイルだけがフィールドに重なって見える。

use agb::display::{
    GraphicsFrame, Priority,
    object::{DynamicSprite16, Object, Size, SpriteVram},
    tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileFormat},
};
use retris_core::{Game, LockEvent, NEXT_COUNT, Phase, Rotation, Tetromino};

use crate::font::Font;
use crate::render::{
    FIELD_ORIGIN_TX, draw_block_bevel, make_block_tile, obj_piece_palette, ui_effect,
};

// ---- レイアウト (タイル座標)。フィールドは中央 x9..=20、HUD はその左右 ----

/// 右カラム (ホールド・スコア・レベル・ライン・演出) の左端。
/// 右壁 (x=20) から 1 タイル空け、SCORE 8 桁 (x22..=29) が画面右端で収まる位置。
const HUD_X: i32 = 22;
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

/// 左カラム (ネクスト) のラベル左端。
/// 左サイド x0..=8 のうち、左壁 (x=9) との間に余白が残るよう左寄り。
const NEXT_X: i32 = 2;
const NEXT_LABEL_Y: i32 = 0;
/// ネクスト i 番目のスロット上端 (タイル) = `NEXT_SLOT_Y0 + i * NEXT_SLOT_PITCH`。
const NEXT_SLOT_Y0: i32 = 2;
const NEXT_SLOT_PITCH: i32 = 3;

// ---- ミノプレビューのスプライト配置 (ピクセル座標) ----

/// プレビュースプライトの幅 (I ミノ 4 セルがちょうど収まる)。
const PREVIEW_W: i32 = 32;
/// ネクストスロットのスプライト左上 x。左サイド x0..=8 (72px) の水平中央。
const NEXT_SPRITE_X: i32 = (9 * 8 - PREVIEW_W) / 2;
/// ホールドプレビューのスプライト左上。枠 (6×4 タイル) の内側 4×2 セルと一致。
const HOLD_SPRITE_X: i32 = (HUD_X + 1) * 8;
const HOLD_SPRITE_Y: i32 = (HOLD_FRAME_Y + 1) * 8;

/// フィールド上オーバーレイの行 (フィールド中央付近)。
const OVERLAY_Y: i32 = 9;
/// 「PAUSE」(5 文字) をフィールド 10 列の中央に置く左端。
const PAUSE_X: i32 = FIELD_ORIGIN_TX + 2;
/// 「GAME OVER」(9 文字) をフィールド 10 列の中央に置く左端。
const GAME_OVER_X: i32 = FIELD_ORIGIN_TX;

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
    /// ホールド枠用のブロック。
    block_tile: DynamicTile16,
    /// 種別ごとのプレビュースプライト (遅延生成して全スロットで使い回す)。
    previews: PreviewSprites,
    /// ホールドのプレビュー (未ホールドなら `None`)。
    hold_obj: Option<Object>,
    /// ネクスト 5 スロットのプレビュー (`None` は初回描画前のみ)。
    next_objs: [Option<Object>; NEXT_COUNT],
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
            previews: PreviewSprites::default(),
            hold_obj: None,
            next_objs: Default::default(),
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
            self.hold_obj = snapshot.hold.map(|kind| {
                let mut obj = Object::new(self.previews.get(kind));
                obj.set_pos((HOLD_SPRITE_X, HOLD_SPRITE_Y));
                obj
            });
        }
        for (i, &kind) in snapshot.next.iter().enumerate() {
            // ミノ種別が変わったスロットだけスプライトを差し替える。
            if force || prev.is_some_and(|p| p.next[i] != kind) {
                let mut obj = Object::new(self.previews.get(kind));
                obj.set_pos((
                    NEXT_SPRITE_X,
                    (NEXT_SLOT_Y0 + i as i32 * NEXT_SLOT_PITCH) * 8,
                ));
                self.next_objs[i] = Some(obj);
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

    /// このフレームに HUD レイヤとプレビュースプライトを表示する。
    ///
    /// OAM は毎フレーム `show` したものだけが表示される (agb の仕様) ため、
    /// シーン遷移でこの呼び出しが無くなればスプライトも自動的に消える。
    pub fn show(&self, frame: &mut GraphicsFrame<'_>) {
        self.bg.show(frame);
        if let Some(obj) = &self.hold_obj {
            obj.show(frame);
        }
        for obj in self.next_objs.iter().flatten() {
            obj.show(frame);
        }
    }

    /// フィールド中央の「PAUSE」表示を出す/消す (§14.3)。
    pub fn set_pause_overlay(&mut self, shown: bool) {
        let text: &[u8] = if shown { b"PAUSE" } else { b"     " };
        self.write(PAUSE_X, OVERLAY_Y, text);
    }

    /// フィールド中央に「GAME OVER」を表示する (§14.4)。
    pub fn draw_game_over_overlay(&mut self) {
        self.write(GAME_OVER_X, OVERLAY_Y, b"GAME OVER");
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

}

/// 種別ごとのプレビュースプライトのキャッシュ (添字は [`Tetromino`] の宣言順)。
///
/// スプライト VRAM は参照カウント管理 ([`SpriteVram`]) のため、ここで保持して
/// いる限り再生成されず、`Hud` ごと Drop されれば解放される。
#[derive(Default)]
struct PreviewSprites([Option<SpriteVram>; 7]);

impl PreviewSprites {
    /// `kind` のプレビュースプライトを返す (初回のみ生成)。
    fn get(&mut self, kind: Tetromino) -> SpriteVram {
        self.0[kind as usize]
            .get_or_insert_with(|| make_preview_sprite(kind))
            .clone()
    }
}

/// ミノ 1 種のプレビュースプライト (32×16, 4bpp) を生成して VRAM へ置く。
///
/// スポーン向き ([`Rotation::Spawn`]) の形をフィールドと同じベベル付きブロック
/// ([`draw_block_bevel`]) で描き、スプライト内でピクセル単位にセンタリングする
/// (I: x0/y4、O: x8/y0、その他 3 セル幅: x4/y0)。色は背景と同内容の OBJ
/// パレット ([`obj_piece_palette`])。
fn make_preview_sprite(kind: Tetromino) -> SpriteVram {
    let mut sprite = DynamicSprite16::new(Size::S32x16);

    let cells = kind.cells(Rotation::Spawn);
    let min_cx = cells.iter().map(|&(cx, _)| cx).min().unwrap_or(0);
    let max_cx = cells.iter().map(|&(cx, _)| cx).max().unwrap_or(0);
    let min_cy = cells.iter().map(|&(_, cy)| cy).min().unwrap_or(0);
    let max_cy = cells.iter().map(|&(_, cy)| cy).max().unwrap_or(0);
    let x0 = (32 - i32::from(max_cx - min_cx + 1) * 8) / 2;
    let y0 = (16 - i32::from(max_cy - min_cy + 1) * 8) / 2;

    for (cx, cy) in cells {
        // セル座標は y 上向き (§1.1) なので、描画時は上下を反転する。
        let bx = (x0 + i32::from(cx - min_cx) * 8) as usize;
        let by = (y0 + i32::from(max_cy - cy) * 8) as usize;
        draw_block_bevel(|dx, dy, colour| sprite.set_pixel(bx + dx, by + dy, colour));
    }

    sprite.to_vram(obj_piece_palette(kind))
}
