//! The three floating-point wrappers from `src/static/libc64/math64.c`.

/// `fatan2`'s C ABI argument and return width.
#[unsafe(no_mangle)]
pub extern "C" fn fatan2(x: f32, y: f32) -> f32 {
    // TARGET_PC's <math.h> declares atan2(double, double).  The explicit
    // casts document the same default argument promotions as the oracle.
    (f64::from(x).atan2(f64::from(y))) as f32
}

/// `fsqrt`'s source-level Rust spelling.
pub fn fsqrt(x: f32) -> f32 {
    x.sqrt()
}

/// PC's `math64.h` macro changes the exported `fsqrt` symbol to this name.
#[unsafe(no_mangle)]
pub extern "C" fn libc64_fsqrt(x: f32) -> f32 {
    fsqrt(x)
}

/// `facos` with the C wrapper's promoted `double` calculation.
#[unsafe(no_mangle)]
pub extern "C" fn facos(x: f32) -> f32 {
    f64::from(x).acos() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_sqrt_alias_has_the_same_f32_result() {
        for bits in [0x0000_0000, 0x3f80_0000, 0x4000_0000, 0x7f80_0000] {
            let x = f32::from_bits(bits);
            assert_eq!(fsqrt(x).to_bits(), libc64_fsqrt(x).to_bits());
        }
    }
}
