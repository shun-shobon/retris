//! ランダム入力による長時間プレイスルーのプロパティテスト。
//!
//! 決定的な擬似ランダム入力 ([`Rng`] から導出) で [`Game`] を大量フレーム回し、
//! パニックと不変条件違反 (ミノの重なり・場外、スコアの単調性、レベル整合、
//! ポーズ中の凍結) を検出する。「落ちないこと」の検証が主目的。

use retris_core::{Board, Buttons, FrameEvents, Game, NEXT_COUNT, Phase, Rng, Rotation, Tetromino};

// ---- ランダム入力生成 ----

/// `InputGen::held` の添字。
const LEFT: usize = 0;
const RIGHT: usize = 1;
const DOWN: usize = 2;
const UP: usize = 3;
const CW: usize = 4;
const CCW: usize = 5;
const HOLD: usize = 6;
const START: usize = 7;

/// 現実的な操作に寄せたランダム入力生成器。
///
/// 全ビット独立ランダムだと全ボタン押しっぱなしに近くなりエッジ入力が育たないため、
/// ボタンごとに「残り保持フレーム数」を持ち、種別ごとのバイアスで押下を開始する:
///
/// - 左右・下: 数十フレームの保持を継続しやすい (DAS/ARR・ソフトドロップを踏む)
/// - 回転・ホールド・ハードドロップ: 短いタップをたまに
/// - START: 稀にタップ (ポーズ中は高確率で押して早期解除に寄せる)
struct InputGen {
    rng: Rng,
    held: [u32; 8],
}

impl InputGen {
    fn new(seed: u32) -> Self {
        Self {
            rng: Rng::new(seed),
            held: [0; 8],
        }
    }

    /// 確率 1/n で true。
    fn chance(&mut self, n: u32) -> bool {
        self.rng.bounded(n) == 0
    }

    /// ボタン `idx` の保持を `min..=min+spread` フレームで開始する。
    fn press(&mut self, idx: usize, min: u32, spread: u32) {
        self.held[idx] = min + self.rng.bounded(spread + 1);
    }

    /// 1 フレーム分のボタン状態を生成する。
    fn next_frame(&mut self, paused: bool) -> Buttons {
        for frames in &mut self.held {
            *frames = frames.saturating_sub(1);
        }
        if self.held[LEFT] == 0 && self.held[RIGHT] == 0 {
            if self.chance(10) {
                let idx = if self.chance(2) { LEFT } else { RIGHT };
                self.press(idx, 8, 50);
            }
        } else if self.chance(80) {
            // 稀に反対方向を重ね、左右同時押し・方向切替の経路も踏む。
            let idx = if self.held[LEFT] == 0 { LEFT } else { RIGHT };
            self.press(idx, 4, 20);
        }
        if self.held[DOWN] == 0 && self.chance(14) {
            self.press(DOWN, 5, 40);
        }
        if self.held[UP] == 0 && self.chance(20) {
            self.press(UP, 1, 2);
        }
        if self.held[CW] == 0 && self.chance(9) {
            self.press(CW, 1, 3);
        }
        if self.held[CCW] == 0 && self.chance(13) {
            self.press(CCW, 1, 3);
        }
        if self.held[HOLD] == 0 && self.chance(40) {
            self.press(HOLD, 1, 2);
        }
        let start_chance = if paused { 8 } else { 700 };
        if self.held[START] == 0 && self.chance(start_chance) {
            self.press(START, 1, 2);
        }
        Buttons {
            left: self.held[LEFT] > 0,
            right: self.held[RIGHT] > 0,
            down: self.held[DOWN] > 0,
            up: self.held[UP] > 0,
            rotate_cw: self.held[CW] > 0,
            rotate_ccw: self.held[CCW] > 0,
            hold: self.held[HOLD] > 0,
            start: self.held[START] > 0,
        }
    }
}

// ---- 不変条件 ----

