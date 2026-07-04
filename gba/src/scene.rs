//! アプリレベルの画面遷移 (仕様書 §14): TITLE → PLAYING → GAME OVER → TITLE。
//!
//! core の [`Phase`] はプレイ中の状態 (Playing/Paused/GameOver) のみを持つため、
//! タイトルを含む画面遷移と各画面のリソース (背景・タイル) の生成/破棄は
//! ここで管理する。シーンの値ごと入れ替えることで、前画面の VRAM
//! (DynamicTile16・スクリーンブロック) は Drop で解放される。

use agb::display::GraphicsFrame;
use agb::input::{ButtonController, ButtonState};
use retris_core::{Game, Phase, TSpin};

use crate::audio::Audio;
use crate::buttons;
use crate::hud::Hud;
use crate::render::Renderer;
use crate::title::TitleScreen;

/// 現在の画面。
pub enum Scene {
    /// タイトル (§14.1)。シード収集とレベル選択。
    Title(TitleScreen),
    /// プレイ中 (§14.2, §14.3)。ポーズ表示もこの中で扱う。
    Playing(PlayScreen),
    /// ゲームオーバー (§14.4)。盤面を残したまま任意ボタン待ち。
    GameOver(PlayScreen),
}

impl Scene {
    /// 起動直後のシーン。
    pub fn boot() -> Self {
        Self::Title(TitleScreen::new())
    }

    /// 1 フレーム進め、遷移があれば次のシーンを返す。
    pub fn update(self, input: &ButtonController, audio: &mut Audio) -> Self {
        match self {
            Self::Title(mut title) => match title.update(input) {
                Some((seed, level)) => {
                    agb::println!("retris: start (seed={seed:#x}, level={level})");
                    audio.play_start();
                    audio.start_bgm();
                    Self::Playing(PlayScreen::new(seed, level))
                }
                None => Self::Title(title),
            },
            Self::Playing(mut play) => {
                if play.update(input, audio) {
                    agb::println!("retris: game over (score={})", play.game.score());
                    Self::GameOver(play)
                } else {
                    Self::Playing(play)
                }
            }
            Self::GameOver(play) => {
                // 任意ボタンの新規押下で TITLE へ (押しっぱなしでは遷移しない)。
                if input.is_just_pressed(ButtonState::all()) {
                    Self::Title(TitleScreen::new())
                } else {
                    Self::GameOver(play)
                }
            }
        }
    }

    /// このフレームに現在のシーンを表示する。
    pub fn show(&self, frame: &mut GraphicsFrame<'_>) {
        match self {
            Self::Title(title) => title.show(frame),
            Self::Playing(play) | Self::GameOver(play) => play.show(frame),
        }
    }
}

/// プレイ画面一式 (ゲーム状態 + フィールド描画 + HUD)。
pub struct PlayScreen {
    game: Game,
    renderer: Renderer,
    hud: Hud,
    /// PAUSE オーバーレイを表示中か (フィールドも隠す §14.3)。
    pause_shown: bool,
}

impl PlayScreen {
    fn new(seed: u32, start_level: u32) -> Self {
        Self {
            game: Game::new(seed, start_level),
            renderer: Renderer::new(),
            hud: Hud::new(),
            pause_shown: false,
        }
    }

    /// 1 フレーム進める。ゲームオーバーに遷移したフレームで `true` を返す。
    fn update(&mut self, input: &ButtonController, audio: &mut Audio) -> bool {
        self.game.update(buttons::read(input));
        self.renderer.render(&self.game);
        self.hud.update(&self.game); // events() は同フレーム内に読む必要がある
        self.play_sfx(audio);

        // ポーズ表示 (§14.3): フィールドを隠して「PAUSE」を出す。BGM も止める。
        let paused = matches!(self.game.phase(), Phase::Paused);
        if paused != self.pause_shown {
            self.hud.set_pause_overlay(paused);
            if paused {
                audio.pause_bgm();
            } else {
                audio.resume_bgm();
            }
            self.pause_shown = paused;
        }

        if self.game.events().topped_out {
            // ゲームオーバー演出 (§14.4): 盤面グレーアウト + 表示。
            // スコア等は HUD に出たまま残る。BGM を止めて SE を鳴らす。
            audio.stop_bgm();
            audio.play_game_over();
            self.renderer.greyout(&self.game);
            self.hud.draw_game_over_overlay();
            true
        } else {
            false
        }
    }

    /// このフレームのイベントから SE を選んで鳴らす。
    ///
    /// - ロックしたフレームはロック系 1 音のみ:
    ///   Perfect Clear > テトリス (4 列) > T-Spin > 消去数別 > ハードドロップ >
    ///   通常ロック。レベルアップはそれに重ねて鳴らす。
    /// - 移動・回転・ソフトドロップは連打されるため控えめ音量 (audio 側で調整)。
    /// - ゲームオーバー音は呼び出し元が BGM 停止とセットで鳴らす。
    fn play_sfx(&self, audio: &mut Audio) {
        let events = self.game.events();
        if events.topped_out {
            return;
        }

        if let Some(lock) = events.locked {
            if lock.perfect_clear {
                audio.play_perfect_clear();
            } else if lock.lines_cleared == 4 {
                audio.play_tetris();
            } else if lock.tspin != TSpin::None {
                audio.play_tspin();
            } else if lock.lines_cleared > 0 {
                audio.play_line_clear(lock.lines_cleared);
            } else if events.hard_dropped {
                audio.play_hard_drop();
            } else {
                audio.play_lock();
            }

            if lock.level_up {
                audio.play_level_up();
            }
        } else {
            if events.hold_performed {
                audio.play_hold();
            }
            if events.rotated {
                audio.play_rotate();
            }
            if events.shifted {
                audio.play_move();
            }
            if events.soft_drop_rows > 0 {
                audio.play_soft_drop();
            }
        }
    }

    fn show(&self, frame: &mut GraphicsFrame<'_>) {
        if !self.pause_shown {
            self.renderer.show(frame);
        }
        self.hud.show(frame);
    }
}
