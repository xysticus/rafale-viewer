//! Panneau de droite, rastérisé sur CPU et renvoyé au GPU seulement s'il change :
//! informations sur l'image courante, puis liste des rafales (ou grille de
//! photos, en mode à plat) avec vignettes, et barre de filtre en bas.

use std::num::NonZeroUsize;
use std::sync::Arc;

use fontdue::{Font, FontSettings};
use lru::LruCache;
use turbojpeg::{Decompressor, Image, PixelFormat};

use crate::marks::{BurstMark, Color};
use crate::meta::Meta;

pub const PADDING: f32 = 24.0;
/// Marge au-dessus des commandes de marquage.
const INFO_TOP: f32 = 14.0;
pub const THUMBS_PER_ROW: usize = 4;
const THUMB_GAP: f32 = 8.0;
const ROW_HEADER: f32 = 25.0;
const ROW_GAP: f32 = 14.0;
const HIGHLIGHT: [u8; 4] = [255, 255, 255, 22];
const MARKED: [u8; 3] = [110, 210, 120];
/// Rangée de commandes (étoiles, couleurs, coche, rejet).
const CTRL: f32 = 22.0;
const CTRL_GAP: f32 = 18.0;
/// Écart entre la rangée de la photo et celle de sa rafale.
const BURST_ROW_GAP: f32 = 8.0;
/// Disque du Live GND, à côté de son code.
const GND_DISC: f32 = 20.0;
const STAR_ON: [u8; 3] = [235, 190, 60];
const OFF: [u8; 3] = [85, 85, 85];
const CHECK_ON: [u8; 3] = [80, 175, 85];
const REJECT_ON: [u8; 3] = [205, 70, 60];
/// Rafale bloquée : bandeau et cadenas.
const LOCKED: [u8; 3] = [70, 140, 240];
/// Signalement d'une rafale (règle de cohérence non respectée).
pub const WARNING: [u8; 3] = [240, 170, 60];
/// Hauteur de la barre de filtre des photos, puis de celle des rafales, puis de
/// la ligne de boutons (vue, aide), en bas de la liste.
const FILTER_BAR: f32 = CTRL + 16.0;
const BURST_FILTER_BAR: f32 = CTRL + 8.0;
const BUTTON_H: f32 = 24.0;
const BUTTON_LINE: f32 = BUTTON_H + 8.0;
pub const BOTTOM_BAR: f32 = FILTER_BAR + BURST_FILTER_BAR + 3.0 * BUTTON_LINE + 6.0;

/// Élément cliquable du panneau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Widget {
    Star(u8),
    Color(Color),
    Check,
    Reject,
    /// Marquage de la rafale affichée (rangée « Rafale » du panneau).
    BurstStar(u8),
    BurstCheck,
    BurstReject,
    /// Outil de la rangée du haut (L, A, C, N, G), comme sa touche.
    Tool(char),
    /// Filtre des rafales (rangée « Rafales » de la barre de filtre).
    FilterBurstStar(u8),
    FilterBurstCheck,
    ClearFilter,
    /// Photo de la liste (début de rafale, ou vignette en mode à plat).
    Photo(usize),
    /// Vue de la liste : par rafales ou à plat.
    Bursts,
    Flat,
    /// Fenêtres d'aide.
    Principles,
    Shortcuts,
    /// Fichiers à copier (0 JPEG, 1 RAW, 2 XMP), cochés indépendamment, et copie
    /// de la sélection dans le presse-papiers.
    CopyFile(u8),
    Copy,
    /// Choix du logiciel de retouche, sans lancer la retouche.
    Editor,
    /// Mettre les photos rejetées à la corbeille.
    Trash,
    /// Copier la sélection et l'ouvrir dans le logiciel de retouche.
    Retouch,
}

/// Fichiers copiés pour chaque photo de la sélection, par Ctrl+C comme par
/// « Retoucher » ; JPEG seul au départ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyFiles {
    pub jpeg: bool,
    pub raw: bool,
    /// Les `.xmp` des photos copiées (ceux des JPEG si ni JPEG ni RAW n'est coché).
    pub xmp: bool,
}

impl Default for CopyFiles {
    fn default() -> Self {
        CopyFiles { jpeg: true, raw: false, xmp: false }
    }
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
    /// Texte ; un « ⚠ » en tête (absent de la police) est dessiné.
    pub text: String,
    pub size: f32,
    /// Couleur sRGB.
    pub color: [u8; 3],
    /// Espace ajouté au-dessus de la ligne.
    pub gap: f32,
    /// Texte aligné à droite sur la même ligne, et sa couleur.
    pub right: String,
    pub right_color: [u8; 3],
}

