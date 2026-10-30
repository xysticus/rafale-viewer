//! Panneau de droite, rastérisé sur CPU et renvoyé au GPU seulement s'il change :
//! informations sur l'image courante, puis liste des rafales avec vignettes.

use std::num::NonZeroUsize;
use std::sync::Arc;

use fontdue::{Font, FontSettings};
use lru::LruCache;
use turbojpeg::{Decompressor, Image, PixelFormat};

use crate::meta::Meta;

pub const PADDING: f32 = 28.0;
const THUMBS_PER_ROW: usize = 4;
const THUMB_GAP: f32 = 8.0;
const ROW_HEADER: f32 = 32.0;
const ROW_GAP: f32 = 14.0;
const HIGHLIGHT: [u8; 4] = [255, 255, 255, 22];
const MARKED: [u8; 3] = [110, 210, 120];

pub struct Line {
    pub text: String,
    pub size: f32,
    /// Couleur sRGB.
    pub color: [u8; 3],
    /// Espace ajouté au-dessus de la ligne.
    pub gap: f32,
}

/// Une rafale telle qu'affichée dans la liste.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub start: usize,
    pub len: usize,
    pub number: usize,
    /// Rythme mesuré, en images/s.
    pub rate: Option<f32>,
    pub marked: usize,
    /// Rafale de l'image courante.
    pub current: bool,
}

/// Image RGBA (alpha = couverture) en cours de composition.
struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Canvas {
            width: width as usize,
            height: height as usize,
            pixels: vec![0; 4 * width as usize * height as usize],
        }
    }

    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let x0 = (x.round().max(0.0) as usize).min(self.width);
        let y0 = (y.round().max(0.0) as usize).min(self.height);
        let x1 = ((x + w).round().max(0.0) as usize).min(self.width);
        let y1 = ((y + h).round().max(0.0) as usize).min(self.height);
        for py in y0..y1 {
            for px in x0..x1 {
                let i = 4 * (py * self.width + px);
                self.pixels[i..i + 4].copy_from_slice(&color);
            }
        }
    }

    fn blit(&mut self, x: usize, y: usize, img: &Thumb) {
        for row in 0..img.height.min(self.height.saturating_sub(y)) {
            let w = img.width.min(self.width.saturating_sub(x));
            let src = &img.pixels[4 * row * img.width..][..4 * w];
            let i = 4 * ((y + row) * self.width + x);
            self.pixels[i..i + 4 * w].copy_from_slice(src);
        }
    }

    /// Écrit `text` à partir de `x` sur la ligne de base `baseline` ; tronque à `max_x`.
    fn text(&mut self, font: &Font, x: f32, baseline: f32, max_x: f32, text: &str, size: f32, color: [u8; 3]) {
        let mut x = x;
        for ch in text.chars() {
            let (m, coverage) = font.rasterize(ch, size);
            let gx = x.round() as i32 + m.xmin;
            let gy = baseline.round() as i32 - m.ymin - m.height as i32;
            if (gx + m.width as i32) as f32 > max_x {
                break;
            }
            for row in 0..m.height {
                for col in 0..m.width {
                    let (px, py) = (gx + col as i32, gy + row as i32);
                    if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
                        continue;
                    }
                    let a = coverage[row * m.width + col];
                    let i = 4 * (py as usize * self.width + px as usize);
                    if a > self.pixels[i + 3] {
                        self.pixels[i..i + 3].copy_from_slice(&color);
                        self.pixels[i + 3] = a;
                    }
                }
            }
            x += m.advance_width;
        }
    }
}

