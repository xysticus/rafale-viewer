//! Panneau de droite, rastérisé sur CPU et renvoyé au GPU seulement s'il change :
//! informations sur l'image courante, puis liste des rafales avec vignettes.

use std::num::NonZeroUsize;
use std::sync::Arc;

use fontdue::{Font, FontSettings};
use lru::LruCache;
use turbojpeg::{Decompressor, Image, PixelFormat};

use crate::marks::Color;
use crate::meta::Meta;

pub const PADDING: f32 = 24.0;
const THUMBS_PER_ROW: usize = 4;
const THUMB_GAP: f32 = 8.0;
const ROW_HEADER: f32 = 25.0;
const ROW_GAP: f32 = 14.0;
const HIGHLIGHT: [u8; 4] = [255, 255, 255, 22];
const MARKED: [u8; 3] = [110, 210, 120];
/// Rangée de commandes (étoiles, couleurs, coche, rejet).
const CTRL: f32 = 22.0;
const CTRL_GAP: f32 = 18.0;
const STAR_ON: [u8; 3] = [235, 190, 60];
const OFF: [u8; 3] = [85, 85, 85];
const CHECK_ON: [u8; 3] = [80, 175, 85];
const REJECT_ON: [u8; 3] = [205, 70, 60];
/// Hauteur de la barre de filtre en haut de la liste.
pub const FILTER_BAR: f32 = CTRL + 16.0;

/// Élément cliquable du panneau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Widget {
    Star(u8),
    Color(Color),
    Check,
    Reject,
    ClearFilter,
}

/// État affiché par une rangée de commandes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Controls {
    pub rating: u8,
    pub colors: [bool; 5],
    pub check: bool,
    pub reject: bool,
}

