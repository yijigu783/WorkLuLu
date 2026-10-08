//! 矩形和 DPI 换算：贴边隐藏（`edge.rs`）与边缘吸附（`snap.rs`）共用的地基。
//!
//! 单独拎出来是因为两边要反复比较「窗口」和「显示器」这两个矩形，各写一份
//! 迟早会出现两个字段顺序不一致的 `Rect`，比出来的结果谁也看不懂。
//!
//! **统一用物理像素。** Tauri 给的 `outer_position` / `outer_size` / 显示器尺寸
//! 本来就是物理像素；逻辑像素只出现在「用户能感知」的地方（比如容差 12 逻辑像素），
//! 进出屏幕坐标前一律过 `px()` —— 少了这一步，125% / 150% 缩放的屏幕上
//! 所有判定都会整体偏掉一截，而且偏得不多，最难查。

/// 一块矩形区域，物理像素。窗口和显示器都用它描述，方便直接比较。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// 右边界（不含）。比 `x + w` 写起来短，两处都要用同一个，别一处含一处不含。
    pub fn right(self) -> i32 {
        self.x + self.w
    }

    /// 下边界（不含）。
    pub fn bottom(self) -> i32 {
        self.y + self.h
    }

    /// 点在不在这块矩形里，可以外扩 `pad`（判断鼠标「还在窗口附近」用得上：
    /// 贴着窗口边缘蹭过去的那一下，不该算已经走开）。
    pub fn holds(self, x: i32, y: i32, pad: i32) -> bool {
        x >= self.x - pad && x <= self.right() + pad && y >= self.y - pad && y <= self.bottom() + pad
    }
}

/// 逻辑像素 → 物理像素。屏幕缩放 125% / 150% 时不做换算，
/// 判定阈值在 4K 屏上会窄得几乎碰不到。
pub fn px(v: f64, scale: f64) -> i32 {
    ((v * scale).round() as i32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_and_bottom_are_exclusive_edges() {
        let r = Rect { x: 10, y: 20, w: 100, h: 50 };
        assert_eq!(r.right(), 110);
        assert_eq!(r.bottom(), 70);
    }

    /// 缩放 125% 时容差跟着放大，否则高 DPI 屏上贴边 / 吸附会变得极难触发
    #[test]
    fn tolerance_scales_with_dpi() {
        assert_eq!(px(8.0, 1.0), 8);
        assert_eq!(px(8.0, 1.25), 10);
        assert_eq!(px(8.0, 1.5), 12);
        // 再小的值也不能变成 0，否则容差等于没有
        assert_eq!(px(0.2, 1.0), 1);
    }

    #[test]
    fn holds_accepts_inside_and_a_little_outside() {
        let r = Rect { x: 100, y: 100, w: 200, h: 100 };
        assert!(r.holds(150, 150, 0));
        assert!(!r.holds(99, 150, 0), "边界外一像素不算在里面");
        assert!(r.holds(99, 150, 8), "外扩之后要算");
        assert!(!r.holds(500, 150, 8));
    }
}
