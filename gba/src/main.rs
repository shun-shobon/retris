#![no_std]
#![no_main]

use agb::display::Rgb;

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    agb::println!("retris boot ok");

    let mut gfx = gba.graphics.get();

    // パレット0の色0はバックドロップ(何も描画されていない領域の色)。
    // 起動確認のため画面全体を単色で塗る。
    gfx.set_background_palette_colour(0, 0, Rgb::new(0x20, 0x60, 0xa0).to_rgb15());

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
