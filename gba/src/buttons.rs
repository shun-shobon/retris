//! agb の物理ボタン状態を retris-core の [`Buttons`] へ写像する (仕様書 §12.1)。
//!
//! DAS/ARR・押下エッジ検出はすべて core 側 (`Game::update`) の責務のため、
//! ここでは「いま押されているか」をそのまま詰め替えるだけにする。

use agb::input::{Button, ButtonController};
use retris_core::Buttons;

/// 現在のボタン押下状態を [`Buttons`] に変換する (§12.1 の割当)。
pub fn read(input: &ButtonController) -> Buttons {
    Buttons {
        left: input.is_pressed(Button::Left),
        right: input.is_pressed(Button::Right),
        down: input.is_pressed(Button::Down),
        up: input.is_pressed(Button::Up),
        rotate_cw: input.is_pressed(Button::A),
        rotate_ccw: input.is_pressed(Button::B),
        hold: input.is_pressed(Button::L) || input.is_pressed(Button::R),
        start: input.is_pressed(Button::Start),
    }
}
