//! Pixel snapping for crisp HiDPI rows (pure, no windowing types).

// IMPL_PLACEHOLDER

#[cfg(test)]
mod tests {
    use super::*;

    const PPPS: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

    #[test]
    fn rows_land_on_physical_pixels() {
        for ppp in PPPS {
            for i in 0..200 {
                let p = row_y(37.3, i, 20.0, ppp) * ppp;
                assert!((p - p.round()).abs() < 1e-3, "ppp {ppp} i {i} p {p}");
            }
        }
    }

    #[test]
    fn rows_strictly_increasing() {
        for ppp in PPPS {
            for i in 1..200 {
                assert!(row_y(37.3, i, 20.0, ppp) > row_y(37.3, i - 1, 20.0, ppp));
            }
        }
    }

    #[test]
    fn rows_stay_close_to_ideal() {
        for ppp in PPPS {
            for i in 0..200 {
                let d = (row_y(37.3, i, 20.0, ppp) - (37.3 + 20.0 * i as f32)).abs();
                assert!(d <= 0.5 / ppp + 1e-3);
            }
        }
    }
}
