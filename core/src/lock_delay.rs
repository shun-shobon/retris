//! ロックディレイ (Extended Placement Lockdown / move reset 方式) (仕様書 §8)。
//!
//! ロック処理 (§8.3) 自体は含まない。呼び出し側 (ゲームループ) の使い方:
//!
//! - スポーン時に [`LockDelay::new`] でミノごとに作り直す。
//! - 左右移動・回転が**成功**したとき [`LockDelay::notify_move`] を呼ぶ (§8.2-2)。
//!   移動・回転の不発や O ミノの回転入力 (§4.4) では**呼ばない** (§8.2-6。呼び出し側責務)。
//!   `grounded` には移動後の位置での接地状態を渡す。
//! - 1 行落下が成功したとき [`LockDelay::notify_fall`] に新しい `by` を渡す (§8.2-5)。
//! - 毎フレーム末尾 (§14.2 手順 7) に [`LockDelay::frame_update`] を呼び、
//!   `true` が返ったらロック処理へ。ハードドロップ (§8.2-7) は本モジュールを経由せず即ロック。

/// ロックディレイのフレーム数 (仕様書 §8, §15)。
pub const LOCK_DELAY_FRAMES: u8 = 30;

/// ロックディレイのリセット回数上限 (仕様書 §8, §15)。
pub const LOCK_RESET_MAX: u8 = 15;

/// ロックディレイの状態 (仕様書 §8.1)。ミノごとにスポーン時へ作り直す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockDelay {
    /// 残りフレーム数 (接地中のみ減算)。
    timer: u8,
    /// 使用済みリセット回数。
    reset_count: u8,
    /// このミノが到達した最低 (最小) の `by`。
    lowest_y: i8,
    /// 前フレーム (前回の `frame_update`) 時点の接地状態。規則 3 の「新たに接地」検出用。
    was_grounded: bool,
}

impl LockDelay {
    /// スポーン時の初期状態で生成する (`timer = 30`, `reset_count = 0`, `lowest_y = spawn_y`)。
    #[must_use]
    pub const fn new(spawn_y: i8) -> Self {
        Self {
            timer: LOCK_DELAY_FRAMES,
            reset_count: 0,
            lowest_y: spawn_y,
            was_grounded: false,
        }
    }

    /// 毎フレーム末尾 (§14.2 手順 7) に呼ぶ。ロックすべきなら `true`。
    ///
    /// - 接地中はタイマーを 1 減らし、0 になった瞬間にロック (§8.2-1)。
    /// - 空中ではタイマーを減らさず、値を保持する (§8.2-4)。
    /// - リセットを 15 回使い切った状態で**新たに接地した** (空中→接地に遷移した)
    ///   場合は残タイマーに関係なく即ロック (§8.2-3)。接地が継続している間は
    ///   残タイマーで進行する。
    pub fn frame_update(&mut self, grounded: bool) -> bool {
        let touched_down = grounded && !self.was_grounded;
        self.was_grounded = grounded;
        if !grounded {
            return false;
        }
        if touched_down && self.reset_count >= LOCK_RESET_MAX {
            return true;
        }
        self.timer = self.timer.saturating_sub(1);
        self.timer == 0
    }

    /// 左右移動・回転が成功したとき呼ぶ (§8.2-2)。
    ///
    /// 接地中かつ `reset_count < 15` ならリセットを 1 消費してタイマーを 30 に戻す。
    /// 空中、またはリセットを使い切った後は何もしない。
    pub fn notify_move(&mut self, grounded: bool) {
        if grounded && self.reset_count < LOCK_RESET_MAX {
            self.reset_count += 1;
            self.timer = LOCK_DELAY_FRAMES;
        }
    }