/// 数百フレームごと・ゲームオーバー時に検査する不変条件。
///
/// `seed` / `frame` は失敗時の再現用コンテキスト。
#[track_caller]
fn check_invariants(game: &Game, start_level: u32, seed: u32, frame: u32) {
    // レベルは Fixed Goal (§11): start_level + total_lines / 10。
    assert_eq!(
        game.level(),
        start_level + game.total_lines() / 10,
        "レベルが Fixed Goal と不一致 (seed={seed} frame={frame})"
    );
    // ネクスト 0..5 が全てパニックせず取得できること。
    for i in 0..NEXT_COUNT {
        let _ = game.next(i);
    }
    let _ = game.combo();
    let _ = game.b2b();
    match game.phase() {
        Phase::Playing | Phase::Paused => {
            let active = *game.active_piece().unwrap_or_else(|| {
                panic!("プレイ中は操作ミノがあるはず (seed={seed} frame={frame})")
            });
            assert!(
                game.board().fits(&active),
                "操作ミノが盤面と重なる/場外 (seed={seed} frame={frame}): {active:?}"
            );
            let ghost_piece = game.ghost_piece().unwrap_or_else(|| {
                panic!("プレイ中はゴーストがあるはず (seed={seed} frame={frame})")
            });
            assert!(
                game.board().fits(&ghost_piece),
                "ゴーストが盤面と重なる/場外 (seed={seed} frame={frame}): {ghost_piece:?}"
            );
            assert!(
                ghost_piece.y <= active.y,
                "ゴーストが操作ミノより上 (seed={seed} frame={frame}): active={active:?} ghost={ghost_piece:?}"
            );
            assert_eq!(
                (ghost_piece.kind, ghost_piece.rot, ghost_piece.x),
                (active.kind, active.rot, active.x),
                "ゴーストは kind/rot/x を保つはず (seed={seed} frame={frame})"
            );
        }
        Phase::GameOver => {
            assert_eq!(
                game.active_piece(),
                None,
                "ゲームオーバー後に操作ミノが残っている (seed={seed} frame={frame})"
            );
            assert_eq!(
                game.ghost_piece(),
                None,
                "ゲームオーバー後にゴーストが残っている (seed={seed} frame={frame})"
            );
        }
    }
}

// ---- 1. ランダムプレイスルー (複数シード × 大量フレーム) ----

/// プレイスルーを検証するシード (多様なビットパターンの 12 種)。
const SEEDS: [u32; 12] = [
    1,
    2,
    42,
    0xDEAD_BEEF,
    0x1234_5678,
    0xFFFF_FFFF,
    7_777_777,
    0xCAFE_BABE,
    0x0BAD_F00D,
    424_242,
    0x8000_0000,
    0xA5A5_A5A5,
];

/// シードごとに回すフレーム数。
const FRAMES_PER_SEED: u32 = 30_000;

/// シード 1 個分のランダムプレイスルー。GameOver になったら派生シードで
/// 新しい [`Game`] に置き換えて続行し、フレーム予算を使い切る。
fn run_random_playthrough(seed: u32, start_level: u32, frames: u32) {
    let mut input = InputGen::new(seed ^ 0xA5A5_5A5A);
    let mut game_seed = seed;
    let mut game = Game::new(game_seed, start_level);
    let mut prev_score = 0u32;
    let mut prev_lines = 0u32;
    for frame in 0..frames {
        let paused = matches!(game.phase(), Phase::Paused);
        game.update(input.next_frame(paused));
        // スコア・累計ラインの単調非減少 (毎フレーム検査)。
        assert!(
            game.score() >= prev_score,
            "スコアが減少 (seed={game_seed} frame={frame}): {prev_score} -> {}",
            game.score()
        );
        assert!(
            game.total_lines() >= prev_lines,
            "累計ラインが減少 (seed={game_seed} frame={frame}): {prev_lines} -> {}",
            game.total_lines()
        );
        prev_score = game.score();
        prev_lines = game.total_lines();
        if frame % 240 == 0 {
            check_invariants(&game, start_level, game_seed, frame);
        }
        if matches!(game.phase(), Phase::GameOver) {
            check_invariants(&game, start_level, game_seed, frame);
            // GameOver 後の update が不活性であること (§14.4) も軽く確認。
            game.update(input.next_frame(false));
            assert_eq!(
                *game.events(),
                FrameEvents::default(),
                "GameOver 後にイベントが発生 (seed={game_seed} frame={frame})"
            );
            // 次の (派生) シードで新規ゲームに置き換えて続行する。
            game_seed = game_seed.wrapping_mul(0x9E37_79B9).wrapping_add(1);
            game = Game::new(game_seed, start_level);
            prev_score = 0;
            prev_lines = 0;
        }
    }
    check_invariants(&game, start_level, game_seed, frames);
}

#[test]
fn random_playthrough_survives_across_seeds() {
    // 開始レベルも巡回させ、低速〜高速重力の帯を広く踏む。
    const START_LEVELS: [u32; 5] = [1, 4, 7, 11, 15];
    for (i, &seed) in SEEDS.iter().enumerate() {
        run_random_playthrough(seed, START_LEVELS[i % START_LEVELS.len()], FRAMES_PER_SEED);
    }
}

// ---- 2. 高レベル帯 (start_level=15 から 20G 領域へ) ----

/// 列 `x` の高さ (最上段の占有セル + 1)。空列は 0。
fn column_height(board: &Board, x: i8) -> i8 {
    (0i8..40)
        .rev()
        .find(|&y| board.get(x, y).is_some())
        .map_or(0, |y| y + 1)
}

