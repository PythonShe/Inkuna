//! Fixed-point layout arithmetic: i32 units of 1/64 layout point.
//!
//! Floats exist only at the boundaries — [`Fx::from_pt`] where viewport
//! and settings values enter, [`Fx::to_f32`] where display lists leave.
//! Every operation in between is exact integer math; overflow saturates
//! (the never-panic backstop). Accumulation order is the caller's
//! responsibility.

/// A length in 1/64 layout point.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
pub struct Fx(pub i32);

impl Fx {
    pub const ZERO: Fx = Fx(0);

    /// Layout points → fixed units, rounding half away from zero.
    /// Non-finite input and out-of-range magnitudes saturate.
    pub fn from_pt(pt: f64) -> Fx {
        if pt.is_nan() {
            return Fx::ZERO;
        }
        // f64::round rounds half away from zero, exactly the contract.
        let units = (pt * 64.0).round();
        if units >= i32::MAX as f64 {
            Fx(i32::MAX)
        } else if units <= i32::MIN as f64 {
            Fx(i32::MIN)
        } else {
            Fx(units as i32)
        }
    }

    /// The ONLY exit to float, at display-list emission.
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 64.0
    }

    pub fn saturating_add(self, o: Fx) -> Fx {
        Fx(self.0.saturating_add(o.0))
    }

    pub fn saturating_sub(self, o: Fx) -> Fx {
        Fx(self.0.saturating_sub(o.0))
    }

    /// `self × num / den` in one i64 multiply, rounding half up
    /// (toward +∞). A zero denominator saturates by sign instead of
    /// panicking.
    pub fn mul_ratio(self, num: i32, den: i32) -> Fx {
        let mut n = self.0 as i64 * num as i64;
        let mut d = den as i64;
        if d == 0 {
            return match n.cmp(&0) {
                core::cmp::Ordering::Greater => Fx(i32::MAX),
                core::cmp::Ordering::Less => Fx(i32::MIN),
                core::cmp::Ordering::Equal => Fx::ZERO,
            };
        }
        if d < 0 {
            n = -n;
            d = -d;
        }
        saturate_i64((n + d / 2).div_euclid(d))
    }

    /// Font units → layout units at `size`: the one formula every
    /// font-unit conversion uses. `(units × size + upem/2) / upem` in
    /// i64 with floor division, so the result rounds half up (toward
    /// +∞) for negatives too — exact negative multiples stay exact. A
    /// zero upem (impossible past registry load) yields zero.
    pub fn scale_font_units(units: i32, size: Fx, upem: u16) -> Fx {
        if upem == 0 {
            return Fx::ZERO;
        }
        let upem = upem as i64;
        saturate_i64((units as i64 * size.0 as i64 + upem / 2).div_euclid(upem))
    }
}

fn saturate_i64(v: i64) -> Fx {
    if v > i32::MAX as i64 {
        Fx(i32::MAX)
    } else if v < i32::MIN as i64 {
        Fx(i32::MIN)
    } else {
        Fx(v as i32)
    }
}

impl core::ops::Add for Fx {
    type Output = Fx;
    fn add(self, o: Fx) -> Fx {
        self.saturating_add(o)
    }
}

impl core::ops::Sub for Fx {
    type Output = Fx;
    fn sub(self, o: Fx) -> Fx {
        self.saturating_sub(o)
    }
}

impl core::ops::Neg for Fx {
    type Output = Fx;
    fn neg(self) -> Fx {
        // -i32::MIN overflows; saturate like everything else.
        Fx(self.0.checked_neg().unwrap_or(i32::MAX))
    }
}

impl core::ops::AddAssign for Fx {
    fn add_assign(&mut self, o: Fx) {
        *self = *self + o;
    }
}

impl core::ops::SubAssign for Fx {
    fn sub_assign(&mut self, o: Fx) {
        *self = *self - o;
    }
}

#[cfg(test)]
#[path = "fixed_tests.rs"]
mod tests;
