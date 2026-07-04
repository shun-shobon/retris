//! ゲームステートマシン — 1 フレーム更新の統合 (仕様書 §14)。
//!
//! タイトル画面・レベル選択・シード収集は GBA 層の責務。本モジュールは
//! [`Game::new`] でシードと開始レベルを受け取り、毎フレーム [`Game::update`] に
//! 生ボタン状態 ([`Buttons`]) を渡すだけでプレイ全体が進行する。

use crate::active::ActivePiece;
use crate::bag::PieceQueue;
use crate::board::{Board, VISIBLE_HEIGHT};
use crate::input::Buttons;
use crate::lock_delay::LockDelay;
use crate::physics::{
    GravityAccumulator, SOFT_DROP_FACTOR, ghost, gravity_q16, is_grounded, try_fall, try_shift,
};
use crate::piece::Tetromino;
use crate::score::Scoring;
use crate::srs::{RotateDir, RotateOutcome, try_rotate};
use crate::tspin::{TSpin, detect_tspin};

/// DAS: 押下から初回リピートまでの遅延フレーム数 (仕様書 §12.2, §15)。
pub const DAS_FRAMES: u8 = 10;

/// ARR: リピート間隔フレーム数 (仕様書 §12.2, §15)。
pub const ARR_FRAMES: u8 = 2;

/// ゲームの状態 (仕様書 §14)。TITLE は GBA 層の責務のため持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// プレイ中 (§14.2)。
    Playing,
    /// ポーズ中 (§14.3)。状態を一切進めない。
    Paused,
    /// トップアウト後 (§13, §14.4)。リスタートは GBA 層が [`Game::new`] で行う。
    GameOver,
}

/// DAS/ARR の状態 (仕様書 §12.2)。ミノのロック・スポーンをまたいで保持する。
#[derive(Debug, Clone, Copy)]
struct Das {
    /// 現在有効な移動方向 (`-1` = 左, `0` = なし, `+1` = 右)。
    dir: i8,
    /// 次の自動リピート移動までの残りフレーム数。
    timer: u8,
}

/// このフレームで発生したロックの詳細 (HUD・効果音・演出用)。
///
/// ロックアウト (§13) で即ゲームオーバーになったロックでは発行されない
/// (スコア処理まで進まないため。[`FrameEvents::topped_out`] のみ立つ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockEvent {
    /// 消去ライン数 (0 = 消去なしロック)。
    pub lines_cleared: u8,
    /// T-Spin 判定 (§10)。
    pub tspin: TSpin,
    /// この消去に B2B 倍率 ×1.5 が掛かったか (§9.3)。
    pub b2b_applied: bool,
    /// この消去時点のコンボ数 (§9.4)。消去なしロックでは −1。
    pub combo: i16,
    /// Perfect Clear か (§9.6)。
    pub perfect_clear: bool,
    /// このロックでの加点 (§9.7)。ソフト/ハードドロップ点 (§9.5) は含まない。
    pub points_awarded: u32,
    /// このロックでレベルが上がったか (§11)。
    pub level_up: bool,
}

/// 直近の [`Game::update`] 1 フレームで起きたこと (HUD・効果音・演出用)。
///
/// 毎 update の冒頭でクリアされるため、GBA 層は update 直後に読み取ること。
/// ポーズ中・ゲームオーバー後の update ではすべて空になる。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameEvents {
    /// このフレームでロックが発生したときの詳細。
    pub locked: Option<LockEvent>,
    /// ホールドが実行された (§6.2)。
    pub hold_performed: bool,
    /// ハードドロップが実行された (§7.4)。0 セル落下の据え置きロックも含む。
    pub hard_dropped: bool,
    /// 回転が成功した (§4)。不発・O ミノの回転入力では立たない。
    pub rotated: bool,
    /// 左右移動が成功した (§12.2)。壁などによる移動失敗では立たない。
    pub shifted: bool,
    /// このフレームのソフトドロップによる落下行数 (§7.3)。
    pub soft_drop_rows: u8,
    /// このフレームで GameOver に遷移した (§13)。
    pub topped_out: bool,
}

/// ゲーム全体の状態 (仕様書 §14)。
///
/// 毎フレーム [`Self::update`] を 1 回呼ぶこと。処理順は §14.2 に従う。
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    queue: PieceQueue,
    active: ActivePiece,
    hold: Option<Tetromino>,
    hold_used: bool,
    lock_delay: LockDelay,
    gravity: GravityAccumulator,
    scoring: Scoring,
    das: Das,
    prev_buttons: Buttons,
    /// 次の [`Self::update`] が初回か。初回はエッジの飲み込みを行う (§12.3)。
    first_update: bool,
    /// 最後に成功した操作が回転か (§10.1-2)。
    last_action_was_rotation: bool,
    /// 最後の回転で成立したキックテスト番号 1〜5 (§10.3 のキック 5 例外用)。
    last_kick: u8,
    phase: Phase,
    /// 直近の update で起きたこと (update 冒頭でクリア)。
    events: FrameEvents,
}

impl Game {
    /// シードと開始レベル (1〜15、§11) から新規ゲームを生成する。
    ///
    /// ネクストキューを生成し、最初のミノを §3.2 の手順でスポーンする。
    ///
    /// 初回 `update` は渡されたボタン状態をそのまま「前フレーム状態」として扱う
    /// (エッジの飲み込み)。タイトル画面から押しっぱなしの START / 上 / ホールド /
    /// 回転はエッジ発火せず、押し直しが必要 (§12.3 の誤爆防止)。左右・下の保持系は
    /// 初回から有効で、押しっぱなしの左右は初回フレームで新規押下として扱われる
    /// (1 セル移動 + DAS チャージ 0 から)。重力等の時間進行は初回から通常どおり。
    #[must_use]
    pub fn new(seed: u32, start_level: u32) -> Self {
        let mut queue = PieceQueue::new(seed);
        let first = queue.pop();
        let mut game = Self {
            board: Board::new(),
            queue,
            active: ActivePiece::spawn(first),
            hold: None,
            hold_used: false,
            lock_delay: LockDelay::new(0),
            gravity: GravityAccumulator::new(),
            scoring: Scoring::new(start_level),
            das: Das {
                dir: 0,
                timer: DAS_FRAMES,
            },
            prev_buttons: Buttons::default(),
            first_update: true,
            last_action_was_rotation: false,
            last_kick: 0,
            phase: Phase::Playing,
            events: FrameEvents::default(),
        };
        game.spawn(first);
        game
    }

