//! スコアリング・B2B・コンボ・レベル進行 (仕様書 §9, §11)。

use crate::tspin::TSpin;

/// レベルアップに必要な消去ライン数 (仕様書 §11, §15)。
const LINES_PER_LEVEL: u32 = 10;

/// 重力・スコア倍率のレベル上限 (仕様書 §11, §15)。
const LEVEL_MAX: u32 = 20;

/// スコア・B2B・コンボ・レベル進行の状態 (仕様書 §9, §11)。
///
/// T-Spin 判定に必要な「最後の操作が回転か」「使用キックテスト番号」は
/// ゲームステート側 (後工程) が保持し、[`crate::tspin::detect_tspin`] の結果を
/// [`Self::on_lock`] に渡す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoring {
    score: u32,
    start_level: u32,
    total_lines: u32,
    /// B2B チェーン継続中か = 直前の「難しい消去」から通常消去を挟んでいないか (§9.3)。
    b2b: bool,
    /// コンボカウンタ (§9.4)。−1 で初期化。
    combo: i16,
}

impl Scoring {
    /// 開始レベル `start_level` (1〜15 を想定、タイトル画面で選択 §11) で初期化する。
    #[must_use]
    pub const fn new(start_level: u32) -> Self {
        Self {
            score: 0,
            start_level,
            total_lines: 0,
            b2b: false,
            combo: -1,
        }
    }

    /// 現在のスコア。
    #[must_use]
    pub const fn score(&self) -> u32 {
        self.score
    }

    /// 累計消去ライン数。
    #[must_use]
    pub const fn total_lines(&self) -> u32 {
        self.total_lines
    }

    /// B2B チェーン継続中か (§9.3)。
    #[must_use]
    pub const fn b2b(&self) -> bool {
        self.b2b
    }

    /// コンボカウンタ (§9.4)。−1 = コンボなし。
    #[must_use]
    pub const fn combo(&self) -> i16 {
        self.combo
    }

    /// 表示用レベル (§11 Fixed Goal): `start_level + total_lines / 10`。上限なし。
    #[must_use]
    pub const fn level(&self) -> u32 {
        self.start_level + self.total_lines / LINES_PER_LEVEL
    }

    /// 重力・スコア倍率用レベル (§11): 20 で頭打ち。
    #[must_use]
    pub const fn effective_level(&self) -> u32 {
        let level = self.level();
        if level > LEVEL_MAX { LEVEL_MAX } else { level }
    }

    /// ロック 1 回分のスコア加算 (§9.7)。加点した額を返す。
    ///
    /// `floor(基本点 × B2B倍率) × level + コンボ点 + PCボーナス × level` を加点し、
    /// B2B・コンボ・累計ライン数を更新する。スコア倍率の `level` は消去発生時点
    /// (`total_lines` 加算前) の [`Self::effective_level`] (§9.2)。
    /// ドロップ点は [`Self::add_soft_drop_cells`] / [`Self::add_hard_drop_cells`] で別途加算する。
    pub fn on_lock(&mut self, lines_cleared: u8, tspin: TSpin, perfect_clear: bool) -> u32 {
        // スコア倍率のレベルは消去発生時点 = total_lines 加算前の値 (§9.2)。
        let level = self.effective_level();

        // 「難しい消去」= Tetris、および消去を伴う T-Spin / Mini (§9.3)。
        let is_hard_clear =
            lines_cleared == 4 || (lines_cleared > 0 && !matches!(tspin, TSpin::None));
        let b2b_applied = is_hard_clear && self.b2b;

        // 基本点 × B2B 倍率 (×1.5、floor) × level (§9.2, §9.3)。
        let base = base_points(lines_cleared, tspin);
        let base = if b2b_applied { base * 3 / 2 } else { base };
        let mut points = base * level;

        // コンボ (§9.4): 消去ありロックで +1、消去なしロックで −1 にリセット。
        if lines_cleared > 0 {
            self.combo += 1;
            if self.combo >= 1 {
                points += 50 * self.combo as u32 * level;
            }
        } else {
            self.combo = -1;
        }

        // Perfect Clear ボーナス (§9.6): 通常の消去点に加算。
        if perfect_clear {
            points += perfect_clear_bonus(lines_cleared, b2b_applied) * level;
        }

        // B2B チェーン更新 (§9.3): 難しい消去で継続、通常消去で切れる。
        // 消去なしロック (消去なし T-Spin 含む) では変化しない。
        if is_hard_clear {
            self.b2b = true;
        } else if lines_cleared > 0 {
            self.b2b = false;
        }

        self.total_lines += u32::from(lines_cleared);
        self.score += points;
        points
    }