/// Outil de la rangée du haut : sa touche, son nom, son état.
#[derive(Debug, Clone, Copy)]
pub struct Tool {
    pub key: char,
    pub label: &'static str,
    /// Actif ou non ; None pour une action (N).
    pub on: Option<bool>,
    /// Sans effet sur cette photo (pas de zone AF, de GND, de rafale) : grisé.
    pub enabled: bool,
}

/// Haut du panneau : outils, rangées de commandes et ce qu'on lit à leur droite.
pub struct Info<'a> {
    pub tools: &'a [Tool],
    pub controls: &'a Controls,
    /// Rangée de la rafale, si son marquage est possible.
    pub burst: Option<&'a Controls>,
    pub locked: bool,
    /// État de la photo en toutes lettres, et sa couleur.
    pub mark: (&'a str, [u8; 3]),
    /// Live GND : code du boîtier et direction du côté sombre.
    pub gnd: Option<(&'a str, (f32, f32))>,
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
    /// Marquage de la rafale elle-même.
    pub mark: BurstMark,
    /// Règle de cohérence non respectée (⚠).
    pub warn: bool,
    /// Rafale de l'image courante.
    pub current: bool,
}

/// Une photo de la grille, en mode à plat.
#[derive(Debug, PartialEq)]
pub struct Cell {
    pub index: usize,
    /// Photos dont la vignette tient lieu, à partir de `index` (vues espacées
    /// d'une rafale bloquée) : le cadre s'y place quand on les parcourt.
    pub span: usize,
    pub rating: u8,
    pub rejected: bool,
    /// Retenue (✓).
    pub picked: bool,
    pub color: Option<Color>,
}

/// Une ligne de la liste : une rafale, ou une rangée de photos (mode à plat).
#[derive(Debug, PartialEq)]
pub enum Entry {
    Burst(Row),
    Photos(Vec<Cell>),
}

/// État des boutons du bas : copie, puis vue (le mode à plat n'est proposé
/// que pour les petits dossiers).
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// Rafale bloquée : la ligne d'état devient un bandeau, avec un cadenas.
    pub locked: bool,
    pub flat_available: bool,
    pub flat: bool,
    pub copy_files: CopyFiles,
    /// « 12 retenues », « 40 filtrées » ; compte rendu de la dernière copie.
    pub count_label: String,
    pub copy_message: String,
    /// Bouton du logiciel de retouche : son nom, ou une invitation à le choisir.
    pub editor_label: String,
    /// Libellé du bouton de corbeille, s'il y a des photos rejetées.
    pub trash_label: Option<String>,
}

/// Emplacement du repère de position sous les photos `start..start + len` : (x, y, largeur).
#[derive(Debug, Clone, Copy)]
pub struct Track {
    pub start: usize,
    pub len: usize,
    pub x: f32,
    pub y: f32,
    pub span: f32,
    /// Vignette de la grille (x, y, largeur, hauteur) : encadrée au lieu d'être soulignée.
    pub cell: Option<(f32, f32, f32, f32)>,
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

    /// Coche verte (photo retenue) dans le carré (x, y, s).
    fn check(&mut self, x: f32, y: f32, s: f32) {
        self.rect(x, y, s, s, CHECK_ON, 1.0);
        let pts = [(x + s * 0.24, y + s * 0.52), (x + s * 0.43, y + s * 0.72), (x + s * 0.78, y + s * 0.3)];
        self.polyline(&pts, s * 0.12, [255, 255, 255]);
    }

    /// Rangée de commandes à partir de (x, y) ; renvoie les zones cliquables.
    fn controls(&mut self, x: f32, y: f32, c: &Controls) -> Vec<Hit> {
        let mut hits = Vec::with_capacity(12);
        let mut x = self.stars(x, y, c.rating, Widget::Star, &mut hits);
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
        self.check_reject(x, y, c, Widget::Check, Widget::Reject, &mut hits);
        hits
    }

    /// Cinq étoiles cliquables à partir de (x, y) ; renvoie l'abscisse de fin.
    fn stars(&mut self, x: f32, y: f32, rating: u8, widget: fn(u8) -> Widget, hits: &mut Vec<Hit>) -> f32 {
        let mut x = x;
        for n in 1..=5u8 {
            let rgb = if n <= rating { STAR_ON } else { OFF };
            self.star(x, y, CTRL, rgb);
            hits.push(Hit { widget: widget(n), x, y, w: CTRL + 2.0, h: CTRL });
            x += CTRL + 2.0;
        }
        x
    }

