#![no_std]
#![no_main]

mod buttons;
mod hud;
mod render;

use agb::input::ButtonController;
use retris_core::Game;

use crate::hud::Hud;
use crate::render::Renderer;

// TODO(後工程・タイトル画面): シード収集 (ボタン押下までの経過フレーム等) と
// 開始レベル選択はタイトル画面で行う。当面は固定シード・レベル 1 で即プレイ開始。
const FIXED_SEED: u32 = 0xC0_FFEE;
const START_LEVEL: u32 = 1;

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    let mut renderer = Renderer::new(&mut gfx); // パレット登録を含むため Hud より先
    let mut hud = Hud::new();
    let mut input = ButtonController::new();

    let mut game = Game::new(FIXED_SEED, START_LEVEL);
    let mut prev_phase = game.phase();

    agb::println!("retris: start (seed={FIXED_SEED:#x}, level={START_LEVEL})");

    loop {
        input.update();
        game.update(buttons::read(&input));

        let phase = game.phase();
        if phase != prev_phase {
            // TODO(後工程): GameOver / Paused の画面演出。当面はログのみ。
            agb::println!("retris: phase {prev_phase:?} -> {phase:?}");
            prev_phase = phase;
        }

        renderer.render(&game);
        hud.update(&game); // events() は同フレーム内に読む必要がある

        let mut frame = gfx.frame();
        renderer.show(&mut frame);
        hud.show(&mut frame);
        frame.commit(); // VBlank 待ちを含む
    }
}
