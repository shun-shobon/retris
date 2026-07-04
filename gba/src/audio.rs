//! 効果音と BGM。
//!
//! wav はすべて `assets-gen/gen_sfx.py` が決定的に合成したもの
//! (モノラル 16bit 10512Hz)。agb の mixer はリサンプリングしないため、
//! サンプルレートは [`Frequency::Hz10512`] と一致していなければならない。
//!
//! - SE: 低優先度チャンネル。8ch が埋まっていたら鳴らさない (`play_sound` が
//!   `None` を返すだけで安全)。
//! - BGM: 高優先度チャンネル + ループ。SE より控えめな音量にする。
//! - [`Audio::frame`] を毎フレーム呼ばないと音が途切れる (mixer の仕様)。

use agb::fixnum::num;
use agb::include_wav;
use agb::sound::mixer::{ChannelId, Frequency, Mixer, SoundChannel, SoundData};

/// mixer の動作周波数。wav のサンプルレートと一致必須。
pub const FREQUENCY: Frequency = Frequency::Hz10512;

static SFX_MOVE: SoundData = include_wav!("sfx/move.wav");
static SFX_ROTATE: SoundData = include_wav!("sfx/rotate.wav");
static SFX_HOLD: SoundData = include_wav!("sfx/hold.wav");
static SFX_HARD_DROP: SoundData = include_wav!("sfx/hard_drop.wav");
static SFX_LOCK: SoundData = include_wav!("sfx/lock.wav");
static SFX_LINE_CLEAR_1: SoundData = include_wav!("sfx/line_clear_1.wav");
static SFX_LINE_CLEAR_2: SoundData = include_wav!("sfx/line_clear_2.wav");
static SFX_LINE_CLEAR_3: SoundData = include_wav!("sfx/line_clear_3.wav");
static SFX_TETRIS: SoundData = include_wav!("sfx/tetris.wav");
static SFX_TSPIN: SoundData = include_wav!("sfx/tspin.wav");
static SFX_PERFECT_CLEAR: SoundData = include_wav!("sfx/perfect_clear.wav");
static SFX_LEVEL_UP: SoundData = include_wav!("sfx/level_up.wav");
static SFX_GAME_OVER: SoundData = include_wav!("sfx/game_over.wav");
static SFX_START: SoundData = include_wav!("sfx/start.wav");
static BGM_KOROBEINIKI: SoundData = include_wav!("sfx/bgm_korobeiniki.wav");

/// mixer を包んだサウンド出力。`main` で 1 つだけ作り、シーンをまたいで
/// `&mut` で引き回す。
pub struct Audio<'g> {
    mixer: Mixer<'g>,
    /// 再生中の BGM チャンネル (停止済みなら `None`)。
    bgm: Option<ChannelId>,
}

impl<'g> Audio<'g> {
    pub fn new(mixer: Mixer<'g>) -> Self {
        Self { mixer, bgm: None }
    }

    /// 毎フレーム 1 回、`frame.commit()` の直前に呼ぶこと。
    pub fn frame(&mut self) {
        self.mixer.frame();
    }

    /// 通常音量で SE を鳴らす。空きチャンネルがなければ諦める。
    fn play(&mut self, data: SoundData) {
        let _ = self.mixer.play_sound(SoundChannel::new(data));
    }

    /// 控えめな音量で SE を鳴らす (連打される操作音用)。
    fn play_quiet(&mut self, data: SoundData) {
        let mut channel = SoundChannel::new(data);
        channel.volume(num!(0.35));
        let _ = self.mixer.play_sound(channel);
    }

    // --- 操作音 (連打されるので控えめ) ---

    pub fn play_move(&mut self) {
        self.play_quiet(SFX_MOVE);
    }

    pub fn play_rotate(&mut self) {
        self.play_quiet(SFX_ROTATE);
    }

    /// ソフトドロップの落下音。毎フレーム鳴り得るのでさらに控えめ。
    pub fn play_soft_drop(&mut self) {
        let mut channel = SoundChannel::new(SFX_MOVE);
        channel.volume(num!(0.15));
        let _ = self.mixer.play_sound(channel);
    }

    // --- 単発イベント音 ---

    pub fn play_hold(&mut self) {
        self.play(SFX_HOLD);
    }

    pub fn play_hard_drop(&mut self) {
        self.play(SFX_HARD_DROP);
    }

    pub fn play_lock(&mut self) {
        self.play(SFX_LOCK);
    }

    /// 消去ライン数 (1〜3) に応じた消去音。4 列は [`Self::play_tetris`]。
    pub fn play_line_clear(&mut self, lines: u8) {
        self.play(match lines {
            1 => SFX_LINE_CLEAR_1,
            2 => SFX_LINE_CLEAR_2,
            _ => SFX_LINE_CLEAR_3,
        });
    }

    pub fn play_tetris(&mut self) {
        self.play(SFX_TETRIS);
    }

    pub fn play_tspin(&mut self) {
        self.play(SFX_TSPIN);
    }

    pub fn play_perfect_clear(&mut self) {
        self.play(SFX_PERFECT_CLEAR);
    }

    pub fn play_level_up(&mut self) {
        self.play(SFX_LEVEL_UP);
    }

    pub fn play_game_over(&mut self) {
        self.play(SFX_GAME_OVER);
    }

    /// タイトルでのゲーム開始音。
    pub fn play_start(&mut self) {
        self.play(SFX_START);
    }

    // --- BGM ---

    /// BGM (コロブチカ) をループ再生する。再生中なら停止してから鳴らし直す。
    pub fn start_bgm(&mut self) {
        self.stop_bgm();
        let mut channel = SoundChannel::new_high_priority(BGM_KOROBEINIKI);
        channel.should_loop().volume(num!(0.55));
        self.bgm = self.mixer.play_sound(channel);
    }

    pub fn stop_bgm(&mut self) {
        if let Some(id) = self.bgm.take()
            && let Some(channel) = self.mixer.channel(&id)
        {
            channel.stop();
        }
    }

    pub fn pause_bgm(&mut self) {
        if let Some(id) = &self.bgm
            && let Some(channel) = self.mixer.channel(id)
        {
            channel.pause();
        }
    }

    pub fn resume_bgm(&mut self) {
        if let Some(id) = &self.bgm
            && let Some(channel) = self.mixer.channel(id)
        {
            channel.resume();
        }
    }
}