    /// 1 フレーム進める (仕様書 §14.2 の処理順)。
    ///
    /// 1. エッジ検出 (前フレームとの差分)
    /// 2. START エッジ → PAUSED 往復 (以降スキップ)
    /// 3. ホールド / ハードドロップ (排他、ホールド優先)
    /// 4. 回転 (A=CW, B=CCW、エッジのみ)
    /// 5. 左右移動 (DAS/ARR)
    /// 6. 重力 / ソフトドロップ
    /// 7. 接地判定・ロックディレイ → ロック処理
    ///
    /// PAUSED 中は START エッジ検出以外なにも進めない (§14.3)。
    /// GAME OVER 後は何もしない (§14.4)。
    ///
    /// 初回呼び出しは「エッジの飲み込み」を行う: 渡された `buttons` をそのまま
    /// 前フレーム状態とみなし、押しっぱなしのボタンをエッジと誤検出しない
    /// (詳細は [`Self::new`] のドキュメント)。
    pub fn update(&mut self, buttons: Buttons) {
        // (0) フレームイベントのクリア。GBA 層は update 直後に読み取ること。
        self.events = FrameEvents::default();
        if matches!(self.phase, Phase::GameOver) {
            return;
        }
        // (1) エッジ検出の基準となる前フレーム状態。初回はタイトル画面などから
        // 持ち越された押しっぱなしをエッジ発火させないため buttons 自身を使う。
        let prev = if self.first_update {
            self.first_update = false;
            buttons
        } else {
            self.prev_buttons
        };
        self.prev_buttons = buttons;

        // (2) ポーズ往復 (§14.3)。エッジのフレーム自体は他の処理を進めない。
        if buttons.start && !prev.start {
            self.phase = match self.phase {
                Phase::Paused => Phase::Playing,
                Phase::Playing | Phase::GameOver => Phase::Paused,
            };
            return;
        }
        if matches!(self.phase, Phase::Paused) {
            return;
        }

        // (3) ホールド / ハードドロップ (§6.2, §7.4)。同一フレームではホールド優先。
        // ホールドが実行されなかった (hold_used 中の) 場合のみハードドロップに落ちる。
        if buttons.hold && !prev.hold && !self.hold_used {
            self.do_hold();
        } else if buttons.up && !prev.up {
            self.hard_drop();
        }
        if matches!(self.phase, Phase::GameOver) {
            return; // ブロックアウト / ロックアウト (§13)
        }

        // (4) 回転 (§4)。エッジのみ。同一フレームの両押しは CW を優先。
        if buttons.rotate_cw && !prev.rotate_cw {
            self.rotate(RotateDir::Cw);
        } else if buttons.rotate_ccw && !prev.rotate_ccw {
            self.rotate(RotateDir::Ccw);
        }

        // (5) 左右移動 (DAS/ARR §12.2)
        self.horizontal_move(buttons, prev);

        // (6) 重力 / ソフトドロップ (§7)
        self.apply_gravity(buttons.down);

        // (7) 接地判定・ロックディレイ更新 (§8) → ロック処理 (§8.3)
        let grounded = is_grounded(&self.board, &self.active);
        if grounded {
            self.gravity.reset(); // §7.2: 接地中はアキュムレータをクリア
        }
        if self.lock_delay.frame_update(grounded) {
            self.lock_active();
        }
    }

    // ---- 描画用アクセサ (§14.2-8) ----

    /// フィールド (固定済みブロック)。
    #[must_use]
    pub const fn board(&self) -> &Board {
        &self.board
    }

    /// 操作中のミノ。ゲームオーバー後は `None`。
    #[must_use]
    pub fn active_piece(&self) -> Option<&ActivePiece> {
        match self.phase {
            Phase::GameOver => None,
            Phase::Playing | Phase::Paused => Some(&self.active),
        }
    }

    /// ゴーストピース位置 = 現在位置から純落下させた位置 (§14.2-8)。
    /// ゲームオーバー後は `None`。
    #[must_use]
    pub fn ghost_piece(&self) -> Option<ActivePiece> {
        self.active_piece().map(|piece| ghost(&self.board, piece))
    }

    /// ホールド枠のミノ (§6.2)。
    #[must_use]
    pub const fn hold_piece(&self) -> Option<Tetromino> {
        self.hold
    }

    /// ネクスト `i` 番目 (`0..NEXT_COUNT`) のミノ (§6.1)。
    ///
    /// # Panics
    ///
    /// `i >= NEXT_COUNT` のとき ([`crate::bag::PieceQueue::peek`] に準ずる)。
    #[must_use]
    pub fn next(&self, i: usize) -> Tetromino {
        self.queue.peek(i)
    }

    /// 現在のスコア (§9)。
    #[must_use]
    pub const fn score(&self) -> u32 {
        self.scoring.score()
    }

    /// 表示用レベル (§11)。
    #[must_use]
    pub const fn level(&self) -> u32 {
        self.scoring.level()
    }

    /// 累計消去ライン数 (§11)。
    #[must_use]
    pub const fn total_lines(&self) -> u32 {
        self.scoring.total_lines()
    }

    /// 現在のゲーム状態 (§14)。
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// 直近の [`Self::update`] で起きたイベント (HUD・効果音・演出用)。
    ///
    /// update 冒頭でクリアされるため、毎フレーム update 直後に読み取ること。
    /// 一度も update していない場合はすべて空。
    #[must_use]
    pub const fn events(&self) -> &FrameEvents {
        &self.events
    }

    /// コンボカウンタ (§9.4)。−1 = コンボなし。
    #[must_use]
    pub const fn combo(&self) -> i16 {
        self.scoring.combo()
    }

    /// B2B チェーン継続中か (§9.3)。
    #[must_use]
    pub const fn b2b(&self) -> bool {
        self.scoring.b2b()
    }

    // ---- 内部処理 ----

    /// §3.2 のスポーン処理: ブロックアウト判定 → 1 行落下試行 → 各状態リセット。
    ///
    /// ミノごとの状態 (ロックディレイ・重力アキュムレータ・T-Spin フラグ) は
    /// リセットするが、DAS チャージは意図的に保持する (§12.2)。
    fn spawn(&mut self, kind: Tetromino) {
        let piece = ActivePiece::spawn(kind);
        if !self.board.fits(&piece) {
            // ブロックアウト (§13): 生成位置が既存ブロックと重なる。
            self.phase = Phase::GameOver;
            self.events.topped_out = true;
            return;
        }
        let piece = try_fall(&self.board, &piece).unwrap_or(piece);
        self.active = piece;
        self.gravity.reset();
        self.lock_delay = LockDelay::new(piece.y);
        self.last_action_was_rotation = false;
        self.last_kick = 0;
    }