struct Thumb {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

pub struct Panel {
    font: Option<Font>,
    dec: Decompressor,
    /// Vignettes décodées, orientées et mises à la taille des cases.
    thumbs: LruCache<(usize, u32), Arc<Thumb>>,
}

/// Résultat d'un rendu de la liste : image et position verticale de chaque ligne.
pub struct ListImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// (début de rafale, haut, bas) en pixels dans l'image.
    pub hits: Vec<(usize, f32, f32)>,
    /// Emplacement du repère de position sous la rafale courante : (x, y, largeur).
    pub track: Option<(f32, f32, f32)>,
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
        Panel {
            font,
            dec: Decompressor::new().expect("initialisation de turbojpeg"),
            thumbs: LruCache::new(NonZeroUsize::new(512).unwrap()),
        }
    }

    /// Rend les lignes d'informations dans une image de largeur `width`.
    pub fn render_info(&self, width: u32, lines: &[Line]) -> (Vec<u8>, u32, u32) {
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
        let height = (y + PADDING / 2.0).ceil() as u32;
        let mut canvas = Canvas::new(width, height);
        let max_x = width as f32 - PADDING / 2.0;
        for (line, &baseline) in lines.iter().zip(&baselines) {
            canvas.text(font, PADDING, baseline, max_x, &line.text, line.size, line.color);
        }
        (canvas.pixels, width, height)
    }

    /// Hauteur d'une ligne de rafale pour une largeur de panneau donnée.
    pub fn row_height(width: u32) -> f32 {
        let (_, th) = thumb_size(width);
        ROW_HEADER + th as f32 + ROW_GAP
    }

    /// Rend les lignes `rows` (déjà choisies pour tenir dans `height`).
    pub fn render_list(&mut self, width: u32, height: u32, rows: &[Row], metas: &[Option<Meta>], status: &str) -> ListImage {
        let mut canvas = Canvas::new(width, height.max(1));
        let mut hits = Vec::with_capacity(rows.len());
        let mut track = None;
        let Some(font) = self.font.take() else {
            return ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, track };
        };
        let (tw, th) = thumb_size(width);
        let row_h = Self::row_height(width);
        let max_x = width as f32 - PADDING / 2.0;
        let mut y = 0.0;

        if !status.is_empty() {
            canvas.text(&font, PADDING, y + 22.0, max_x, status, 20.0, [125, 125, 125]);
            y += ROW_HEADER;
        }
        for row in rows {
            let top = y;
            if row.current {
                canvas.fill(PADDING / 2.0, top, width as f32 - PADDING, row_h - ROW_GAP / 2.0, HIGHLIGHT);
            }
            let clock = metas
                .get(row.start)
                .and_then(|m| m.as_ref())
                .map_or("", |m| m.clock.as_str());
            let rate = row.rate.map(|r| format!("   {}", format_rate(r))).unwrap_or_default();
            let header = format!("Rafale {}   {clock}   {} images{rate}", row.number, row.len);
            let color = if row.current { [235, 235, 235] } else { [160, 160, 160] };
            canvas.text(&font, PADDING, top + 23.0, max_x, &header, 20.0, color);
            if row.marked > 0 {
                let mark = format!("★ {}", row.marked);
                canvas.text(&font, width as f32 - PADDING - 50.0, top + 23.0, max_x, &mark, 20.0, MARKED);
            }

            // Vignettes réparties sur la rafale : première, …, dernière.
            let n = THUMBS_PER_ROW.min(row.len);
            for k in 0..n {
                let index = if n == 1 {
                    row.start
                } else {
                    row.start + k * (row.len - 1) / (n - 1)
                };
                let x = PADDING + k as f32 * (tw as f32 + THUMB_GAP);
                let ty = top + ROW_HEADER;
                if let Some(t) = self.thumb(index, tw, th, metas) {
                    // Centrée dans sa case (vignettes portrait plus étroites).
                    let ox = (tw - t.width as u32) / 2;
                    let oy = (th - t.height as u32) / 2;
                    canvas.blit(x as usize + ox as usize, ty as usize + oy as usize, &t);
                } else {
                    canvas.fill(x, ty, tw as f32, th as f32, [255, 255, 255, 12]);
                }
            }
            // Le repère de position est dessiné à part (quad GPU) : la liste
            // n'a pas à être recomposée à chaque image du défilé.
            if row.current {
                let y = top + ROW_HEADER + th as f32 + 3.0;
                track = Some((PADDING, y, width as f32 - 2.0 * PADDING));
            }
            hits.push((row.start, top, top + row_h));
            y += row_h;
        }
        self.font = Some(font);
        ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, track }
    }

    fn thumb(&mut self, index: usize, tw: u32, th: u32, metas: &[Option<Meta>]) -> Option<Arc<Thumb>> {
        if let Some(t) = self.thumbs.get(&(index, tw)) {
            return Some(t.clone());
        }
        let meta = metas.get(index)?.as_ref()?;
        let data = meta.thumb.as_ref()?;
        let header = self.dec.read_header(data).ok()?;
        let mut img = Image {
            pixels: vec![0u8; 4 * header.width * header.height],
            width: header.width,
            pitch: 4 * header.width,
            height: header.height,
            format: PixelFormat::RGBA,
        };
        self.dec.decompress(data, img.as_deref_mut()).ok()?;
        let src = Thumb {
            width: header.width,
            height: header.height,
            pixels: img.pixels,
        };
        let t = Arc::new(fit(&orient(src, meta.orientation), tw, th));
        self.thumbs.put((index, tw), t.clone());
        Some(t)
    }
}