/// 盤面の貪欲評価 (Dellacherie 系の整数重み)。大きいほど良い。
///
/// 消去ラインを優遇し、総高さ・穴・凹凸にペナルティを与える。
fn evaluate(board: &Board, lines_cleared: i32) -> i32 {
    let mut aggregate = 0i32;
    let mut holes = 0i32;
    let mut bumpiness = 0i32;
    let mut prev_height: Option<i8> = None;
    for x in 0i8..10 {
        let height = column_height(board, x);
        aggregate += i32::from(height);
        for y in 0..height {
            if board.get(x, y).is_none() {
                holes += 1;
            }
        }
        if let Some(prev) = prev_height {
            bumpiness += i32::from((height - prev).abs());
        }
        prev_height = Some(height);
    }
    760 * lines_cleared - 510 * aggregate - 2000 * holes - 180 * bumpiness
}

/// `from` から `to` への CW 回転回数 (0..=3)。
fn cw_steps(from: Rotation, to: Rotation) -> u8 {
    let mut rot = from;
    for steps in 0..4 {
        if rot == to {
            return steps;
        }
        rot = rot.cw();
    }
    unreachable!("Rotation は 4 状態の巡回");
}

/// 操作中のミノを目標 (回転状態, bx) へタップ操作 (押す 1 フレーム + 離す 1 フレーム)
/// で操作し、最後にハードドロップして、ロックまたはゲームオーバーまで進める。
///
/// `move_first` が false なら回転 → 移動、true なら移動 → 回転の順で操作する
/// (高重力の接地スライドでは順序によって到達できる置き場所が変わる)。
/// 目標に到達できない場合 (地形・壁による移動失敗や回転不発が続く場合) も、
/// ロックディレイ満了かフォールバックのハードドロップで必ずロックまで進む。
fn steer_piece(game: &mut Game, target_rot: Rotation, target_x: i8, move_first: bool) {
    for _ in 0..120 {
        let Some(piece) = game.active_piece().copied() else {
            return; // ゲームオーバー
        };
        let need_rotate = piece.rot != target_rot;
        let rotate_now = need_rotate && !(move_first && piece.x != target_x);
        let mut press = Buttons::default();
        if rotate_now {
            if cw_steps(piece.rot, target_rot) == 3 {
                press.rotate_ccw = true;
            } else {
                press.rotate_cw = true;
            }
        } else if piece.x < target_x {
            press.right = true;
        } else if piece.x > target_x {
            press.left = true;
        } else {
            press.up = true;
        }
        game.update(press);
        if game.events().locked.is_some() || game.events().topped_out {
            return;
        }
        game.update(Buttons::default()); // 離す (次のタップをエッジにする)
        if game.events().locked.is_some() || game.events().topped_out {
            return;
        }
    }
    // ここまでロックしないのは回転不発が続く場合など。据え置きでハードドロップする。
    game.update(Buttons::default());
    game.update(Buttons {
        up: true,
        ..Buttons::default()
    });
}

/// 操作中のミノを最良の置き場所へ操作してロックまで進める。
///
/// 全候補 (回転 × 目標列 × 操作順) をクローンした [`Game`] 上で [`steer_piece`]
/// により実際に操作してみて、ロック後の盤面の貪欲評価が最良の候補を本体で再実行する。
/// クローン上と本体は同一状態から同一の決定的な操作列を辿るため、計画と実行が
/// 完全に一致する (重力・キック・ロックディレイの近似モデルを使わない)。
fn play_one_piece_greedily(game: &mut Game) {
    let Some(active) = game.active_piece() else {
        return;
    };
    // O は回転入力が無視される (§4.4) ため Spawn のみを候補にする。
    let rotations: &[Rotation] = if active.kind == Tetromino::O {
        &[Rotation::Spawn]
    } else {
        &[Rotation::Spawn, Rotation::Cw, Rotation::Flip, Rotation::Ccw]
    };
    let mut best_score = i32::MIN;
    let mut best = (active.rot, active.x, false);
    for &rot in rotations {
        for x in -2i8..=9 {
            for move_first in [false, true] {
                let mut sim = game.clone();
                let lines_before = sim.total_lines();
                steer_piece(&mut sim, rot, x, move_first);
                if !matches!(sim.phase(), Phase::Playing) {
                    continue; // トップアウトする手は選ばない (全滅時は現在位置に落とす)
                }
                let cleared = (sim.total_lines() - lines_before) as i32;
                let score = evaluate(sim.board(), cleared);
                if score > best_score {
                    best_score = score;
                    best = (rot, x, move_first);
                }
            }
        }
    }
    steer_piece(game, best.0, best.1, best.2);
}