    /// 1 行落下が成功したとき呼ぶ (§8.2-5)。
    ///
    /// `new_y` が新しい最低到達行 (`new_y < lowest_y`) なら `lowest_y` を更新し、
    /// `reset_count = 0`・`timer = 30` に回復する。それ以外 (キック上昇後の落下で
    /// 既到達行を再通過する場合など) は何もしない。
    pub fn notify_fall(&mut self, new_y: i8) {
        if new_y < self.lowest_y {
            self.lowest_y = new_y;
            self.reset_count = 0;
            self.timer = LOCK_DELAY_FRAMES;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストで使うスポーン行 (T ミノ等の spawn_y に相当)。
    const SPAWN_Y: i8 = 19;

    /// `frames` 回の `frame_update` が全て `false` (ロックしない) ことを検証する。
    #[track_caller]
    fn assert_survives(ld: &mut LockDelay, grounded: bool, frames: u32) {
        for frame in 1..=frames {
            assert!(
                !ld.frame_update(grounded),
                "フレーム {frame}/{frames} (grounded={grounded}) でロックしてはならない"
            );
        }
    }

    // ---- 初期状態 (§8.1) ----

    #[test]
    fn new_starts_with_full_timer_zero_resets_and_spawn_lowest_y() {
        let ld = LockDelay::new(SPAWN_Y);
        assert_eq!(ld.timer, LOCK_DELAY_FRAMES);
        assert_eq!(ld.reset_count, 0);
        assert_eq!(ld.lowest_y, SPAWN_Y);
        assert!(!ld.was_grounded, "スポーン直後は空中扱い");
    }

    // ---- 規則 1: 接地中のタイマー減算 ----

    #[test]
    fn locks_on_30th_consecutive_grounded_frame() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert_survives(&mut ld, true, 29);
        assert!(ld.frame_update(true), "30 フレーム目でロックすべき");
    }

    // ---- 規則 4: 空中ではタイマー保持 ----

    #[test]
    fn airborne_frames_do_not_consume_timer() {
        let mut ld = LockDelay::new(SPAWN_Y);
        // 接地 10F → 空中 20F → 接地 20F: 接地合計 30F 目でロック。
        assert_survives(&mut ld, true, 10);
        assert_survives(&mut ld, false, 20);
        assert_survives(&mut ld, true, 19);
        assert!(
            ld.frame_update(true),
            "接地合計 30 フレーム目でロックすべき"
        );
    }

    // ---- 規則 2: 接地中の移動・回転によるリセット ----

    #[test]
    fn grounded_move_restores_timer_up_to_15_times() {
        let mut ld = LockDelay::new(SPAWN_Y);
        for _reset in 1..=15 {
            // 29 フレーム生存 (timer 30 → 1) してからリセット。回復していなければ
            // 次の周回の 29 フレーム中にロックして assert_survives が落ちる。
            assert_survives(&mut ld, true, 29);
            ld.notify_move(true);
        }
        // 15 回目のリセット後もフル 30 フレーム: 29 フレーム生存 → 30 フレーム目で満了ロック。
        assert_survives(&mut ld, true, 29);
        assert!(
            ld.frame_update(true),
            "15 回目のリセット後 30 フレーム目でロックすべき"
        );
    }

    #[test]
    fn sixteenth_move_does_not_reset_timer() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert!(!ld.frame_update(true)); // 接地 (timer 29)
        for _ in 0..15 {
            ld.notify_move(true); // 15 回のリセットを即時に使い切る (timer 30)
        }
        assert_survives(&mut ld, true, 10); // timer 20
        ld.notify_move(true); // 16 回目: リセットされない
        assert_survives(&mut ld, true, 19); // timer 1
        assert!(
            ld.frame_update(true),
            "16 回目の移動ではタイマーが戻らず満了ロックすべき"
        );
    }

