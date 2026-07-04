#![no_std]
#![no_main]

mod audio;
mod buttons;
mod font;
mod hud;
mod render;
mod scene;
mod title;

use agb::input::ButtonController;

use crate::audio::Audio;
use crate::scene::Scene;

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    render::init_palettes(&mut gfx);

    let mut audio = Audio::new(gba.mixer.mixer(audio::FREQUENCY));
    let mut input = ButtonController::new();
    let mut scene = Scene::boot();

    loop {
        input.update();
        scene = scene.update(&input, &mut audio);

        let mut frame = gfx.frame();
        scene.show(&mut frame);
        audio.frame(); // VBlank 待ち直前にミキシング (毎フレーム必須)
        frame.commit(); // VBlank 待ちを含む
    }
}
