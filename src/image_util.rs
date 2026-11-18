//! Resizes pictures by averaging pixels, so they stay sharp instead of being squashed by the graphics card.
//! (The graphics card's own scaling is a simple blend; shrinking a lot with it looks soft and jagged.)

/// Resizes RGBA pixels from `sw` x `sh` to `dw` x `dh` (box filter). Alpha-weighted, so the
/// edges of a transparent picture don't get dark fringes.
pub fn box_resize(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || src.len() < sw * sh * 4 {
        return out;
    }
    for dy in 0..dh {
        let y0 = dy * sh / dh;
        let y1 = (((dy + 1) * sh + dh - 1) / dh).clamp(y0 + 1, sh);
        for dx in 0..dw {
            let x0 = dx * sw / dw;
            let x1 = (((dx + 1) * sw + dw - 1) / dw).clamp(x0 + 1, sw);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * sw + x) * 4;
                    let al = src[i + 3] as u64;
                    r += src[i] as u64 * al;
                    g += src[i + 1] as u64 * al;
                    b += src[i + 2] as u64 * al;
                    a += al;
                    n += 1;
                }
            }
            if a > 0 {
                let o = (dy * dw + dx) * 4;
                out[o] = (r / a) as u8;
                out[o + 1] = (g / a) as u8;
                out[o + 2] = (b / a) as u8;
                out[o + 3] = (a / n) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::box_resize;

    #[test]
    fn averages_blocks_of_pixels() {
        let src = [0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255, 100, 100, 100, 255];
        assert_eq!(box_resize(&src, 2, 2, 1, 1), [100, 100, 100, 255]);
    }

    #[test]
    fn same_size_is_a_copy_and_enlarging_repeats_pixels() {
        let src = [10, 20, 30, 255, 40, 50, 60, 255];
        assert_eq!(box_resize(&src, 2, 1, 2, 1), src);
        assert_eq!(box_resize(&src, 2, 1, 4, 1), [10, 20, 30, 255, 10, 20, 30, 255, 40, 50, 60, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn transparent_pixels_do_not_darken_the_edge() {
        // one opaque blue pixel next to a fully transparent (black) one
        let src = [0, 0, 255, 255, 0, 0, 0, 0];
        assert_eq!(box_resize(&src, 2, 1, 1, 1), [0, 0, 255, 127]);
    }

    #[test]
    fn bad_input_gives_an_empty_picture_not_a_crash() {
        assert_eq!(box_resize(&[], 0, 0, 2, 2), vec![0u8; 16]);
    }
}