/// « 25 i/s », ou « 12,5 i/s » si le rythme n'est pas entier.
pub fn format_rate(rate: f32) -> String {
    if (rate - rate.round()).abs() < 0.05 {
        format!("{} i/s", rate.round())
    } else {
        format!("{:.1} i/s", rate).replace('.', ",")
    }
}

/// Taille d'une case de vignette (4:3) pour une largeur de panneau.
fn thumb_size(width: u32) -> (u32, u32) {
    let inner = width as f32 - 2.0 * PADDING;
    let w = ((inner - THUMB_GAP * (THUMBS_PER_ROW - 1) as f32) / THUMBS_PER_ROW as f32).max(16.0);
    (w as u32, (w * 3.0 / 4.0) as u32)
}

/// Applique l'orientation EXIF (même correspondance que le shader).
fn orient(src: Thumb, orientation: u8) -> Thumb {
    if orientation <= 1 || orientation > 8 {
        return src;
    }
    let (sw, sh) = (src.width, src.height);
    let (dw, dh) = if orientation >= 5 { (sh, sw) } else { (sw, sh) };
    let mut pixels = vec![0u8; 4 * dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let (sx, sy) = match orientation {
                2 => (sw - 1 - x, y),
                3 => (sw - 1 - x, sh - 1 - y),
                4 => (x, sh - 1 - y),
                5 => (y, x),
                6 => (y, sh - 1 - x),
                7 => (sw - 1 - y, sh - 1 - x),
                _ => (sw - 1 - y, x),
            };
            let s = 4 * (sy * sw + sx);
            let d = 4 * (y * dw + x);
            pixels[d..d + 4].copy_from_slice(&src.pixels[s..s + 4]);
        }
    }
    Thumb {
        width: dw,
        height: dh,
        pixels,
    }
}

/// Redimensionne (bilinéaire) pour tenir dans `tw` × `th` en gardant les proportions.
fn fit(src: &Thumb, tw: u32, th: u32) -> Thumb {
    let scale = (tw as f32 / src.width as f32).min(th as f32 / src.height as f32);
    let dw = ((src.width as f32 * scale).round() as usize).max(1);
    let dh = ((src.height as f32 * scale).round() as usize).max(1);
    let mut pixels = vec![0u8; 4 * dw * dh];
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, (src.height - 1) as f32);
        let (y0, wy) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(src.height - 1);
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, (src.width - 1) as f32);
            let (x0, wx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(src.width - 1);
            let d = 4 * (y * dw + x);
            for c in 0..4 {
                let p = |xx: usize, yy: usize| src.pixels[4 * (yy * src.width + xx) + c] as f32;
                let top = p(x0, y0) * (1.0 - wx) + p(x1, y0) * wx;
                let bottom = p(x0, y1) * (1.0 - wx) + p(x1, y1) * wx;
                pixels[d + c] = (top * (1.0 - wy) + bottom * wy).round() as u8;
            }
        }
    }
    Thumb {
        width: dw,
        height: dh,
        pixels,
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