/// Zone cliquable, en pixels dans l'image du panneau.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub widget: Widget,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Hit {
    pub fn contains(&self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

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
    /// Images de la rafale qui passent le filtre (None sans filtre).
    pub shown: Option<usize>,
    /// Images dont on montre la vignette.
    pub picks: Vec<usize>,
    /// Première image visible (cible d'un clic).
    pub first: usize,
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

    /// Compose une couleur par-dessus le pixel (alpha non prémultiplié).
    fn blend(&mut self, px: usize, py: usize, rgb: [u8; 3], a: f32) {
        if px >= self.width || py >= self.height || a <= 0.0 {
            return;
        }
        let i = 4 * (py * self.width + px);
        let da = self.pixels[i + 3] as f32 / 255.0;
        let out = a + da * (1.0 - a);
        for c in 0..3 {
            let v = (rgb[c] as f32 * a + self.pixels[i + c] as f32 * da * (1.0 - a)) / out;
            self.pixels[i + c] = v.round() as u8;
        }
        self.pixels[i + 3] = (out * 255.0).round() as u8;
    }

    /// Remplit une forme définie par `inside`, avec anticrénelage (4 × 4 échantillons).
    fn shape(&mut self, x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3], alpha: f32, inside: impl Fn(f32, f32) -> bool) {
        let (x0, y0) = (x.floor().max(0.0) as usize, y.floor().max(0.0) as usize);
        let (x1, y1) = ((x + w).ceil() as usize, (y + h).ceil() as usize);
        for py in y0..y1.min(self.height) {
            for px in x0..x1.min(self.width) {
                let mut n = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let fx = px as f32 + (sx as f32 + 0.5) / 4.0;
                        let fy = py as f32 + (sy as f32 + 0.5) / 4.0;
                        n += inside(fx, fy) as u32;
                    }
                }
                self.blend(px, py, rgb, alpha * n as f32 / 16.0);
            }
        }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3], alpha: f32) {
        self.shape(x, y, w, h, rgb, alpha, |_, _| true);
    }

    fn frame(&mut self, x: f32, y: f32, s: f32, t: f32, rgb: [u8; 3]) {
        self.shape(x, y, s, s, rgb, 1.0, |px, py| {
            px < x + t || px > x + s - t || py < y + t || py > y + s - t
        });
    }

    /// Trait épais entre une suite de points.
    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: [u8; 3]) {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        let r = width / 2.0;
        self.shape(x0 - r, y0 - r, x1 - x0 + width, y1 - y0 + width, rgb, 1.0, |px, py| {
            pts.windows(2).any(|s| segment_distance((px, py), s[0], s[1]) <= r)
        });
    }

    /// Étoile à cinq branches inscrite dans le carré (x, y, s).
    fn star(&mut self, x: f32, y: f32, s: f32, rgb: [u8; 3]) {
        let (cx, cy) = (x + s / 2.0, y + s / 2.0 + s * 0.04);
        let pts: Vec<(f32, f32)> = (0..10)
            .map(|k| {
                let r = if k % 2 == 0 { s * 0.5 } else { s * 0.2 };
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.shape(x, y, s, s, rgb, 1.0, |px, py| point_in_polygon((px, py), &pts));
    }

    /// Rangée de commandes à partir de (x, y) ; renvoie les zones cliquables.
    fn controls(&mut self, x: f32, y: f32, c: &Controls) -> Vec<Hit> {
        let mut hits = Vec::with_capacity(12);
        let mut x = x;
        for n in 1..=5u8 {
            let rgb = if n <= c.rating { STAR_ON } else { OFF };
            self.star(x, y, CTRL, rgb);
            hits.push(Hit { widget: Widget::Star(n), x, y, w: CTRL + 2.0, h: CTRL });
            x += CTRL + 2.0;
        }
        x += CTRL_GAP;
        let chip = CTRL - 4.0;
        for color in Color::ALL {
            let on = c.colors[color.index()];
            let (cx, cy) = (x + 2.0, y + 2.0);
            if on {
                self.rect(cx - 2.0, cy - 2.0, chip + 4.0, chip + 4.0, [235, 235, 235], 1.0);
            }
            self.rect(cx, cy, chip, chip, color.rgb(), if on { 1.0 } else { 0.35 });
            hits.push(Hit { widget: Widget::Color(color), x, y, w: CTRL + 2.0, h: CTRL });
            x += CTRL + 4.0;
        }
        x += CTRL_GAP;
        let s = CTRL;
        if c.check {
            self.rect(x, y, s, s, CHECK_ON, 1.0);
            let pts = [(x + s * 0.24, y + s * 0.52), (x + s * 0.43, y + s * 0.72), (x + s * 0.78, y + s * 0.3)];
            self.polyline(&pts, 2.6, [255, 255, 255]);
        } else {
            self.frame(x, y, s, 2.0, OFF);
        }
        hits.push(Hit { widget: Widget::Check, x, y, w: s, h: s });
        x += s + 8.0;
        if c.reject {
            self.rect(x, y, s, s, REJECT_ON, 1.0);
            let (a, b) = (s * 0.28, s * 0.72);
            self.polyline(&[(x + a, y + a), (x + b, y + b)], 2.6, [255, 255, 255]);
            self.polyline(&[(x + b, y + a), (x + a, y + b)], 2.6, [255, 255, 255]);
        } else {
            self.frame(x, y, s, 2.0, OFF);
        }
        hits.push(Hit { widget: Widget::Reject, x, y, w: s, h: s });
        hits
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
    /// (image à afficher au clic, haut, bas) en pixels dans l'image.
    pub hits: Vec<(usize, f32, f32)>,
    /// Commandes de la barre de filtre.
    pub filter_hits: Vec<Hit>,
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

    /// Rend la rangée de commandes puis les lignes d'informations, en largeur `width`.
    pub fn render_info(&self, width: u32, controls: &Controls, lines: &[Line]) -> (Vec<u8>, u32, u32, Vec<Hit>) {
        let Some(font) = &self.font else {
            return (vec![0; 4], 1, 1, Vec::new());
        };
        let mut y = PADDING + CTRL + 6.0;
        let mut baselines = Vec::with_capacity(lines.len());
        for line in lines {
            let m = font.horizontal_line_metrics(line.size).unwrap();
            y += line.gap + m.ascent;
            baselines.push(y);
            y += -m.descent + m.line_gap;
        }
        let height = (y + PADDING / 2.0).ceil() as u32;
        let mut canvas = Canvas::new(width, height);
        let hits = canvas.controls(PADDING, PADDING, controls);
        let max_x = width as f32 - PADDING / 2.0;
        for (line, &baseline) in lines.iter().zip(&baselines) {
            canvas.text(font, PADDING, baseline, max_x, &line.text, line.size, line.color);
        }
        (canvas.pixels, width, height, hits)
    }

    /// Hauteur d'une ligne de rafale pour une largeur de panneau donnée.
    pub fn row_height(width: u32) -> f32 {
        let (_, th) = thumb_size(width);
        ROW_HEADER + th as f32 + ROW_GAP
    }

    /// Rend la barre de filtre puis les lignes `rows` (déjà choisies pour tenir dans `height`).
    #[allow(clippy::too_many_arguments)]
    pub fn render_list(
        &mut self,
        width: u32,
        height: u32,
        filter: &Controls,
        filter_text: &str,
        rows: &[Row],
        metas: &[Option<Meta>],
        status: &str,
    ) -> ListImage {
        let mut canvas = Canvas::new(width, height.max(1));
        let mut hits = Vec::with_capacity(rows.len());
        let mut track = None;
        let mut filter_hits = canvas.controls(PADDING, 6.0, filter);
        let Some(font) = self.font.take() else {
            return ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, filter_hits, track };
        };
        let (tw, th) = thumb_size(width);
        let row_h = Self::row_height(width);
        let max_x = width as f32 - PADDING / 2.0;

        // À droite des commandes de filtre : nombre de photos et « effacer ».
        let after = filter_hits.last().map_or(PADDING, |h| h.x + h.w) + 16.0;
        let active = filter != &Controls::default();
        let color = if active { [215, 215, 215] } else { [120, 120, 120] };
        canvas.text(&font, after, 6.0 + 16.0, max_x, filter_text, 15.0, color);
        if active {
            let label = "effacer";
            let w = 56.0;
            let x = width as f32 - PADDING - w;
            canvas.text(&font, x, 6.0 + 16.0, max_x, label, 15.0, [150, 150, 150]);
            filter_hits.push(Hit { widget: Widget::ClearFilter, x, y: 0.0, w, h: FILTER_BAR });
        }
        canvas.rect(PADDING / 2.0, FILTER_BAR - 4.0, width as f32 - PADDING, 1.0, [255, 255, 255], 0.12);
        let mut y = FILTER_BAR;

        if !status.is_empty() {
            canvas.text(&font, PADDING, y + 17.0, max_x, status, 15.0, [125, 125, 125]);
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
            let count = match row.shown {
                Some(n) => format!("{n} / {} images", row.len),
                None => format!("{} images", row.len),
            };
            let header = format!("Rafale {}   {clock}   {count}{rate}", row.number);
            let color = if row.current { [235, 235, 235] } else { [160, 160, 160] };
            canvas.text(&font, PADDING, top + 18.0, max_x, &header, 15.0, color);
            if row.marked > 0 {
                // Étoile dessinée : la police n'a pas toujours le caractère ★.
                let x = width as f32 - PADDING - 40.0;
                canvas.star(x, top + 5.0, 14.0, MARKED);
                canvas.text(&font, x + 18.0, top + 18.0, max_x, &row.marked.to_string(), 15.0, MARKED);
            }

            for (k, &index) in row.picks.iter().enumerate() {
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
            hits.push((row.first, top, top + row_h));
            y += row_h;
        }
        self.font = Some(font);
        ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, filter_hits, track }
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

/// Images de la rafale dont on montre la vignette : réparties de la première à la dernière.
pub fn spread(indices: &[usize]) -> Vec<usize> {
    let n = THUMBS_PER_ROW.min(indices.len());
    (0..n)
        .map(|k| if n == 1 { indices[0] } else { indices[k * (indices.len() - 1) / (n - 1)] })
        .collect()
}

fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.0 + t * dx - p.0, a.1 + t * dy - p.1);
    (qx * qx + qy * qy).sqrt()
}

fn point_in_polygon(p: (f32, f32), pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
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