    /// ソフトドロップ点: 1 点 × 落下セル数。レベル倍率なし (§9.5)。
    pub fn add_soft_drop_cells(&mut self, cells: u32) {
        self.score += cells;
    }

    /// ハードドロップ点: 2 点 × 落下セル数、最大 40 点 (= 20 セル)。レベル倍率なし (§9.5)。
    pub fn add_hard_drop_cells(&mut self, cells: u32) {
        self.score += 2 * cells.min(20);
    }
}

/// 基本スコア表 (仕様書 §9.2、level を掛ける前の値)。
///
/// 表にない組み合わせ (T-Spin Mini Triple 等) は発生しないため 0。
const fn base_points(lines_cleared: u8, tspin: TSpin) -> u32 {
    match (tspin, lines_cleared) {
        (TSpin::None, 1) => 100,  // Single
        (TSpin::None, 2) => 300,  // Double
        (TSpin::None, 3) => 500,  // Triple
        (TSpin::None, 4) => 800,  // Tetris
        (TSpin::Mini, 0) => 100,  // T-Spin Mini (消去なし)
        (TSpin::Mini, 1) => 200,  // T-Spin Mini Single
        (TSpin::Mini, 2) => 400,  // T-Spin Mini Double
        (TSpin::Full, 0) => 400,  // T-Spin (消去なし)
        (TSpin::Full, 1) => 800,  // T-Spin Single
        (TSpin::Full, 2) => 1200, // T-Spin Double
        (TSpin::Full, 3) => 1600, // T-Spin Triple
        _ => 0,
    }
}