    /// Cases ✓ et ✕ à partir de (x, y).
    fn check_reject(&mut self, x: f32, y: f32, c: &Controls, check: Widget, reject: Widget, hits: &mut Vec<Hit>) {
        self.check_box(x, y, c.check, check, hits);
        self.reject_box(x + CTRL + 8.0, y, c.reject, reject, hits);
    }

    fn check_box(&mut self, x: f32, y: f32, on: bool, check: Widget, hits: &mut Vec<Hit>) {
        let s = CTRL;
        if on {
            self.rect(x, y, s, s, CHECK_ON, 1.0);
            let pts = [(x + s * 0.24, y + s * 0.52), (x + s * 0.43, y + s * 0.72), (x + s * 0.78, y + s * 0.3)];
            self.polyline(&pts, 2.6, [255, 255, 255]);
        } else {
            // Décochée : la coche reste visible en gris, pour la distinguer de la croix.
            self.frame(x, y, s, 2.0, OFF);
            let pts = [(x + s * 0.24, y + s * 0.52), (x + s * 0.43, y + s * 0.72), (x + s * 0.78, y + s * 0.3)];
            self.polyline(&pts, 2.2, OFF);
        }
        hits.push(Hit { widget: check, x, y, w: s, h: s });
    }

    fn reject_box(&mut self, x: f32, y: f32, on: bool, reject: Widget, hits: &mut Vec<Hit>) {
        let s = CTRL;
        if on {
            self.rect(x, y, s, s, REJECT_ON, 1.0);
            let (a, b) = (s * 0.28, s * 0.72);
            self.polyline(&[(x + a, y + a), (x + b, y + b)], 2.6, [255, 255, 255]);
            self.polyline(&[(x + b, y + a), (x + a, y + b)], 2.6, [255, 255, 255]);
        } else {
            self.frame(x, y, s, 2.0, OFF);
            let (a, b) = (s * 0.3, s * 0.7);
            self.polyline(&[(x + a, y + a), (x + b, y + b)], 2.2, OFF);
            self.polyline(&[(x + b, y + a), (x + a, y + b)], 2.2, OFF);
        }
        hits.push(Hit { widget: reject, x, y, w: s, h: s });
    }

    /// Cadenas fermé (rafale bloquée) dans le carré (x, y, s).
    fn padlock(&mut self, x: f32, y: f32, s: f32, rgb: [u8; 3]) {
        let body_y = y + s * 0.45;
        self.rect(x + s * 0.12, body_y, s * 0.76, s * 0.55, rgb, 1.0);
        let (cx, r) = (x + s / 2.0, s * 0.24);
        let arc: Vec<(f32, f32)> = (0..=12)
            .map(|k| {
                let a = std::f32::consts::PI * k as f32 / 12.0;
                (cx - r * a.cos(), y + s * 0.3 - r * a.sin() + r * 0.4)
            })
            .collect();
        let mut pts = vec![(cx - r, body_y)];
        pts.extend(arc);
        pts.push((cx + r, body_y));
        self.polyline(&pts, s * 0.13, rgb);
    }

    /// Disque du Live GND dans le carré (x, y, s), comme à l'écran du boîtier : un
    /// demi-disque noir du côté sombre, un blanc de l'autre, cerclés de clair.
    fn gnd_disc(&mut self, x: f32, y: f32, s: f32, (dx, dy): (f32, f32)) {
        let (cx, cy, r) = (x + s / 2.0, y + s / 2.0, s / 2.0);
        let inside = move |px: f32, py: f32| (px - cx).hypot(py - cy) <= r;
        self.shape(x, y, s, s, [245, 245, 245], 1.0, inside);
        self.shape(x, y, s, s, [15, 15, 15], 1.0, move |px, py| inside(px, py) && (px - cx) * dx + (py - cy) * dy > 0.0);
        self.shape(x, y, s, s, [245, 245, 245], 1.0, move |px, py| {
            let d = (px - cx).hypot(py - cy);
            d <= r && d >= r - 1.5
        });
    }

    /// Triangle d'avertissement (⚠) dans le carré (x, y, s).
    fn warning(&mut self, x: f32, y: f32, s: f32) {
        let pts = [(x + s / 2.0, y), (x + s, y + s * 0.92), (x, y + s * 0.92)];
        self.shape(x, y, s, s, WARNING, 1.0, |px, py| point_in_polygon((px, py), &pts));
        let w = (s * 0.14).max(1.5);
        self.rect(x + (s - w) / 2.0, y + s * 0.3, w, s * 0.34, [30, 30, 30], 1.0);
        self.rect(x + (s - w) / 2.0, y + s * 0.71, w, w, [30, 30, 30], 1.0);
    }