#[test]
fn high_level_playthrough_reaches_beyond_level_20() {
    const SEED: u32 = 0x00C0_FFEE;
    const START_LEVEL: u32 = 15;
    let mut game = Game::new(SEED, START_LEVEL);
    game.update(Buttons::default()); // 初回のエッジ飲み込みを済ませる
    // レベル 21 (= 60 ライン消去) まで貪欲ボットで大量消去する。
    let mut pieces = 0u32;
    while game.level() < 21 {
        assert!(
            matches!(game.phase(), Phase::Playing),
            "ボットがレベル 21 到達前にトップアウト: pieces={pieces} lines={}",
            game.total_lines()
        );
        assert!(
            pieces < 1_000,
            "レベル 21 に到達できない: pieces={pieces} lines={}",
            game.total_lines()
        );
        play_one_piece_greedily(&mut game);
        pieces += 1;
        check_invariants(&game, START_LEVEL, SEED, pieces);
    }
    // レベル 20 超 = 実効レベル 20 (20G 超・スポーン直後に即接地) の領域でさらに回し、
    // 重力ループと即接地の境界を叩く。
    for _ in 0..80 {
        if !matches!(game.phase(), Phase::Playing) {
            break;
        }
        play_one_piece_greedily(&mut game);
        pieces += 1;
        check_invariants(&game, START_LEVEL, SEED, pieces);
    }
    // 最後はランダム入力に切り替えても壊れないこと (トップアウトしても不変条件を保つ)。
    let mut input = InputGen::new(SEED ^ 0x5EED_5EED);
    for frame in 0..3_000 {
        let paused = matches!(game.phase(), Phase::Paused);
        game.update(input.next_frame(paused));
        if frame % 200 == 0 {
            check_invariants(&game, START_LEVEL, SEED, frame);
        }
    }
    check_invariants(&game, START_LEVEL, SEED, 3_000);
}

// ---- 4. ポーズ中の凍結 (あるシードで 1 回だけ検査) ----

#[test]
fn paused_game_is_frozen_for_1000_frames() {
    const SEED: u32 = 0x0BAD_5EED;
    let mut input = InputGen::new(SEED);
    let mut game = Game::new(SEED, 5);
    // しばらくランダムに進めてからポーズする (START はここでは押させない)。
    for _ in 0..300 {
        let mut buttons = input.next_frame(false);
        buttons.start = false;
        game.update(buttons);
    }
    assert!(
        matches!(game.phase(), Phase::Playing),
        "前提: このシードでは 300 フレームでトップアウトしない"
    );
    game.update(Buttons::default());
    game.update(Buttons {
        start: true,
        ..Buttons::default()
    });
    assert_eq!(game.phase(), Phase::Paused);

    let board_before = game.board().clone();
    let active_before = game.active_piece().copied();
    let ghost_before = game.ghost_piece();
    let score_before = game.score();
    let lines_before = game.total_lines();
    let hold_before = game.hold_piece();
    let next_before: [Tetromino; NEXT_COUNT] = core::array::from_fn(|i| game.next(i));

    // START 以外の全ボタンを 1000 フレーム乱打しても一切進まないこと (§14.3)。
    for frame in 0..1_000 {
        let mut buttons = input.next_frame(false);
        buttons.start = false;
        game.update(buttons);
        assert_eq!(
            *game.events(),
            FrameEvents::default(),
            "ポーズ中にイベントが発生 (frame={frame})"
        );
        if frame % 250 == 0 {
            check_invariants(&game, 5, SEED, frame);
        }
    }
    assert_eq!(game.phase(), Phase::Paused);
    assert_eq!(game.board(), &board_before, "ポーズ中に盤面が変化");
    assert_eq!(
        game.active_piece().copied(),
        active_before,
        "ポーズ中に操作ミノが変化"
    );
    assert_eq!(game.ghost_piece(), ghost_before, "ポーズ中にゴーストが変化");
    assert_eq!(game.score(), score_before, "ポーズ中にスコアが変化");
    assert_eq!(
        game.total_lines(),
        lines_before,
        "ポーズ中に累計ラインが変化"
    );
    assert_eq!(game.hold_piece(), hold_before, "ポーズ中にホールドが変化");
    let next_after: [Tetromino; NEXT_COUNT] = core::array::from_fn(|i| game.next(i));
    assert_eq!(next_after, next_before, "ポーズ中にネクストが変化");

    // START の押し直しで再開できること。
    game.update(Buttons::default());
    game.update(Buttons {
        start: true,
        ..Buttons::default()
    });
    assert_eq!(game.phase(), Phase::Playing);
}
