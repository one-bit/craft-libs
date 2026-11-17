// The pitch searches of a zoom-following grid, written for this crate (the ladder follows the usual 1, 2, 5 steps of a ruler).

//! Helpers for a lattice whose pitch follows the zoom. A [`crate::Lattice`] is lines at `origin + n * pitch`; which base pitch and spacing to use
//! is the app's, and these find the pitch that keeps the lines apart on screen. Each search is bounded.

/// A relative slack for "at least this far apart", so that a pitch that lands exactly on the limit is not turned down by the last bit of a
/// quotient.
const SLACK: f64 = 1.0e-9;

/// The most doublings a search makes.
pub const MAX_DOUBLINGS: u32 = 64;

/// The smallest `base × 2^n` that puts lines at least `min_screen` screen units apart at `zoom` (screen units per document unit), for whole
/// `n` from 0 up to [`MAX_DOUBLINGS`]. `None` when a number is not finite or not above 0. A zoom so small that [`MAX_DOUBLINGS`] is not enough
/// gives the last pitch tried.
pub fn doubling(base: f64, min_screen: f64, zoom: f64) -> Option<f64> {
    if !(base.is_finite() && base > 0.0 && min_screen.is_finite() && min_screen >= 0.0 && zoom.is_finite() && zoom > 0.0) {
        return None;
    }
    let mut pitch = base;
    for _ in 0..MAX_DOUBLINGS {
        if pitch * zoom >= min_screen * (1.0 - SLACK) || !(pitch * 2.0).is_finite() {
            break;
        }
        pitch *= 2.0;
    }
    Some(pitch)
}

/// The smallest of 1, 2 and 5 times a power of ten that is at least `min`; 1 for a `min` that is not above 0 or not finite.
pub fn one_two_five(min: f64) -> f64 {
    if !(min.is_finite() && min > 0.0) {
        return 1.0;
    }
    let base = 10f64.powf(min.log10().floor().clamp(-300.0, 300.0));
    [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * base).find(|c| *c >= min * (1.0 - SLACK)).unwrap_or(10.0 * base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pitch_doubles_until_the_lines_are_far_enough_apart() {
        assert_eq!(doubling(0.125, 16.0, 96.0), Some(0.25), "0.125 * 96 = 12 < 16; 0.25 * 96 = 24");
        assert_eq!(doubling(0.125, 12.0, 96.0), Some(0.125), "exactly on the limit is enough");
        assert_eq!(doubling(1.0, 16.0, 1.0), Some(16.0));
        assert_eq!(doubling(1.0, 17.0, 1.0), Some(32.0));
        for bad in [(0.0, 1.0, 1.0), (1.0, f64::NAN, 1.0), (1.0, 1.0, 0.0), (f64::INFINITY, 1.0, 1.0), (-1.0, 1.0, 1.0)] {
            assert_eq!(doubling(bad.0, bad.1, bad.2), None, "{bad:?}");
        }
        let far = doubling(1.0, 1.0e300, 1.0e-300).unwrap();
        assert!(far.is_finite() && far <= 2.0_f64.powi(64), "bounded: {far}");
    }

    #[test]
    fn the_ladder_is_one_two_five() {
        for (x, want) in [(0.7, 1.0), (1.0, 1.0), (1.1, 2.0), (2.0, 2.0), (3.0, 5.0), (5.0, 5.0), (6.0, 10.0), (0.03, 0.05), (480.0, 500.0)] {
            assert_eq!(one_two_five(x), want, "{x}");
        }
        assert_eq!(one_two_five(0.0), 1.0);
        assert_eq!(one_two_five(f64::NAN), 1.0);
        assert!(one_two_five(1.0e300).is_finite());
    }

    #[test]
    fn a_pitch_that_is_just_the_minimum_on_the_screen_is_kept() {
        assert_eq!(doubling(2.0, 8.0, 1.0), Some(8.0));
        assert_eq!(doubling(1.0, 8.0, 2.0), Some(4.0));
        assert_eq!(doubling(10.0, 8.0, 1.0), Some(10.0));
    }

    #[test]
    fn a_one_two_five_pitch_is_the_smallest_of_them_that_holds_the_minimum() {
        for (min, want) in [(0.3, 0.5), (3.0, 5.0), (7.0, 10.0), (1.0, 1.0), (2.0, 2.0), (5.0, 5.0), (12.0, 20.0), (0.012, 0.02)] {
            assert!((one_two_five(min) - want).abs() < 1e-12, "{min}: {}", one_two_five(min));
        }
    }
}