    /// Marquage d'une rafale dans sa ligne de la liste, aligné à droite sur
    /// `right` : étoiles puis ✓, ou ✕ ; renvoie l'abscisse de début.
    fn burst_badges(&mut self, right: f32, y: f32, m: &BurstMark) -> f32 {
        let s = 14.0;
        if m.rejected {
            let x = right - s;
            self.rect(x, y, s, s, REJECT_ON, 1.0);
            let (a, b) = (s * 0.28, s * 0.72);
            self.polyline(&[(x + a, y + a), (x + b, y + b)], 2.0, [255, 255, 255]);
            self.polyline(&[(x + b, y + a), (x + a, y + b)], 2.0, [255, 255, 255]);
            return x;
        }
        let mut x = right;
        if m.picked {
            x -= s;
            self.check(x, y, s);
            x -= 6.0;
        }
        for _ in 0..m.rating {
            x -= 13.0;
            self.star(x, y + 1.0, 12.0, STAR_ON);
        }
        x
    }

    /// Bouton d'action destructrice : contour et texte rouges ; renvoie sa largeur.
    fn danger_button(&mut self, font: &Font, x: f32, y: f32, label: &str) -> f32 {
        let w = button_width(font, label);
        for (rx, ry, rw, rh) in [(x, y, w, 1.0), (x, y + BUTTON_H - 1.0, w, 1.0), (x, y, 1.0, BUTTON_H), (x + w - 1.0, y, 1.0, BUTTON_H)] {
            self.rect(rx, ry, rw, rh, REJECT_ON, 0.8);
        }
        self.text(font, x + 10.0, y + 17.0, x + w, label, 15.0, [235, 120, 105]);
        w
    }

    /// Outil de la rangée du haut : sa lettre, puis son nom ; fond clair quand il
    /// est actif, contour sinon ; grisé quand il est sans effet. Renvoie sa largeur.
    fn tool(&mut self, font: &Font, x: f32, y: f32, t: &Tool) -> f32 {
        let key = t.key.to_string();
        let (kw, lw) = (text_width(font, &key, 15.0), text_width(font, t.label, 14.0));
        let w = (kw + lw + 22.0).round();
        let on = t.on == Some(true);
        if on {
            self.rect(x, y, w, BUTTON_H, [90, 160, 255], 0.35);
        } else {
            for (rx, ry, rw, rh) in [(x, y, w, 1.0), (x, y + BUTTON_H - 1.0, w, 1.0), (x, y, 1.0, BUTTON_H), (x + w - 1.0, y, 1.0, BUTTON_H)] {
                self.rect(rx, ry, rw, rh, [255, 255, 255], if t.enabled { 0.22 } else { 0.1 });
            }
        }
        let (kc, lc) = match (on, t.enabled) {
            (true, _) => ([255, 255, 255], [225, 235, 250]),
            (false, true) => ([235, 235, 235], [150, 150, 150]),
            (false, false) => ([95, 95, 95], [80, 80, 80]),
        };
        self.text(font, x + 8.0, y + 17.0, x + w, &key, 15.0, kc);
        self.text(font, x + 14.0 + kw, y + 17.0, x + w, t.label, 14.0, lc);
        w
    }

    /// Bouton texte ; renvoie sa largeur.
    fn button(&mut self, font: &Font, x: f32, y: f32, label: &str, on: bool) -> f32 {
        let w = button_width(font, label);
        if on {
            self.rect(x, y, w, BUTTON_H, [255, 255, 255], 0.16);
        } else {
            for (rx, ry, rw, rh) in [(x, y, w, 1.0), (x, y + BUTTON_H - 1.0, w, 1.0), (x, y, 1.0, BUTTON_H), (x + w - 1.0, y, 1.0, BUTTON_H)] {
                self.rect(rx, ry, rw, rh, [255, 255, 255], 0.22);
            }
        }
        let color = if on { [235, 235, 235] } else { [150, 150, 150] };
        self.text(font, x + 10.0, y + 17.0, x + w, label, 15.0, color);
        w
    }

