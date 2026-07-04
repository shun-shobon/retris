//! 毎フレームの生ボタン入力 (仕様書 §12.1)。

/// 毎フレームの「押されている」ボタン状態 (仕様書 §12.1)。
///
/// GBA 層は毎フレームの生のキー状態をそのまま渡す。エッジ検出 (押下瞬間の判定) は
/// [`crate::game::Game::update`] が前フレームとの差分で行う。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buttons {
    /// 十字キー左: 左移動 (DAS/ARR §12.2)。
    pub left: bool,
    /// 十字キー右: 右移動 (DAS/ARR §12.2)。
    pub right: bool,
    /// 十字キー下: ソフトドロップ (保持中 20 倍重力 §7.3)。
    pub down: bool,
    /// 十字キー上: ハードドロップ (押下エッジのみ §7.4, §12.3)。
    pub up: bool,
    /// A: 右回転 CW (押下エッジのみ §12.3)。
    pub rotate_cw: bool,
    /// B: 左回転 CCW (押下エッジのみ §12.3)。
    pub rotate_ccw: bool,
    /// L または R: ホールド (押下エッジのみ §6.2)。
    pub hold: bool,
    /// START: ポーズ / 解除 (押下エッジのみ §14.3)。
    pub start: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_released() {
        let buttons = Buttons::default();
        assert!(
            !(buttons.left
                || buttons.right
                || buttons.down
                || buttons.up
                || buttons.rotate_cw
                || buttons.rotate_ccw
                || buttons.hold
                || buttons.start)
        );
    }
}
