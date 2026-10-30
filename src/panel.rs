//! Panneau d'informations : texte rastérisé sur CPU, envoyé au GPU seulement s'il change.

use fontdue::{Font, FontSettings};

const PADDING: f32 = 32.0;

pub struct Line {
    pub text: String,
    pub size: f32,
    /// Couleur sRGB.
    pub color: [u8; 3],
    /// Espace ajouté au-dessus de la ligne.
    pub gap: f32,
}

pub struct Panel {
    font: Option<Font>,
}

impl Panel {
    pub fn load() -> Self {
        let font = system_font().and_then(|path| {
            let data = std::fs::read(&path).ok()?;
            match Font::from_bytes(data, FontSettings::default()) {
                Ok(f) => Some(f),
                Err(e) => {
                    log::warn!("police {path} : {e}");
                    None
                }
            }
        });
        if font.is_none() {
            log::warn!("aucune police : panneau d'informations vide");
        }
        Panel { font }
    }

    /// Rend les lignes dans une image RGBA de largeur `width` (alpha = couverture).
    pub fn render(&self, width: u32, lines: &[Line]) -> (Vec<u8>, u32, u32) {
        let Some(font) = &self.font else {
            return (vec![0; 4], 1, 1);
        };
        let mut y = PADDING;
        let mut baselines = Vec::with_capacity(lines.len());
        for line in lines {
            let m = font.horizontal_line_metrics(line.size).unwrap();
            y += line.gap + m.ascent;
            baselines.push(y);
            y += -m.descent + m.line_gap;
        }
        let height = (y + PADDING).ceil() as u32;
        let mut pixels = vec![0u8; (4 * width * height) as usize];

        for (line, &baseline) in lines.iter().zip(&baselines) {
            let mut x = PADDING;
            for ch in line.text.chars() {
                let (m, coverage) = font.rasterize(ch, line.size);
                let gx = x.round() as i32 + m.xmin;
                let gy = baseline.round() as i32 - m.ymin - m.height as i32;
                if gx + m.width as i32 > width as i32 - PADDING as i32 / 2 {
                    break; // texte trop long : on tronque
                }
                for row in 0..m.height {
                    for col in 0..m.width {
                        let (px, py) = (gx + col as i32, gy + row as i32);
                        if px < 0 || py < 0 || py >= height as i32 {
                            continue;
                        }
                        let a = coverage[row * m.width + col];
                        let i = 4 * (py as usize * width as usize + px as usize);
                        if a > pixels[i + 3] {
                            pixels[i..i + 3].copy_from_slice(&line.color);
                            pixels[i + 3] = a;
                        }
                    }
                }
                x += m.advance_width;
            }
        }
        (pixels, width, height)
    }
}

fn system_font() -> Option<String> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans-serif"])
        .output()
        .ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    (!path.is_empty()).then_some(path)
}