    /// Marquage d'une vignette de la grille : étoiles ou rejet à gauche, couleur à droite.
    fn badges(&mut self, x: f32, y: f32, w: f32, cell: &Cell) {
        if cell.rejected {
            let s = 16.0;
            let (x, y) = (x + 3.0, y + 3.0);
            self.rect(x, y, s, s, REJECT_ON, 1.0);
            let (a, b) = (s * 0.28, s * 0.72);
            self.polyline(&[(x + a, y + a), (x + b, y + b)], 2.0, [255, 255, 255]);
            self.polyline(&[(x + b, y + a), (x + a, y + b)], 2.0, [255, 255, 255]);
        } else if cell.rating > 0 {
            let n = cell.rating as f32;
            self.rect(x + 3.0, y + 3.0, 4.0 + 12.0 * n, 16.0, [0, 0, 0], 0.55);
            for k in 0..cell.rating {
                self.star(x + 5.0 + 12.0 * k as f32, y + 5.0, 12.0, STAR_ON);
            }
        }
        if cell.picked && !cell.rejected {
            self.check(x + 3.0, y + 22.0, 16.0);
        }
        if let Some(c) = cell.color {
            self.rect(x + w - 17.0, y + 3.0, 14.0, 14.0, [0, 0, 0], 0.55);
            self.rect(x + w - 15.0, y + 5.0, 10.0, 10.0, c.rgb(), 1.0);
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
        self.draw_text(font, x, baseline, max_x, text, size, color, false);
    }

    /// Texte composé par-dessus un fond déjà peint (bandeau).
    #[allow(clippy::too_many_arguments)]
    fn text_on(&mut self, font: &Font, x: f32, baseline: f32, max_x: f32, text: &str, size: f32, color: [u8; 3]) {
        self.draw_text(font, x, baseline, max_x, text, size, color, true);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(&mut self, font: &Font, x: f32, baseline: f32, max_x: f32, text: &str, size: f32, color: [u8; 3], on_background: bool) {
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
                    if self.pixels[i + 3] > 0 && !on_background {
                        // Bord d'une lettre voisine : la plus couvrante l'emporte.
                        if a > self.pixels[i + 3] {
                            self.pixels[i..i + 3].copy_from_slice(&color);
                            self.pixels[i + 3] = a;
                        }
                    } else {
                        self.blend(px as usize, py as usize, color, a as f32 / 255.0);
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
    /// Lignes ou vignettes cliquables (`Widget::Photo`).
    pub hits: Vec<Hit>,
    /// Commandes du bas : filtre, vue, aide.
    pub bar_hits: Vec<Hit>,
    /// Emplacements possibles du repère de position de l'image courante.
    pub tracks: Vec<Track>,
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

    /// Rend la rangée de commandes de la photo, celle de sa rafale (si son
    /// marquage est possible), puis les lignes d'informations, en largeur `width`.
    pub fn render_info(&self, width: u32, info: &Info, lines: &[Line]) -> (Vec<u8>, u32, u32, Vec<Hit>) {
        let Some(font) = &self.font else {
            return (vec![0; 4], 1, 1, Vec::new());
        };
        let top = INFO_TOP + BUTTON_H + 10.0;
        let burst_y = top + CTRL + BURST_ROW_GAP;
        let mut y = if info.burst.is_some() { burst_y + CTRL } else { top + CTRL } + 6.0;
        let mut baselines = Vec::with_capacity(lines.len());
        for line in lines {
            let m = font.horizontal_line_metrics(line.size).unwrap();
            y += line.gap + m.ascent;
            baselines.push(y);
            y += -m.descent + m.line_gap;
        }
        // Double trait en bas : sépare les informations de la liste des rafales.
        let height = (y + 16.0).ceil() as u32;
        let mut canvas = Canvas::new(width, height);
        // Tout en haut : les outils, cliquables comme leurs touches.
        let mut hits = Vec::new();
        let mut x = PADDING;
        for t in info.tools {
            let w = canvas.tool(font, x, INFO_TOP, t);
            hits.push(Hit { widget: Widget::Tool(t.key.to_ascii_lowercase()), x, y: INFO_TOP, w, h: BUTTON_H });
            x += w + 6.0;
        }
        hits.extend(canvas.controls(PADDING, top, info.controls));
        let locked = info.locked;
        // À droite des commandes : l'état de la photo, puis le Live GND calé à droite.
        let mut mark_end = width as f32 - PADDING / 2.0;
        if let Some((label, dark)) = info.gnd {
            let x = width as f32 - PADDING - text_width(font, label, 16.0);
            canvas.text(font, x, top + 17.0, width as f32, label, 16.0, [235, 235, 235]);
            let disc = x - GND_DISC - 6.0;
            canvas.gnd_disc(disc, top + 1.0, GND_DISC, dark);
            mark_end = disc - 8.0;
        }
        let mark_x = hits.last().map_or(PADDING, |h| h.x + h.w) + 14.0;
        canvas.text(font, mark_x, top + 17.0, mark_end, info.mark.0, 16.0, info.mark.1);
        if let Some(c) = info.burst {
            // « Rafale » (cadenas bleu devant, si elle est bloquée) puis ses
            // étoiles ; ✓ et ✕ sous ceux de la photo.
            let mut x = PADDING;
            if locked {
                canvas.padlock(x, burst_y + 1.0, 18.0, LOCKED);
                x += 24.0;
            }
            let color = if locked { LOCKED } else { [150, 150, 150] };
            canvas.text(font, x, burst_y + 16.0, width as f32, "Rafale", 15.0, color);
            let label: f32 = "Rafale".chars().map(|ch| font.metrics(ch, 15.0).advance_width).sum();
            canvas.stars(x + label + 10.0, burst_y, c.rating, Widget::BurstStar, &mut hits);
            let check_x = hits.iter().find(|h| h.widget == Widget::Check).map_or(PADDING, |h| h.x);
            canvas.check_reject(check_x, burst_y, c, Widget::BurstCheck, Widget::BurstReject, &mut hits);
        }
        for dy in [7.0, 3.0] {
            canvas.rect(PADDING / 2.0, height as f32 - dy, width as f32 - PADDING, 1.0, [255, 255, 255], 0.22);
        }
        let max_x = width as f32 - PADDING / 2.0;
        for (line, &baseline) in lines.iter().zip(&baselines) {
            // Le texte de droite d'abord : celui de gauche s'arrête avant lui.
            let mut end = max_x;
            if !line.right.is_empty() {
                let x = width as f32 - PADDING - text_width(font, &line.right, line.size);
                canvas.text(font, x, baseline, width as f32, &line.right, line.size, line.right_color);
                end = x - 12.0;
            }
            match line.text.strip_prefix('⚠') {
                Some(rest) => {
                    let s = line.size;
                    canvas.warning(PADDING, baseline - s * 0.85, s);
                    canvas.text(font, PADDING + s + 4.0, baseline, end, rest.trim_start(), line.size, line.color);
                }
                None => canvas.text(font, PADDING, baseline, end, &line.text, line.size, line.color),
            }
        }
        (canvas.pixels, width, height, hits)
    }

    /// Hauteur d'une ligne de rafale pour une largeur de panneau donnée.
    pub fn row_height(width: u32) -> f32 {
        let (_, th) = thumb_size(width);
        ROW_HEADER + th as f32 + ROW_GAP
    }

    /// Hauteur d'une rangée de la grille (mode à plat).
    pub fn grid_row_height(width: u32) -> f32 {
        let (_, th) = thumb_size(width);
        th as f32 + THUMB_GAP + 8.0
    }

    /// Rend les lignes `entries` (déjà choisies pour tenir dans `height`), puis
    /// la barre de filtre et la ligne de boutons en bas.
    #[allow(clippy::too_many_arguments)]
    pub fn render_list(
        &mut self,
        width: u32,
        height: u32,
        filter: &Controls,
        burst_filter: Option<&Controls>,
        filter_text: &str,
        view: &View,
        entries: &[Entry],
        metas: &[Option<Meta>],
        status: &str,
    ) -> ListImage {
        let mut canvas = Canvas::new(width, height.max(1));
        let mut hits = Vec::new();
        let mut tracks = Vec::new();
        let bar = height as f32 - BOTTOM_BAR;
        let mut bar_hits = canvas.controls(PADDING, bar + 10.0, filter);
        let Some(font) = self.font.take() else {
            return ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, bar_hits, tracks };
        };
        let (tw, th) = thumb_size(width);
        let max_x = width as f32 - PADDING / 2.0;

        // À droite des commandes de filtre : nombre de photos et « effacer ».
        let after = bar_hits.last().map_or(PADDING, |h| h.x + h.w) + 16.0;
        let active = filter != &Controls::default() || burst_filter.is_some_and(|c| c != &Controls::default());
        let color = if active { [215, 215, 215] } else { [120, 120, 120] };
        canvas.text(&font, after, bar + 10.0 + 16.0, max_x, filter_text, 15.0, color);
        if active {
            let label = "effacer";
            let w = 56.0;
            let x = width as f32 - PADDING - w;
            // Sous le compte, à droite de la rangée des rafales s'il y en a une.
            let y = if burst_filter.is_some() { bar + FILTER_BAR } else { bar + 10.0 };
            canvas.text(&font, x, y + 16.0, max_x, label, 15.0, [150, 150, 150]);
            bar_hits.push(Hit { widget: Widget::ClearFilter, x, y: y - 4.0, w, h: CTRL + 8.0 });
        }
        canvas.rect(PADDING / 2.0, bar + 2.0, width as f32 - PADDING, 1.0, [255, 255, 255], 0.12);

        // Filtre des rafales (vue par rafales) : « Rafales », étoiles, puis ✓ sous celui des photos.
        if let Some(c) = burst_filter {
            let y = bar + FILTER_BAR;
            let on = c != &Controls::default();
            let color = if on { [215, 215, 215] } else { [120, 120, 120] };
            canvas.text(&font, PADDING, y + 16.0, max_x, "Rafales", 15.0, color);
            let label: f32 = "Rafales".chars().map(|ch| font.metrics(ch, 15.0).advance_width).sum();
            canvas.stars(PADDING + label + 10.0, y, c.rating, Widget::FilterBurstStar, &mut bar_hits);
            let check_x = bar_hits.iter().find(|h| h.widget == Widget::Check).map_or(PADDING, |h| h.x);
            canvas.check_box(check_x, y, c.check, Widget::FilterBurstCheck, &mut bar_hits);
        }

        // Fichiers copiés : JPEG, RAW, XMP, cochés indépendamment ; compte rendu à droite.
        let by = bar + FILTER_BAR + BURST_FILTER_BAR + 4.0;
        let mut x = PADDING;
        let f = view.copy_files;
        for (k, (label, on)) in [("JPEG", f.jpeg), ("RAW", f.raw), ("XMP", f.xmp)].into_iter().enumerate() {
            let w = canvas.button(&font, x, by, label, on);
            bar_hits.push(Hit { widget: Widget::CopyFile(k as u8), x, y: by, w, h: BUTTON_H });
            x += w;
        }
        canvas.text(&font, x + 12.0, by + 17.0, max_x, &view.copy_message, 15.0, [150, 150, 150]);

        // Sélection, copie, logiciel de retouche, retouche.
        let by = by + BUTTON_LINE;
        canvas.text(&font, PADDING, by + 17.0, max_x, &view.count_label, 15.0, [215, 215, 215]);
        let mut x = PADDING + text_width(&font, &view.count_label, 15.0) + 12.0;
        for (label, widget) in [("Copier", Widget::Copy), (view.editor_label.as_str(), Widget::Editor), ("Retoucher", Widget::Retouch)] {
            if widget == Widget::Editor {
                // Trait vertical : la copie d'un côté, la retouche de l'autre.
                x += 8.0;
                canvas.rect(x, by + 3.0, 1.0, BUTTON_H - 6.0, [255, 255, 255], 0.3);
                x += 15.0;
            }
            let w = canvas.button(&font, x, by, label, false);
            bar_hits.push(Hit { widget, x, y: by, w, h: BUTTON_H });
            x += w + 6.0;
        }

        // Ligne de boutons : vue à gauche, aide à droite.
        let by = by + BUTTON_LINE;
        if view.flat_available {
            let mut x = PADDING;
            for (label, widget, on) in [("Rafales", Widget::Bursts, !view.flat), ("À plat", Widget::Flat, view.flat)] {
                let w = canvas.button(&font, x, by, label, on);
                bar_hits.push(Hit { widget, x, y: by, w, h: BUTTON_H });
                x += w + 6.0;
            }
        }
        let mut left = PADDING;
        if view.flat_available {
            left = bar_hits.last().map_or(PADDING, |h| h.x + h.w) + 12.0;
        }
        if let Some(label) = &view.trash_label {
            let w = canvas.danger_button(&font, left, by, label);
            bar_hits.push(Hit { widget: Widget::Trash, x: left, y: by, w, h: BUTTON_H });
        }
        let mut x = width as f32 - PADDING;
        for (label, widget) in [("Raccourcis", Widget::Shortcuts), ("Principes", Widget::Principles)] {
            x -= button_width(&font, label);
            let w = canvas.button(&font, x, by, label, false);
            bar_hits.push(Hit { widget, x, y: by, w, h: BUTTON_H });
            x -= 6.0;
        }

        let mut y = 0.0;
        if view.locked {
            // Bandeau bleu, cadenas : on voit d'un coup d'œil qu'on est dans une rafale.
            canvas.rect(PADDING / 2.0, y, width as f32 - PADDING, ROW_HEADER - 2.0, LOCKED, 0.85);
            canvas.padlock(PADDING, y + 3.0, 17.0, [255, 255, 255]);
            canvas.text_on(&font, PADDING + 24.0, y + 17.0, max_x, status, 15.0, [255, 255, 255]);
            y += ROW_HEADER;
        } else if !status.is_empty() {
            canvas.text(&font, PADDING, y + 17.0, max_x, status, 15.0, [125, 125, 125]);
            y += ROW_HEADER;
        }
        for entry in entries {
            let top = y;
            match entry {
                Entry::Burst(row) => {
                    let row_h = Self::row_height(width);
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
                    // À droite : marquage de la rafale, puis nombre de photos retenues.
                    let count_x = width as f32 - PADDING - 40.0;
                    let mut badges = canvas.burst_badges(count_x - 10.0, top + 5.0, &row.mark);
                    if row.warn {
                        badges -= 20.0;
                        canvas.warning(badges, top + 4.0, 16.0);
                    }
                    canvas.text(&font, PADDING, top + 18.0, badges - 8.0, &header, 15.0, color);
                    if row.marked > 0 {
                        // Coche dessinée : nombre de photos retenues dans la rafale.
                        let x = count_x;
                        canvas.check(x, top + 5.0, 14.0);
                        canvas.text(&font, x + 18.0, top + 18.0, max_x, &row.marked.to_string(), 15.0, MARKED);
                    }
                    for (k, &index) in row.picks.iter().enumerate() {
                        let x = PADDING + k as f32 * (tw as f32 + THUMB_GAP);
                        self.draw_thumb(&mut canvas, index, x, top + ROW_HEADER, tw, th, metas);
                    }
                    // Le repère de position est dessiné à part (quad GPU) : la liste
                    // n'a pas à être recomposée à chaque image du défilé.
                    let span = width as f32 - 2.0 * PADDING;
                    tracks.push(Track { start: row.start, len: row.len, x: PADDING, y: top + ROW_HEADER + th as f32 + 3.0, span, cell: None });
                    hits.push(Hit { widget: Widget::Photo(row.first), x: 0.0, y: top, w: width as f32, h: row_h });
                    y += row_h;
                }
                Entry::Photos(cells) => {
                    for (k, cell) in cells.iter().enumerate() {
                        let x = PADDING + k as f32 * (tw as f32 + THUMB_GAP);
                        // Marge pour le cadre de la vignette active.
                        let top = top + 4.0;
                        self.draw_thumb(&mut canvas, cell.index, x, top, tw, th, metas);
                        canvas.badges(x, top, tw as f32, cell);
                        let cell_box = Some((x, top, tw as f32, th as f32));
                        tracks.push(Track { start: cell.index, len: cell.span, x, y: top + th as f32 + 3.0, span: tw as f32, cell: cell_box });
                        hits.push(Hit { widget: Widget::Photo(cell.index), x, y: top, w: tw as f32, h: th as f32 });
                    }
                    y += Self::grid_row_height(width);
                }
            }
        }
        self.font = Some(font);
        ListImage { pixels: canvas.pixels, width, height: height.max(1), hits, bar_hits, tracks }
    }

    /// Vignette centrée dans sa case (vignettes portrait plus étroites), ou case vide.
    #[allow(clippy::too_many_arguments)]
    fn draw_thumb(&mut self, canvas: &mut Canvas, index: usize, x: f32, y: f32, tw: u32, th: u32, metas: &[Option<Meta>]) {
        if let Some(t) = self.thumb(index, tw, th, metas) {
            let ox = (tw - t.width as u32) / 2;
            let oy = (th - t.height as u32) / 2;
            canvas.blit(x as usize + ox as usize, y as usize + oy as usize, &t);
        } else {
            canvas.fill(x, y, tw as f32, th as f32, [255, 255, 255, 12]);
        }
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

/// `n` images au plus, réparties de la première à la dernière.
pub fn spread(indices: &[usize], n: usize) -> Vec<usize> {
    let n = n.min(indices.len());
    (0..n)
        .map(|k| if n == 1 { indices[0] } else { indices[k * (indices.len() - 1) / (n - 1)] })
        .collect()
}

fn button_width(font: &Font, label: &str) -> f32 {
    (text_width(font, label, 15.0) + 20.0).round()
}

fn text_width(font: &Font, text: &str, size: f32) -> f32 {
    text.chars().map(|c| font.metrics(c, size).advance_width).sum()
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
