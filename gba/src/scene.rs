//! アプリレベルの画面遷移 (仕様書 §14): TITLE → PLAYING → GAME OVER → TITLE。
//!
//! core の [`Phase`] はプレイ中の状態 (Playing/Paused/GameOver) のみを持つため、
//! タイトルを含む画面遷移と各画面のリソース (背景・タイル) の生成/破棄は
//! ここで管理する。シーンの値ごと入れ替えることで、前画面の VRAM
//! (DynamicTile16・スクリーンブロック) は Drop で解放される。

use agb::display::GraphicsFrame;
use agb::input::{ButtonController, ButtonState};
use retris_core::{Game, Phase};

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
    pub fn update(self, input: &ButtonController) -> Self {
        match self {
            Self::Title(mut title) => match title.update(input) {
                Some((seed, level)) => {
                    agb::println!("retris: start (seed={seed:#x}, level={level})");
                    Self::Playing(PlayScreen::new(seed, level))
                }
                None => Self::Title(title),
            },
            Self::Playing(mut play) => {
                if play.update(input) {
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
    fn update(&mut self, input: &ButtonController) -> bool {
        self.game.update(buttons::read(input));
        self.renderer.render(&self.game);
        self.hud.update(&self.game); // events() は同フレーム内に読む必要がある

        // ポーズ表示 (§14.3): フィールドを隠して「PAUSE」を出す。
        let paused = matches!(self.game.phase(), Phase::Paused);
        if paused != self.pause_shown {
            self.hud.set_pause_overlay(paused);
            self.pause_shown = paused;
        }

        if self.game.events().topped_out {
            // ゲームオーバー演出 (§14.4): 盤面グレーアウト + 表示。
            // スコア等は HUD に出たまま残る。
            self.renderer.greyout(&self.game);
            self.hud.draw_game_over_overlay();
            true
        } else {
            false
        }
    }

    fn show(&self, frame: &mut GraphicsFrame<'_>) {
        if !self.pause_shown {
            self.renderer.show(frame);
        }
        self.hud.show(frame);
    }
}