/// Perfect Clear ボーナス表 (仕様書 §9.6、level を掛ける前の値)。
///
/// `b2b_applied` = その Tetris に B2B 倍率が適用されたとき、2000 の代わりに 3200。
const fn perfect_clear_bonus(lines_cleared: u8, b2b_applied: bool) -> u32 {
    match lines_cleared {
        1 => 800,
        2 => 1200,
        3 => 1800,
        4 => {
            if b2b_applied {
                3200
            } else {
                2000
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 期待値はすべて仕様書 §9, §11 からの手書き転記。

    #[test]
    fn new_scoring_starts_clean() {
        let s = Scoring::new(3);
        assert_eq!(s.score(), 0);
        assert_eq!(s.total_lines(), 0);
        assert_eq!(s.level(), 3);
        assert_eq!(s.combo(), -1);
        assert!(!s.b2b());
    }

    #[test]
    fn no_clear_lock_scores_zero() {
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(0, TSpin::None, false), 0);
        assert_eq!(s.score(), 0);
        assert_eq!(s.total_lines(), 0);
    }

    #[test]
    fn base_scores_at_level_1() {
        // §9.2 の基本スコア表 (level 1)。
        let cases: [(u8, TSpin, u32); 11] = [
            (1, TSpin::None, 100),  // Single
            (2, TSpin::None, 300),  // Double
            (3, TSpin::None, 500),  // Triple
            (4, TSpin::None, 800),  // Tetris
            (0, TSpin::Mini, 100),  // T-Spin Mini (消去なし)
            (1, TSpin::Mini, 200),  // T-Spin Mini Single
            (2, TSpin::Mini, 400),  // T-Spin Mini Double
            (0, TSpin::Full, 400),  // T-Spin (消去なし)
            (1, TSpin::Full, 800),  // T-Spin Single
            (2, TSpin::Full, 1200), // T-Spin Double
            (3, TSpin::Full, 1600), // T-Spin Triple
        ];
        for (lines, tspin, expected) in cases {
            let mut s = Scoring::new(1);
            assert_eq!(
                s.on_lock(lines, tspin, false),
                expected,
                "{lines} lines, {tspin:?}"
            );
            assert_eq!(s.score(), expected);
        }
    }

    #[test]
    fn base_scores_multiply_by_start_level_5() {
        // §9.2: 基本点 × level。start_level=5 なら 5 倍。
        let cases: [(u8, TSpin, u32); 4] = [
            (1, TSpin::None, 500),  // Single 100×5
            (4, TSpin::None, 4000), // Tetris 800×5
            (2, TSpin::Full, 6000), // TSD 1200×5
            (0, TSpin::Full, 2000), // 消去なし T-Spin 400×5
        ];
        for (lines, tspin, expected) in cases {
            let mut s = Scoring::new(5);
            assert_eq!(
                s.on_lock(lines, tspin, false),
                expected,
                "{lines} lines, {tspin:?}"
            );
        }
    }

    #[test]
    fn back_to_back_tetris_multiplies_base_by_1_5() {
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(4, TSpin::None, false), 800); // 1 回目は通常
        s.on_lock(0, TSpin::None, false); // 消去なしロック (コンボを切る)
        assert_eq!(s.on_lock(4, TSpin::None, false), 1200); // §9.3: 800 → 1200
    }

    #[test]
    fn back_to_back_breaks_on_normal_clear() {
        let mut s = Scoring::new(1);
        s.on_lock(4, TSpin::None, false);
        s.on_lock(0, TSpin::None, false);
        assert_eq!(s.on_lock(1, TSpin::None, false), 100); // 通常消去でチェーンが切れる
        s.on_lock(0, TSpin::None, false);
        assert_eq!(s.on_lock(4, TSpin::None, false), 800); // B2B にならない
    }

    #[test]
    fn back_to_back_tsd_then_tst() {
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(2, TSpin::Full, false), 1200); // TSD
        s.on_lock(0, TSpin::None, false);
        assert_eq!(s.on_lock(3, TSpin::Full, false), 2400); // §9.3: TST 1600 → 2400
    }

    #[test]
    fn back_to_back_mini_single_gets_300() {
        let mut s = Scoring::new(1);
        s.on_lock(4, TSpin::None, false);
        s.on_lock(0, TSpin::None, false);
        assert_eq!(s.on_lock(1, TSpin::Mini, false), 300); // §9.3: Mini TSS 200 → 300
    }

    #[test]
    fn no_clear_tspin_scores_but_keeps_b2b_chain() {
        let mut s = Scoring::new(1);
        s.on_lock(4, TSpin::None, false); // チェーン開始
        // 消去なし T-Spin: 400×level 加点、B2B 倍率なし、チェーンは切れない (§9.3)。
        assert_eq!(s.on_lock(0, TSpin::Full, false), 400);
        assert!(s.b2b());
        assert_eq!(s.on_lock(4, TSpin::None, false), 1200); // B2B 維持
    }

    #[test]
    fn no_clear_tspin_does_not_start_b2b_chain() {
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(0, TSpin::Full, false), 400);
        assert!(!s.b2b());
        assert_eq!(s.on_lock(4, TSpin::None, false), 800); // チェーンは繋がっていない
    }

    #[test]
    fn no_clear_lock_keeps_b2b_and_resets_combo() {
        let mut s = Scoring::new(1);
        s.on_lock(4, TSpin::None, false);
        s.on_lock(1, TSpin::None, false); // combo 0 → 1 に備えて 1 消去
        s.on_lock(0, TSpin::None, false); // 消去なしロック
        assert_eq!(s.combo(), -1, "コンボは −1 にリセット (§9.4)");
    }

    #[test]
    fn combo_adds_50_per_count_times_level() {
        let mut s = Scoring::new(1);
        // §9.4: combo は −1 初期化、消去ありで +1。combo >= 1 で 50×combo×level。
        assert_eq!(s.on_lock(1, TSpin::None, false), 100); // combo 0: 加点なし
        assert_eq!(s.on_lock(1, TSpin::None, false), 150); // combo 1: 100 + 50×1×1
        assert_eq!(s.on_lock(1, TSpin::None, false), 200); // combo 2: 100 + 50×2×1
        assert_eq!(s.combo(), 2);
    }

    #[test]
    fn combo_bonus_multiplies_by_level() {
        let mut s = Scoring::new(5);
        assert_eq!(s.on_lock(1, TSpin::None, false), 500); // combo 0
        assert_eq!(s.on_lock(1, TSpin::None, false), 750); // combo 1: 500 + 50×1×5
    }

    #[test]
    fn perfect_clear_bonuses() {
        // §9.6: 通常の消去点に加算 (× level)。
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(1, TSpin::None, true), 900); // Single PC: 100 + 800
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(2, TSpin::None, true), 1500); // Double PC: 300 + 1200
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(3, TSpin::None, true), 2300); // Triple PC: 500 + 1800 (§9.6 の例)
        let mut s = Scoring::new(1);
        assert_eq!(s.on_lock(4, TSpin::None, true), 2800); // Tetris PC: 800 + 2000
    }

    #[test]
    fn perfect_clear_bonus_multiplies_by_level() {
        let mut s = Scoring::new(5);
        assert_eq!(s.on_lock(1, TSpin::None, true), 4500); // (100 + 800) × 5
    }

    #[test]
    fn b2b_tetris_perfect_clear_uses_3200() {
        let mut s = Scoring::new(1);
        s.on_lock(4, TSpin::None, false); // 800、チェーン開始、combo 0
        // B2B Tetris PC (§9.6): 2000 の代わりに 3200。
        // 合成 (§9.7): 800×1.5 + 50×1 (combo 1) + 3200 = 4450。
        assert_eq!(s.on_lock(4, TSpin::None, true), 4450);
    }

    #[test]
    fn soft_drop_scores_1_per_cell_without_level_multiplier() {
        let mut s = Scoring::new(5); // level 5 でも倍率なし (§9.5)
        s.add_soft_drop_cells(3);
        assert_eq!(s.score(), 3);
        s.add_soft_drop_cells(1);
        assert_eq!(s.score(), 4);
    }

    #[test]
    fn hard_drop_scores_2_per_cell_capped_at_40() {
        let mut s = Scoring::new(5); // level 5 でも倍率なし (§9.5)
        s.add_hard_drop_cells(10);
        assert_eq!(s.score(), 20);
        s.add_hard_drop_cells(20); // ちょうど 20 セル = 上限の 40 点
        assert_eq!(s.score(), 60);
        s.add_hard_drop_cells(25); // 20 セル超でも 40 点で頭打ち (§9.5)
        assert_eq!(s.score(), 100);
    }

    #[test]
    fn level_advances_every_10_lines_fixed_goal() {
        // §11: level = start_level + total_lines / 10。
        let mut s = Scoring::new(1);
        assert_eq!(s.level(), 1);
        for _ in 0..3 {
            s.on_lock(3, TSpin::None, false);
        }
        assert_eq!(s.total_lines(), 9);
        assert_eq!(s.level(), 1);
        s.on_lock(1, TSpin::None, false);
        assert_eq!(s.total_lines(), 10);
        assert_eq!(s.level(), 2);
    }

    #[test]
    fn level_reaches_20_at_195_lines_from_start_1() {
        let mut s = Scoring::new(1);
        for _ in 0..48 {
            s.on_lock(4, TSpin::None, false); // 192 ライン
        }
        s.on_lock(3, TSpin::None, false); // 195 ライン
        assert_eq!(s.total_lines(), 195);
        assert_eq!(s.level(), 20);
        assert_eq!(s.effective_level(), 20);
    }

    #[test]
    fn effective_level_caps_at_20_while_display_level_grows() {
        let mut s = Scoring::new(15);
        for _ in 0..25 {
            s.on_lock(4, TSpin::None, false); // 100 ライン → level 25
        }
        assert_eq!(s.level(), 25); // 表示レベルは増え続ける (§11)
        assert_eq!(s.effective_level(), 20); // 倍率は 20 で頭打ち (§11)
        // スコア倍率も ×20 で頭打ち。
        s.on_lock(0, TSpin::None, false); // コンボを切る
        assert_eq!(s.on_lock(1, TSpin::None, false), 2000); // Single 100 × 20
    }

    #[test]
    fn score_uses_level_at_clear_time_before_line_count_update() {
        // §9.2: level は消去が発生した時点 (total_lines 加算前) の値。
        let mut s = Scoring::new(1);
        for _ in 0..3 {
            s.on_lock(3, TSpin::None, false); // 3 ラインずつ積む
            s.on_lock(0, TSpin::None, false); // コンボを切る
        }
        assert_eq!(s.total_lines(), 9);
        // 10 ライン目に到達する Single は level 1 のまま 100 点。
        assert_eq!(s.on_lock(1, TSpin::None, false), 100);
        assert_eq!(s.level(), 2); // 消去後にレベルアップ
        // 以後の消去は level 2 で加点される。
        s.on_lock(0, TSpin::None, false);
        assert_eq!(s.on_lock(1, TSpin::None, false), 200);
    }
}
