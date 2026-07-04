#![no_std]
#![no_main]

mod buttons;
mod font;
mod hud;
mod render;
mod scene;
mod title;

use agb::input::ButtonController;

use crate::scene::Scene;

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    render::init_palettes(&mut gfx);

    let mut input = ButtonController::new();
    let mut scene = Scene::boot();

    loop {
        input.update();
        scene = scene.update(&input);

        let mut frame = gfx.frame();
        scene.show(&mut frame);
        frame.commit(); // VBlank 待ちを含む
    }
}