    /// ロック処理 (§8.3): 固定 → ロックアウト判定 → T-Spin 判定 (消去前盤面) →
    /// ライン消去 → Perfect Clear 判定 → スコア加算 → ホールド解禁 → 次ミノスポーン。
    fn lock_active(&mut self) {
        let piece = self.active;
        self.board.place(&piece);

        // ロックアウト (§13): 4 セルすべてが可視領域外 (y >= 20) なら即ゲームオーバー。
        const VISIBLE_TOP: i8 = VISIBLE_HEIGHT as i8;
        if piece.cells().iter().all(|&(_, y)| y >= VISIBLE_TOP) {
            self.phase = Phase::GameOver;
            self.events.topped_out = true;
            return;
        }

        let tspin = detect_tspin(
            &self.board,
            &piece,
            self.last_action_was_rotation,
            self.last_kick,
        );
        let lines = self.board.clear_full_lines();
        let perfect_clear = lines > 0 && self.board.is_empty();
        // イベント発行 (HUD・SE 用): B2B 適用とレベルは on_lock による更新前後の
        // 比較で確定するため、この順序で評価する。
        let level_before = self.scoring.level();
        let b2b_applied = self.scoring.b2b_applies(lines, tspin);
        let points_awarded = self.scoring.on_lock(lines, tspin, perfect_clear);
        self.events.locked = Some(LockEvent {
            lines_cleared: lines,
            tspin,
            b2b_applied,
            combo: self.scoring.combo(),
            perfect_clear,
            points_awarded,
            level_up: self.scoring.level() > level_before,
        });
        self.hold_used = false; // §6.2: ロックで再ホールド可能に
        let next = self.queue.pop();
        self.spawn(next);
    }

    /// ホールド (§6.2): 枠が空ならキューから、あれば交換でスポーンする。
    fn do_hold(&mut self) {
        let stored = self.hold.replace(self.active.kind);
        self.hold_used = true;
        self.events.hold_performed = true;
        let next = stored.unwrap_or_else(|| self.queue.pop());
        self.spawn(next);
    }

    /// ハードドロップ (§7.4): 接地位置まで即落下し、ロックディレイを無視して即ロック。
    fn hard_drop(&mut self) {
        self.events.hard_dropped = true;
        let target = ghost(&self.board, &self.active);
        let cells = u32::from((self.active.y - target.y).unsigned_abs());
        self.scoring.add_hard_drop_cells(cells);
        if cells > 0 {
            // 1 セル以上落下したら回転フラグを解除 (§10.1-2)。
            // 0 セル (据え置き) なら回転直後の状態を維持する。
            self.last_action_was_rotation = false;
        }
        self.active = target;
        self.lock_active();
    }

    /// 回転 (§4)。成功時のみフラグ更新とロックディレイのリセットを行う。
    fn rotate(&mut self, dir: RotateDir) {
        match try_rotate(&self.board, &self.active, dir) {
            RotateOutcome::Rotated { piece, kick } => {
                self.active = piece;
                self.last_action_was_rotation = true;
                self.last_kick = kick;
                self.events.rotated = true;
                self.lock_delay
                    .notify_move(is_grounded(&self.board, &piece));
            }
            // 不発と O ミノは状態を一切変えない (§4.4, §8.2-6)。
            RotateOutcome::Blocked | RotateOutcome::IgnoredO => {}
        }
    }

    /// 左右移動 (DAS/ARR §12.2)。
    ///
    /// 押下フレームで 1 セル、以後 [`DAS_FRAMES`] 後に 2 セル目、以後
    /// [`ARR_FRAMES`] ごとに 1 セル。移動失敗 (壁) でもタイマーは進める (壁チャージ)。
    fn horizontal_move(&mut self, buttons: Buttons, prev: Buttons) {
        let dir = self.resolve_direction(buttons, prev);
        if dir != self.das.dir {
            // 新規押下・方向切替: チャージは 0 から (§12.2)。押下フレームで 1 セル。
            self.das = Das {
                dir,
                timer: DAS_FRAMES,
            };
            if dir != 0 {
                self.shift(dir);
            }
        } else if dir != 0 {
            self.das.timer = self.das.timer.saturating_sub(1);
            if self.das.timer == 0 {
                self.das.timer = ARR_FRAMES;
                self.shift(dir);
            }
        }
    }

    /// 今フレームの移動方向を決める (§12.2)。左右同時押しは後から押した方を優先。
    fn resolve_direction(&self, buttons: Buttons, prev: Buttons) -> i8 {
        match (buttons.left, buttons.right) {
            (false, false) => 0,
            (true, false) => -1,
            (false, true) => 1,
            (true, true) => {
                let left_edge = !prev.left;
                let right_edge = !prev.right;
                if left_edge && !right_edge {
                    -1
                } else if right_edge && !left_edge {
                    1
                } else {
                    // 両方押下継続中は現在の方向を維持。同一フレームの同時押下は
                    // 「後から押した方」を判別できないため移動なしとする。
                    self.das.dir
                }
            }
        }
    }

    /// 1 セルの左右移動を試みる。成功時のみフラグ更新とロックディレイのリセット。
    fn shift(&mut self, dx: i8) {
        if let Some(moved) = try_shift(&self.board, &self.active, dx) {
            self.active = moved;
            self.last_action_was_rotation = false; // §10.1-2
            self.events.shifted = true;
            self.lock_delay
                .notify_move(is_grounded(&self.board, &moved)); // §8.2-2
        }
        // 失敗はフラグ・ロックディレイとも変更しない (§8.2-6)。
    }

