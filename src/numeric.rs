//! Lossless-by-construction numeric conversions, so that no `as` cast is
//! needed where a count or a channel meets a float.

/// A `usize` as an `f32`, rounded to nearest like an `as` cast; saturates
/// above `u32::MAX`, far past any count this program sees.
pub fn usize_f32(n: usize) -> f32 {
    u32_f32(u32::try_from(n).unwrap_or(u32::MAX))
}

/// A `u32` as an `f32`, rounded to nearest like an `as` cast.
pub fn u32_f32(n: u32) -> f32 {
    let hi = u16::try_from(n >> 16).unwrap_or(u16::MAX);
    let lo = u16::try_from(n & 0xffff).unwrap_or(u16::MAX);
    f32::from(hi) * 65_536.0 + f32::from(lo)
}

/// An `i32` as an `f32`, rounded to nearest like an `as` cast.
pub fn i32_f32(n: i32) -> f32 {
    let hi = i16::try_from(n >> 16).unwrap_or(i16::MAX);
    let lo = u16::try_from(n & 0xffff).unwrap_or(u16::MAX);
    f32::from(hi) * 65_536.0 + f32::from(lo)
}

/// `v` rounded half away from zero to the nearest `u8`, clamped to `0..=255`.
pub fn round_u8(v: f32) -> u8 {
    // The nearest channel value; on a tie the higher one wins (as `round`).
    (0..=u8::MAX)
        .rev()
        .min_by(|a, b| {
            (f32::from(*a) - v)
                .abs()
                .total_cmp(&(f32::from(*b) - v).abs())
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_convert_exactly() {
        assert!((usize_f32(1_000_003) - 1_000_003.0).abs() < f32::EPSILON);
        assert!((i32_f32(-70_000) + 70_000.0).abs() < f32::EPSILON);
        assert!((u32_f32(0) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn channels_round_half_up_and_clamp() {
        assert_eq!(round_u8(127.5), 128);
        assert_eq!(round_u8(127.4), 127);
        assert_eq!(round_u8(300.0), 255);
        assert_eq!(round_u8(-3.0), 0);
    }
}