    #[test]
    fn airborne_move_preserves_timer() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert_survives(&mut ld, true, 10); // timer 20
        assert_survives(&mut ld, false, 5); // 空中
        for _ in 0..16 {
            ld.notify_move(false); // 空中の移動: タイマーを変えない
        }
        assert_survives(&mut ld, true, 19); // timer 20 → 1
        assert!(
            ld.frame_update(true),
            "空中移動でタイマーが回復してはならない"
        );
    }

    #[test]
    fn airborne_move_does_not_consume_resets() {
        let mut ld = LockDelay::new(SPAWN_Y);
        for _ in 0..16 {
            ld.notify_move(false); // 空中の移動: reset_count を消費しない
        }
        assert_eq!(
            ld.reset_count, 0,
            "空中移動で reset_count を消費してはならない"
        );
        // 接地後も 15 回フルにリセットできる。
        for _reset in 1..=15 {
            assert_survives(&mut ld, true, 29);
            ld.notify_move(true);
        }
        assert_survives(&mut ld, true, 29);
        assert!(ld.frame_update(true));
    }

    // ---- 規則 5: 下降による回復 ----

    #[test]
    fn fall_to_new_lowest_row_restores_resets_and_timer() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert!(!ld.frame_update(true)); // 接地 (timer 29)
        for _ in 0..15 {
            ld.notify_move(true); // リセットを使い切る
        }
        assert_survives(&mut ld, true, 10); // timer 20
        ld.notify_fall(SPAWN_Y - 1); // 新しい最低到達行 → reset_count=0, timer=30
        // タイマーが 30 に回復し、リセットも再び 15 回使える。
        for _reset in 1..=15 {
            assert_survives(&mut ld, true, 29);
            ld.notify_move(true);
        }
        assert_survives(&mut ld, true, 29);
        assert!(ld.frame_update(true));
    }

    #[test]
    fn fall_not_below_lowest_does_not_restore() {
        let mut ld = LockDelay::new(SPAWN_Y);
        ld.notify_fall(SPAWN_Y - 1);
        ld.notify_fall(SPAWN_Y - 2); // lowest_y = SPAWN_Y - 2
        assert!(!ld.frame_update(true)); // 接地 (timer 29)
        for _ in 0..15 {
            ld.notify_move(true); // リセットを使い切る (timer 30)
        }
        assert_survives(&mut ld, true, 20); // timer 10
        ld.notify_fall(SPAWN_Y - 2); // 同じ行への落下 → 回復しない
        ld.notify_fall(SPAWN_Y - 1); // キック上昇後の落下 (既到達行) → 回復しない
        assert_eq!(ld.lowest_y, SPAWN_Y - 2, "lowest_y は更新されない");
        assert_survives(&mut ld, true, 9); // timer 1
        assert!(
            ld.frame_update(true),
            "回復せず残タイマーで満了ロックすべき"
        );
    }

    // ---- 規則 3 (修正版): リセット使い切り後の挙動 ----

    #[test]
    fn exhausted_resets_then_new_touchdown_locks_immediately() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert!(!ld.frame_update(true)); // 接地 (timer 29)
        for _ in 0..15 {
            ld.notify_move(true); // リセットを使い切る (timer 30)
        }
        assert_survives(&mut ld, false, 3); // キック上昇で空中へ
        assert!(
            ld.frame_update(true),
            "リセット使い切り後の再接地は即ロックすべき"
        );
    }

    #[test]
    fn exhausted_resets_while_grounded_continues_with_remaining_timer() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert!(!ld.frame_update(true)); // 接地 (timer 29)
        for _ in 0..15 {
            ld.notify_move(true); // 15 回目のリセットで timer 30
        }
        // 接地継続中は即ロックせず、残タイマーで進行する (= 15 回目のリセットが有効)。
        assert_survives(&mut ld, true, 29);
        assert!(ld.frame_update(true), "残タイマー満了でロックすべき");
    }

    #[test]
    fn exhausted_resets_then_fall_to_new_lowest_avoids_immediate_lock() {
        let mut ld = LockDelay::new(SPAWN_Y);
        assert!(!ld.frame_update(true)); // 接地
        for _ in 0..15 {
            ld.notify_move(true); // リセットを使い切る
        }
        assert_survives(&mut ld, false, 2); // 空中へ
        ld.notify_fall(SPAWN_Y - 1); // 新しい最低到達行 → 規則 5 で回復
        assert!(
            !ld.frame_update(true),
            "回復後の再接地は即ロックしてはならない"
        );
        assert_survives(&mut ld, true, 28);
        assert!(ld.frame_update(true), "回復後は通常の 30 フレームでロック");
    }
}