    /// 重力 / ソフトドロップ (§7)。下キー保持中は G×20 とし、落下 1 セルごとに
    /// 1 点加算する (§7.3。接地中は落下しないため自然に加点なし)。
    fn apply_gravity(&mut self, soft_drop: bool) {
        let mut gravity = gravity_q16(self.scoring.effective_level());
        if soft_drop {
            gravity *= SOFT_DROP_FACTOR;
        }
        let rows = self.gravity.tick(gravity);
        for _ in 0..rows {
            let Some(fallen) = try_fall(&self.board, &self.active) else {
                self.gravity.reset(); // §7.2: 接地でクリア
                break;
            };
            self.active = fallen;
            self.last_action_was_rotation = false; // §10.1-2: 1 行落下で解除
            self.lock_delay.notify_fall(fallen.y); // §8.2-5
            if soft_drop {
                self.scoring.add_soft_drop_cells(1);
                self.events.soft_drop_rows += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::Rotation;

    /// seed=1 のミノ順は T, Z, O, J, S, L, I, S, J, L, Z, T, I, O
    /// (bag.rs のテスト済み挙動からの転記)。
    const SEED: u32 = 1;

    fn game() -> Game {
        Game::new(SEED, 1)
    }

    fn idle() -> Buttons {
        Buttons::default()
    }

    fn left() -> Buttons {
        Buttons {
            left: true,
            ..Buttons::default()
        }
    }

    fn right() -> Buttons {
        Buttons {
            right: true,
            ..Buttons::default()
        }
    }

    fn down() -> Buttons {
        Buttons {
            down: true,
            ..Buttons::default()
        }
    }

    fn up() -> Buttons {
        Buttons {
            up: true,
            ..Buttons::default()
        }
    }

    fn cw() -> Buttons {
        Buttons {
            rotate_cw: true,
            ..Buttons::default()
        }
    }

    fn ccw() -> Buttons {
        Buttons {
            rotate_ccw: true,
            ..Buttons::default()
        }
    }

    fn hold() -> Buttons {
        Buttons {
            hold: true,
            ..Buttons::default()
        }
    }

    fn start() -> Buttons {
        Buttons {
            start: true,
            ..Buttons::default()
        }
    }

    /// 同じ入力で `frames` 回 update する。
    fn run(game: &mut Game, buttons: Buttons, frames: u32) {
        for _ in 0..frames {
            game.update(buttons);
        }
    }

    #[track_caller]
    fn active(game: &Game) -> ActivePiece {
        *game.active_piece().expect("操作中ミノがあるはず")
    }

    // ---- 初期スポーン (§3.1, §3.2, §6.1) ----

    #[test]
    fn new_game_spawns_first_piece_one_row_fallen() {
        let game = game();
        let piece = active(&game);
        assert_eq!(piece.kind, Tetromino::T, "seed=1 の先頭ミノは T");
        assert_eq!(piece.rot, Rotation::Spawn);
        // §3.1 の (3,19) から §3.2-3 の 1 行落下を済ませた位置。
        assert_eq!((piece.x, piece.y), (3, 18));
        assert_eq!(game.phase(), Phase::Playing);
        assert_eq!(game.score(), 0);
        assert_eq!(game.total_lines(), 0);
        assert_eq!(game.level(), 1);
        assert_eq!(game.hold_piece(), None);
    }

    #[test]
    fn new_game_shows_five_next_pieces() {
        use Tetromino::{J, L, O, S, Z};
        let game = game();
        let next: [Tetromino; 5] = core::array::from_fn(|i| game.next(i));
        assert_eq!(next, [Z, O, J, S, L]);
    }

    #[test]
    fn new_game_respects_start_level() {
        assert_eq!(Game::new(SEED, 5).level(), 5);
    }

    // ---- ゲーム開始フレームのエッジ飲み込み (§12.3, §14.3) ----

    #[test]
    fn first_update_ignores_held_start() {
        let mut game = game();
        game.update(start()); // f1: タイトル画面から持ち越した START
        assert_eq!(
            game.phase(),
            Phase::Playing,
            "初回フレームの押しっぱなし START でポーズしない"
        );
        game.update(idle()); // f2: 離す
        game.update(start()); // f3: 押し直しで発火
        assert_eq!(game.phase(), Phase::Paused);
    }

    #[test]
    fn first_update_ignores_held_hard_drop() {
        let mut game = game();
        game.update(up()); // f1: 押しっぱなしの上キー (§12.3)
        assert_eq!(active(&game).kind, Tetromino::T, "初回フレームでは無視");
        assert_eq!(game.score(), 0);
        game.update(idle()); // f2: 離す
        game.update(up()); // f3: 押し直しで 19 セルのハードドロップ
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(game.score(), 38);
    }

    #[test]
    fn first_update_ignores_held_hold() {
        let mut game = game();
        game.update(hold()); // f1
        assert_eq!(game.hold_piece(), None, "初回フレームでは無視");
        assert_eq!(active(&game).kind, Tetromino::T);
        game.update(idle()); // f2: 離す
        game.update(hold()); // f3: 押し直しで発火
        assert_eq!(game.hold_piece(), Some(Tetromino::T));
        assert_eq!(active(&game).kind, Tetromino::Z);
    }

    #[test]
    fn first_update_ignores_held_rotation() {
        let mut game = game();
        game.update(cw()); // f1
        assert_eq!(active(&game).rot, Rotation::Spawn, "初回フレームでは無視");
        game.update(idle()); // f2: 離す
        game.update(cw()); // f3: 押し直しで発火
        assert_eq!(active(&game).rot, Rotation::Cw);
    }

    #[test]
    fn held_buttons_from_first_update_do_not_fire_later() {
        let mut game = game();
        let held = Buttons {
            up: true,
            hold: true,
            start: true,
            rotate_cw: true,
            ..Buttons::default()
        };
        run(&mut game, held, 5); // f1-5: 保持継続中はエッジが立たない
        assert_eq!(game.phase(), Phase::Playing);
        assert_eq!(game.hold_piece(), None);
        assert_eq!(active(&game).kind, Tetromino::T);
        assert_eq!(active(&game).rot, Rotation::Spawn);
        assert_eq!(game.score(), 0);
    }

    #[test]
    fn first_update_held_direction_acts_as_fresh_press() {
        // 保持系の左右移動は初回フレームから有効: 押しっぱなしを「このフレームで
        // 押下された」ものとして扱い、1 セル移動 + DAS チャージは 0 から。
        let mut game = game();
        game.update(right()); // f1: 即 1 セル
        assert_eq!(active(&game).x, 4);
        run(&mut game, right(), 9); // f2-10: DAS 充填中
        assert_eq!(active(&game).x, 4);
        game.update(right()); // f11: DAS 満了で 2 セル目
        assert_eq!(active(&game).x, 5);
    }

    // ---- DAS / ARR (§12.2) ----

    #[test]
    fn das_repeats_at_frames_1_11_13_15() {
        let mut game = game();
        // 押下フレームを 1 として、移動フレームは 1, 11, 13, 15 (§12.2)。
        // T は bx=7 で右壁に密着し、以降は移動失敗 (壁チャージ継続)。
        let expected_x = [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 6, 6, 7, 7, 7];
        for (frame, &x) in (1..).zip(&expected_x) {
            game.update(right());
            assert_eq!(active(&game).x, x, "フレーム {frame}");
        }
    }

    #[test]
    fn das_direction_switch_restarts_charge() {
        let mut game = game();
        run(&mut game, right(), 5); // f1-5: f1 に x=4
        assert_eq!(active(&game).x, 4);
        game.update(left()); // f6: 方向切替 → 即 1 セル、チャージは 0 から
        assert_eq!(active(&game).x, 3);
        run(&mut game, left(), 9); // f7-15: DAS 充填中
        assert_eq!(
            active(&game).x,
            3,
            "切替後 10 フレーム経過前にリピートしない"
        );
        game.update(left()); // f16: 切替から 10 フレームで 2 セル目
        assert_eq!(active(&game).x, 2);
    }

    #[test]
    fn das_simultaneous_hold_prefers_later_press() {
        let both = Buttons {
            left: true,
            right: true,
            ..Buttons::default()
        };
        let mut game = game();
        run(&mut game, right(), 2); // f1-2: x=4
        game.update(both); // f3: 後から押した左を優先 (§12.2)
        assert_eq!(active(&game).x, 3);
        run(&mut game, both, 9); // f4-12: 左のチャージ中
        assert_eq!(active(&game).x, 3);
        game.update(both); // f13: 左の DAS 満了
        assert_eq!(active(&game).x, 2);
        game.update(right()); // f14: 左を離す → 右へ切替で即 1 セル
        assert_eq!(active(&game).x, 3);
        run(&mut game, right(), 9); // f15-23: 右のチャージ中
        assert_eq!(active(&game).x, 3);
        game.update(right()); // f24: 右の DAS 満了
        assert_eq!(active(&game).x, 4);
    }

    #[test]
    fn das_wall_charge_flows_immediately_when_opened() {
        let mut game = game();
        // f1-19: f13 に左壁 (bx=0) へ到達、以降も失敗しつつタイマー進行 (壁チャージ)。
        run(&mut game, left(), 19);
        assert_eq!(active(&game).x, 0);
        // f20: CW 回転で左シルエットが x=1..2 列に縮み、bx=-1 が空く。
        game.update(Buttons {
            left: true,
            rotate_cw: true,
            ..Buttons::default()
        });
        assert_eq!(active(&game).rot, Rotation::Cw);
        assert_eq!(active(&game).x, 0);
        game.update(left()); // f21: 次の ARR 周期で即座に流れる
        assert_eq!(active(&game).x, -1);
    }

    #[test]
    fn das_charge_survives_hard_drop_and_spawn() {
        let mut game = game();
        run(&mut game, left(), 19); // f1-19: 左壁で壁チャージ
        assert_eq!(active(&game).x, 0);
        game.update(Buttons {
            left: true,
            up: true,
            ..Buttons::default()
        }); // f20: (0,18) から 19 セルのハードドロップ → 即ロック
        assert_eq!(game.score(), 38);
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(active(&game).x, 3);
        // チャージ保持 (§12.2): 新ミノがスポーン直後から ARR 周期で走る。
        game.update(left()); // f21
        assert_eq!(active(&game).x, 2);
        game.update(left()); // f22
        assert_eq!(active(&game).x, 2);
        game.update(left()); // f23
        assert_eq!(active(&game).x, 1);
        run(&mut game, left(), 2); // f24-25
        assert_eq!(active(&game).x, 0);
    }

    #[test]
    fn das_charge_survives_hold_spawn() {
        let mut game = game();
        run(&mut game, left(), 13); // f1-13: x=0 に到達 (f13 移動、timer=ARR)
        assert_eq!(active(&game).x, 0);
        game.update(Buttons {
            left: true,
            hold: true,
            ..Buttons::default()
        }); // f14: ホールド → Z がスポーン
        assert_eq!(game.hold_piece(), Some(Tetromino::T));
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(active(&game).x, 3);
        game.update(left()); // f15: チャージ保持 → 即 ARR 周期
        assert_eq!(active(&game).x, 2);
        run(&mut game, left(), 2); // f16-17
        assert_eq!(active(&game).x, 1);
    }

    // ---- 回転 (§4, §12.3) ----

    #[test]
    fn rotation_fires_once_per_press_edge() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(cw()); // f1: エッジで 1 回
        assert_eq!(active(&game).rot, Rotation::Cw);
        game.update(cw()); // f2: 保持継続では回らない
        assert_eq!(active(&game).rot, Rotation::Cw);
        game.update(idle()); // f3: 離す
        game.update(cw()); // f4: 再押下で回る
        assert_eq!(active(&game).rot, Rotation::Flip);
    }

    #[test]
    fn rotation_flag_set_and_cleared_by_move_and_fall() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(cw()); // f1: 回転成功
        assert!(game.last_action_was_rotation);
        game.update(right()); // f2: 移動成功でフラグ解除 (§10.1-2)
        assert!(!game.last_action_was_rotation);
        game.update(cw()); // f3: 再び回転
        assert!(game.last_action_was_rotation);
        run(&mut game, down(), 3); // f4-6: f6 に 1 セル落下
        assert_eq!(active(&game).y, 17);
        assert!(!game.last_action_was_rotation, "1 行落下でフラグ解除");
    }

    #[test]
    fn failed_shift_keeps_last_action_rotation_flag() {
        let mut game = game();
        run(&mut game, left(), 13); // 左壁 (bx=0) へ
        assert_eq!(active(&game).x, 0);
        game.update(idle()); // f14
        game.update(ccw()); // f15: CCW はキックなしで成立
        assert!(game.last_action_was_rotation);
        game.update(left()); // f16: bx=-1 は (0,1) セルが壁に食い込み失敗
        assert_eq!(active(&game).x, 0);
        assert!(
            game.last_action_was_rotation,
            "移動失敗ではフラグを変更しない (§10.1-2)"
        );
    }

    #[test]
    fn o_rotation_input_changes_nothing() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: T ロック → Z
        game.update(idle()); // f2
        game.update(up()); // f3: Z ロック → O
        game.update(idle()); // f4
        let before = active(&game);
        assert_eq!(before.kind, Tetromino::O);
        game.update(cw()); // f5: O の回転入力 (§4.4)
        assert_eq!(active(&game), before, "O は回転入力で状態・位置とも不変");
        assert!(!game.last_action_was_rotation);
    }

