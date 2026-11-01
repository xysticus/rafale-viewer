//! Indice de netteté sur la zone AF : variance du laplacien, calculée sur la
//! luminance réduite de moitié (moyenne 2 × 2) pour atténuer le bruit des ISO
//! élevés. L'indice n'a de sens que comparé entre vues d'une même rafale :
//! même sujet, même éclairage, même bruit.

/// `gray` : luminance pleine résolution, non tournée ; `af` : x, y, largeur,
/// hauteur en proportions de cette image.
pub fn score(gray: &[u8], width: usize, height: usize, af: [f32; 4]) -> Option<f32> {
    let px = |v: f32, max: usize| ((v * max as f32).round().max(0.0) as usize).min(max);
    let (x0, y0) = (px(af[0], width), px(af[1], height));
    let (x1, y1) = (px(af[0] + af[2], width), px(af[1] + af[3], height));
    let (bw, bh) = ((x1 - x0) / 2, (y1 - y0) / 2);
    if bw < 8 || bh < 8 {
        return None;
    }
    let mut binned = vec![0f32; bw * bh];
    for by in 0..bh {
        for bx in 0..bw {
            let (x, y) = (x0 + 2 * bx, y0 + 2 * by);
            let p = |dx: usize, dy: usize| gray[(y + dy) * width + x + dx] as f32;
            binned[by * bw + bx] = (p(0, 0) + p(1, 0) + p(0, 1) + p(1, 1)) / 4.0;
        }
    }
    let at = |x: usize, y: usize| binned[y * bw + x];
    let (mut sum, mut sum2, mut n) = (0f64, 0f64, 0f64);
    for y in 1..bh - 1 {
        for x in 1..bw - 1 {
            let l = (4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1)) as f64;
            sum += l;
            sum2 += l * l;
            n += 1.0;
        }
    }
    let mean = sum / n;
    Some((sum2 / n - mean * mean) as f32)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn net_plus_que_flou() {
        // Damier de 4 px, puis le même adouci par une moyenne 5 × 5.
        let (w, h) = (128, 128);
        let sharp: Vec<u8> = (0..w * h)
            .map(|i| if ((i % w) / 4 + (i / w) / 4) % 2 == 0 { 40 } else { 210 })
            .collect();
        let blurred: Vec<u8> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                let mut s = 0u32;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let (xx, yy) = ((x + dx).clamp(0, w as isize - 1), (y + dy).clamp(0, h as isize - 1));
                        s += sharp[yy as usize * w + xx as usize] as u32;
                    }
                }
                (s / 25) as u8
            })
            .collect();
        let af = [0.25, 0.25, 0.5, 0.5];
        let a = score(&sharp, w, h, af).unwrap();
        let b = score(&blurred, w, h, af).unwrap();
        assert!(a > 2.0 * b, "net {a}, flou {b}");
        assert!(score(&sharp, w, h, [0.0, 0.0, 0.05, 0.05]).is_none(), "zone trop petite");
    }
}