    // ---- ソフトドロップ (§7.3) ----

    #[test]
    fn soft_drop_at_level_1_falls_every_3_frames_and_scores() {
        let mut game = game();
        run(&mut game, down(), 2); // f1-2: G×20 でもまだ 1 行未満
        assert_eq!((active(&game).y, game.score()), (18, 0));
        game.update(down()); // f3: 1 セル落下 + 1 点
        assert_eq!((active(&game).y, game.score()), (17, 1));
        run(&mut game, down(), 3); // f4-6: f6 に 2 セル目
        assert_eq!((active(&game).y, game.score()), (16, 2));
    }

    #[test]
    fn soft_drop_while_grounded_adds_no_points() {
        let mut game = game();
        game.board.set(4, 16, Some(Tetromino::J));
        run(&mut game, down(), 6); // f1-6: 2 セル落下して (3,16) で接地
        assert_eq!((active(&game).y, game.score()), (16, 2));
        run(&mut game, down(), 14); // f7-20: 接地中の下キー保持
        assert_eq!(game.score(), 2, "接地中は落下しないため加点なし (§7.3)");
        assert_eq!(active(&game).y, 16);
    }

    // ---- ハードドロップ (§7.4, §12.3) ----

    #[test]
    fn hard_drop_locks_immediately_and_scores_2_per_cell() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: y=18 → -1 の 19 セル
        assert_eq!(game.score(), 38);
        for (x, y) in [(3, 0), (4, 0), (5, 0), (4, 1)] {
            assert_eq!(game.board().get(x, y), Some(Tetromino::T), "({x}, {y})");
        }
        let piece = active(&game);
        assert_eq!(piece.kind, Tetromino::Z, "同一フレームで次ミノがスポーン");
        assert_eq!((piece.x, piece.y), (3, 18));
    }

    #[test]
    fn hard_drop_requires_new_press_for_next_piece() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: T ロック → Z
        game.update(up()); // f2: 押しっぱなしでは発動しない (§12.3)
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(game.score(), 38);
        game.update(idle()); // f3: 離す
        game.update(up()); // f4: 再押下で Z が 17 セル落下してロック
        assert_eq!(active(&game).kind, Tetromino::O);
        assert_eq!(game.score(), 38 + 34);
    }

    // ---- ロックディレイ統合 (§8) ----

    #[test]
    fn grounded_piece_locks_after_30_frames() {
        let mut game = game();
        game.board.set(4, 16, Some(Tetromino::J));
        run(&mut game, down(), 6); // f1-6: (3,16) で接地 (接地 1 フレーム目)
        run(&mut game, idle(), 28); // f7-34: 接地 29 フレーム目まで
        assert_eq!(active(&game).kind, Tetromino::T, "29 フレームでは未ロック");
        game.update(idle()); // f35: 接地 30 フレーム目で満了ロック
        assert_eq!(active(&game).kind, Tetromino::Z);
        for (x, y) in [(3, 17), (4, 17), (5, 17), (4, 18)] {
            assert_eq!(game.board().get(x, y), Some(Tetromino::T), "({x}, {y})");
        }
    }

    #[test]
    fn grounded_shift_resets_lock_timer() {
        let mut game = game();
        game.board.set(4, 16, Some(Tetromino::J));
        run(&mut game, down(), 6); // f1-6: 接地
        run(&mut game, idle(), 13); // f7-19
        game.update(right()); // f20: 接地中の移動成功 → タイマー 30 に回復 (§8.2-2)
        assert_eq!(active(&game).x, 4);
        run(&mut game, idle(), 28); // f21-48: リセットなしなら f35 でロック済み
        assert_eq!(active(&game).kind, Tetromino::T, "移動リセット後は生存");
        game.update(idle()); // f49: 回復後 30 フレームで満了ロック
        assert_eq!(active(&game).kind, Tetromino::Z);
    }

    // ---- ホールド (§6.2) ----

    #[test]
    fn first_hold_stores_piece_and_spawns_from_queue() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1
        assert_eq!(game.hold_piece(), Some(Tetromino::T));
        let piece = active(&game);
        assert_eq!(piece.kind, Tetromino::Z, "キュー先頭からスポーン");
        assert_eq!((piece.x, piece.y), (3, 18), "§3.2 のスポーンフロー適用");
    }

    #[test]
    fn hold_is_ignored_until_next_lock() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1: T → ホールド、Z スポーン
        game.update(idle()); // f2
        game.update(hold()); // f3: ロック前の再ホールドは無視 (§6.2)
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(game.hold_piece(), Some(Tetromino::T));
    }

    #[test]
    fn hold_swaps_after_lock() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1: hold=T、Z スポーン
        game.update(idle()); // f2
        game.update(up()); // f3: Z ロック → hold_used 解除、O スポーン
        game.update(idle()); // f4
        game.update(hold()); // f5: 交換 → hold=O、T が再スポーン
        assert_eq!(game.hold_piece(), Some(Tetromino::O));
        let piece = active(&game);
        assert_eq!(piece.kind, Tetromino::T);
        assert_eq!((piece.x, piece.y), (3, 18));
    }

    // ---- T-Spin フロー統合 (§9.2, §10) ----

    /// TSD 地形を作る (tspin.rs の tsd_setup と同型):
    ///   y=2: . . . S . . . . . .   ← (3,2) が庇
    ///   y=1: L L L _ _ _ L L L L
    ///   y=0: J J J J _ J J J J J
    fn build_tsd_field(game: &mut Game) {
        for x in 0..10 {
            if x != 4 {
                game.board.set(x, 0, Some(Tetromino::J));
            }
            if !(3..=5).contains(&x) {
                game.board.set(x, 1, Some(Tetromino::L));
            }
        }
        game.board.set(3, 2, Some(Tetromino::S));
    }

    #[test]
    fn tspin_double_flow_awards_full_tspin_score() {
        let mut game = game(); // 先頭ミノは T
        build_tsd_field(&mut game);

        run(&mut game, down(), 48); // f1-48: 16 セル落下して庇の上 (by=2) に接地
        assert_eq!((active(&game).y, game.score()), (2, 16));
        game.update(Buttons {
            down: true,
            rotate_cw: true,
            ..Buttons::default()
        }); // f49: 0→R 回転で庇の右下の隙間に入る
        assert_eq!(active(&game).rot, Rotation::Cw);
        run(&mut game, down(), 5); // f50-54: f51, f54 に落下してスロット底 (by=0) へ
        assert_eq!((active(&game).y, game.score()), (0, 18));
        game.update(cw()); // f55: R→2 回転が最終操作 (キック 1)
        assert_eq!(active(&game).rot, Rotation::Flip);
        game.update(idle()); // f56
        game.update(up()); // f57: 落下 0 セルの据え置きハードドロップ → 即ロック
        // 消去前盤面での判定によりフル T-Spin Double = 1200 × level 1 (§9.2, §10.3)。
        // 落下 0 セルなので last_action_rotation は維持される (§7.4)。
        assert_eq!(game.score(), 18 + 1200);
        assert_eq!(game.total_lines(), 2);
        assert_eq!(active(&game).kind, Tetromino::Z);
        // 2 行消去で庇の S が y=0 に落ちる。
        assert_eq!(game.board().get(3, 0), Some(Tetromino::S));
        assert_eq!(game.board().get(4, 0), None);
    }

    // ---- ライン消去統合 (§9, §11) ----

    #[test]
    fn line_clear_updates_score_lines_and_level_multiplier() {
        let mut game = Game::new(SEED, 2); // 開始レベル 2 で倍率確認
        for x in [0, 1, 2, 6, 7, 8, 9] {
            game.board.set(x, 0, Some(Tetromino::L));
        }
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // T が x=3..5 を埋めて Single
        // ハードドロップ 19 セル × 2 点 + Single 100 × level 2 (§9.2, §9.5)。
        assert_eq!(game.score(), 38 + 200);
        assert_eq!(game.total_lines(), 1);
        assert_eq!(game.level(), 2);
        // 消去で上の行が詰められ、T の残りセル (4,1) が (4,0) に落ちる。
        assert_eq!(game.board().get(4, 0), Some(Tetromino::T));
        assert_eq!(game.board().get(0, 0), None);
    }

    // ---- トップアウト (§13) ----

    #[test]
    fn block_out_on_spawn_is_game_over() {
        let mut game = game();
        game.board.set(3, 21, Some(Tetromino::J)); // 次の Z のスポーンセル (§3.1)
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // T ロック → Z のスポーンが重なる
        assert_eq!(game.phase(), Phase::GameOver);
        assert_eq!(game.active_piece(), None);
        assert_eq!(game.ghost_piece(), None);
    }

    #[test]
    fn block_out_on_hold_spawn_is_game_over() {
        let mut game = game();
        game.board.set(3, 21, Some(Tetromino::J));
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // ホールド由来の Z スポーンでもブロックアウト (§6.2)
        assert_eq!(game.phase(), Phase::GameOver);
    }

    #[test]
    fn lock_out_fully_above_visible_area_is_game_over() {
        let mut game = game();
        game.board.set(4, 19, Some(Tetromino::J)); // Z のスポーン直後落下を塞ぐ
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1: Z が (3,19) のまま接地 (全セル y>=20)
        assert_eq!(active(&game).y, 19);
        game.update(up()); // f2: 0 セルハードドロップでロック → ロックアウト
        assert_eq!(game.phase(), Phase::GameOver);
    }

    #[test]
    fn game_over_updates_are_inert() {
        let mut game = game();
        game.board.set(3, 21, Some(Tetromino::J));
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up());
        assert_eq!(game.phase(), Phase::GameOver);
        let score = game.score();
        let board = game.board().clone();
        let chaos = Buttons {
            left: true,
            down: true,
            up: true,
            rotate_cw: true,
            hold: true,
            start: true,
            ..Buttons::default()
        };
        run(&mut game, chaos, 30);
        run(&mut game, idle(), 5);
        assert_eq!(game.phase(), Phase::GameOver, "START でも復帰しない");
        assert_eq!(game.score(), score);
        assert_eq!(game.board(), &board);
        assert_eq!(game.active_piece(), None);
    }

    // ---- ポーズ (§14.3) ----

    #[test]
    fn start_toggles_pause_and_freezes_everything() {
        let mut game = game();
        game.update(right()); // f1: x=4
        run(&mut game, idle(), 4); // f2-5
        game.update(start()); // f6: ポーズ
        assert_eq!(game.phase(), Phase::Paused);
        let piece = active(&game);
        let board = game.board().clone();
        let chaos = Buttons {
            left: true,
            down: true,
            up: true,
            rotate_cw: true,
            hold: true,
            ..Buttons::default()
        };
        run(&mut game, chaos, 50); // f7-56: 状態を一切進めない
        assert_eq!(game.phase(), Phase::Paused);
        assert_eq!(active(&game), piece);
        assert_eq!(game.score(), 0);
        assert_eq!(game.hold_piece(), None);
        assert_eq!(game.board(), &board);
        game.update(Buttons {
            start: true,
            up: true,
            ..Buttons::default()
        }); // f57: 再開 (up は押しっぱなし)
        assert_eq!(game.phase(), Phase::Playing);
        game.update(up()); // f58: up 保持継続はエッジでない → ハードドロップしない
        assert_eq!(active(&game), piece);
        assert_eq!(game.score(), 0);
    }

    #[test]
    fn pause_preserves_lock_timer_progress() {
        let mut game = game();
        game.board.set(4, 16, Some(Tetromino::J));
        run(&mut game, down(), 6); // f1-6: 接地 (接地 1 フレーム目、score 2)
        run(&mut game, idle(), 9); // f7-15: 接地 10 フレーム目まで
        game.update(start()); // f16: ポーズ
        let chaos = Buttons {
            left: true,
            down: true,
            up: true,
            rotate_cw: true,
            ..Buttons::default()
        };
        run(&mut game, chaos, 100); // f17-116: ロックタイマーは進まない
        game.update(start()); // f117: 再開 (このフレームは何も進まない)
        assert_eq!(game.phase(), Phase::Playing);
        run(&mut game, idle(), 19); // f118-136: 接地 29 フレーム目まで
        assert_eq!(active(&game).kind, Tetromino::T, "タイマーは続きから");
        assert_eq!(game.score(), 2, "ポーズ中の下キーは加点しない");
        game.update(idle()); // f137: 接地 30 フレーム目でロック
        assert_eq!(active(&game).kind, Tetromino::Z);
        assert_eq!(game.board().get(3, 17), Some(Tetromino::T));
    }

    // ---- フレームイベント (HUD・効果音・演出用) ----

    #[test]
    fn quiet_frame_has_no_events() {
        let mut game = game();
        assert_eq!(*game.events(), FrameEvents::default(), "update 前は空");
        game.update(idle()); // f0: 何も起きないフレーム
        assert_eq!(*game.events(), FrameEvents::default());
        assert_eq!(game.combo(), -1);
        assert!(!game.b2b());
    }

    #[test]
    fn hard_drop_without_clear_reports_lock_event() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: 19 セルのハードドロップ、消去なし
        let events = game.events();
        assert!(events.hard_dropped);
        assert_eq!(
            events.locked,
            Some(LockEvent {
                lines_cleared: 0,
                tspin: TSpin::None,
                b2b_applied: false,
                combo: -1,
                perfect_clear: false,
                points_awarded: 0,
                level_up: false,
            }),
            "ドロップ点 (38) は points_awarded に含まない (§9.5)"
        );
        assert!(!events.hold_performed);
        assert!(!events.topped_out);
        assert_eq!(game.score(), 38);
        game.update(idle()); // f2: イベントは 1 フレームでクリアされる
        assert_eq!(*game.events(), FrameEvents::default());
    }

    #[test]
    fn line_clear_lock_event_reports_points_and_combo() {
        let mut game = game();
        for x in [0, 1, 2, 6, 7, 8, 9] {
            game.board.set(x, 0, Some(Tetromino::L));
        }
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: T が x=3..5 を埋めて Single
        let locked = game.events().locked.expect("ロック発生");
        assert_eq!(locked.lines_cleared, 1);
        assert_eq!(locked.tspin, TSpin::None);
        assert_eq!(locked.points_awarded, 100, "Single 100 × level 1 (§9.2)");
        assert_eq!(locked.combo, 0, "最初の消去は combo 0 (§9.4)");
        assert!(!locked.b2b_applied);
        assert!(!locked.perfect_clear);
        assert!(!locked.level_up);
        assert_eq!(game.combo(), 0);
        assert!(!game.b2b(), "Single では B2B チェーンは始まらない (§9.3)");
    }

    #[test]
    fn tspin_double_lock_event_reports_tspin_b2b_and_combo() {
        let mut game = game();
        build_tsd_field(&mut game);
        // B2B チェーンとコンボを事前に開始しておく (Tetris 相当を直接注入)。
        game.scoring.on_lock(4, TSpin::None, false);
        assert!(game.b2b());
        // tspin_double_flow_awards_full_tspin_score と同じ TSD 操作。
        run(&mut game, down(), 48);
        game.update(Buttons {
            down: true,
            rotate_cw: true,
            ..Buttons::default()
        });
        run(&mut game, down(), 5);
        game.update(cw());
        game.update(idle());
        game.update(up()); // 据え置きハードドロップ → TSD ロック
        let locked = game.events().locked.expect("ロック発生");
        assert_eq!(locked.lines_cleared, 2);
        assert_eq!(locked.tspin, TSpin::Full);
        assert!(locked.b2b_applied, "B2B 継続中の TSD (§9.3)");
        assert_eq!(locked.combo, 1, "注入した Tetris が combo 0");
        assert!(!locked.perfect_clear);
        // floor(1200 × 1.5) × level 1 + コンボ 50×1×1 = 1850 (§9.7)。
        assert_eq!(locked.points_awarded, 1850);
        assert!(game.events().hard_dropped);
        assert!(game.b2b());
        assert_eq!(game.combo(), 1);
    }

    #[test]
    fn hold_sets_hold_performed_event() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1
        let events = game.events();
        assert!(events.hold_performed);
        assert_eq!(events.locked, None);
        assert!(!events.hard_dropped);
        game.update(idle()); // f2: クリアされる
        assert!(!game.events().hold_performed);
    }

    #[test]
    fn level_up_is_reported_on_lock_event() {
        let mut game = game();
        // 9 ライン分を注入して、次の Single でレベルアップする状態にする。
        for _ in 0..3 {
            game.scoring.on_lock(3, TSpin::None, false);
        }
        game.scoring.on_lock(0, TSpin::None, false); // コンボを切る
        for x in [0, 1, 2, 6, 7, 8, 9] {
            game.board.set(x, 0, Some(Tetromino::L));
        }
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: Single で 10 ライン到達
        let locked = game.events().locked.expect("ロック発生");
        assert!(locked.level_up);
        assert_eq!(
            locked.points_awarded, 100,
            "レベル倍率は消去時点の値 (§9.2)"
        );
        assert_eq!(game.level(), 2);
    }

    #[test]
    fn shift_rotate_and_soft_drop_events() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(right()); // f1: 移動成功
        assert!(game.events().shifted);
        assert!(!game.events().rotated);
        game.update(cw()); // f2: 回転成功
        assert!(game.events().rotated);
        assert!(!game.events().shifted);
        run(&mut game, down(), 2); // f3-4: アキュムレータ充填中
        assert_eq!(game.events().soft_drop_rows, 0);
        game.update(down()); // f5: 1 セル落下
        assert_eq!(game.events().soft_drop_rows, 1);
        assert_eq!(game.events().locked, None);
    }

    #[test]
    fn failed_shift_does_not_report_event() {
        let mut game = game();
        run(&mut game, left(), 13); // f1-13: 左壁 (x=0) 到達
        assert_eq!(active(&game).x, 0);
        run(&mut game, left(), 2); // f14-15: ARR 周期でも壁で移動失敗
        assert!(!game.events().shifted);
    }

    #[test]
    fn paused_frames_report_no_events() {
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(start()); // f1: ポーズ
        assert_eq!(game.phase(), Phase::Paused);
        assert_eq!(*game.events(), FrameEvents::default());
        let chaos = Buttons {
            left: true,
            down: true,
            up: true,
            rotate_cw: true,
            hold: true,
            ..Buttons::default()
        };
        run(&mut game, chaos, 10); // f2-11: ポーズ中はイベントなし
        assert_eq!(*game.events(), FrameEvents::default());
    }

    #[test]
    fn block_out_sets_topped_out_with_lock_event() {
        let mut game = game();
        game.board.set(3, 21, Some(Tetromino::J));
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // f1: T ロック → 次の Z がブロックアウト (§13)
        let events = game.events();
        assert!(events.topped_out);
        assert!(events.hard_dropped);
        assert!(
            events.locked.is_some(),
            "ロック自体は成立しスコア処理まで走る"
        );
        game.update(idle()); // f2: ゲームオーバー後はイベントなし
        assert_eq!(*game.events(), FrameEvents::default());
    }

    #[test]
    fn lock_out_reports_topped_out_without_lock_event() {
        let mut game = game();
        game.board.set(4, 19, Some(Tetromino::J));
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(hold()); // f1: Z が y=19 で接地
        game.update(up()); // f2: 全セル可視領域外でロック → ロックアウト (§13)
        assert_eq!(game.phase(), Phase::GameOver);
        let events = game.events();
        assert!(events.topped_out);
        assert!(events.hard_dropped);
        assert_eq!(
            events.locked, None,
            "ロックアウトでは加点・消去処理は走らない"
        );
    }

    // ---- 描画用アクセサ (§14.2-8) ----

    #[test]
    fn ghost_piece_tracks_hard_drop_position() {
        let mut game = game();
        assert_eq!(
            game.ghost_piece(),
            Some(ActivePiece {
                kind: Tetromino::T,
                rot: Rotation::Spawn,
                x: 3,
                y: -1,
            })
        );
        game.board.set(4, 10, Some(Tetromino::I));
        assert_eq!(game.ghost_piece().map(|g| g.y), Some(10));
    }

    #[test]
    fn next_previews_advance_after_lock() {
        use Tetromino::{I, J, L, O, S};
        let mut game = game();
        game.update(idle()); // f0: 初回エッジ飲み込み
        game.update(up()); // T ロック → Z が操作中に
        let next: [Tetromino; 5] = core::array::from_fn(|i| game.next(i));
        assert_eq!(next, [O, J, S, L, I]);
    }
}
