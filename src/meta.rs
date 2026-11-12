//! Analyse des en-têtes EXIF en arrière-plan : horodatage, numéro de vue dans
//! la rafale (MakerNote OM System / Olympus) et vignette JPEG intégrée.
//! Seul le début de chaque fichier est lu.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Sender;
use crate::loader::Waker;

/// Assez pour le segment APP1 (EXIF + vignette) des boîtiers OM System.
const HEAD: u64 = 64 * 1024;
const HEAD_RETRY: u64 = 512 * 1024;
/// Réveil de la boucle principale tous les N fichiers analysés.
const WAKE_EVERY: usize = 64;

pub struct Meta {
    /// Secondes depuis 1970 (heure locale de l'appareil), résolution 1 s.
    pub time: Option<i64>,
    /// Heure affichable, « HH:MM:SS ».
    pub clock: String,
    /// Mode d'entraînement et numéro de vue dans la rafale (0 = vue unique).
    pub drive: Option<(u16, u16)>,
    /// « 1/500 s   f/4   ISO 12800 ».
    pub exposure: String,
    /// Mode image (PictureMode OM System / Olympus).
    pub picture_mode: Option<String>,
    /// Filtre de bruit (NoiseFilter OM System / Olympus).
    pub noise_filter: Option<&'static str>,
    /// Luminosité (Gradation OM System / Olympus) : « luminosité normale », etc.
    pub gradation: Option<String>,
    /// Vignette EXIF, encore compressée en JPEG.
    pub thumb: Option<Vec<u8>>,
    pub orientation: u8,
    /// Zone de mise au point (OM System / Olympus) : x, y, largeur, hauteur en
    /// proportions de l'image du capteur, avant orientation.
    pub af: Option<[f32; 4]>,
    /// Filtre Live GND du boîtier (OM System).
    pub gnd: Option<Gnd>,
}

/// Réglages du Live GND (dégradé calculé par le boîtier), tels qu'à la prise de vue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gnd {
    /// Densité : 2, 4 ou 8 (1, 2 ou 3 EV).
    pub density: u8,
    /// Transition : 'S' douce, 'M' moyenne, 'H' dure.
    pub transition: char,
    /// Angle en degrés, repère du capteur : 0 = sombre en haut, 180 = en bas.
    pub angle: u16,
    /// Point de passage de la ligne, de -60 à 60 en soixantièmes de demi-largeur
    /// et de demi-hauteur depuis le centre, repère du capteur.
    pub px: i16,
    pub py: i16,
}

/// Le Live GND sur la photo telle qu'affichée (orientation appliquée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GndView {
    /// Extrémités de la ligne de transition, en proportions de la photo.
    pub line: [(f32, f32); 2],
    /// Direction du côté sombre, vecteur unitaire (x vers la droite, y vers le bas).
    pub dark: (f32, f32),
}

impl Gnd {
    /// Libellé du boîtier : « GND4-S ».
    pub fn label(&self) -> String {
        format!("GND{}-{}", self.density, self.transition)
    }

    /// Géométrie affichée. Le capteur est supposé en 4:3 : la photo recadrée en
    /// un autre format aurait sa ligne légèrement décalée.
    pub fn view(&self, orientation: u8) -> GndView {
        const SENSOR: (f32, f32) = (4.0, 3.0);
        let (sin, cos) = (self.angle as f32).to_radians().sin_cos();
        let center = (0.5 + self.px as f32 / 120.0, 0.5 + self.py as f32 / 120.0);
        // La ligne, en proportions, découpée sur les bords de la photo.
        let along = (cos / SENSOR.0, sin / SENSOR.1);
        let (mut from, mut to) = (f32::MIN, f32::MAX);
        for (c, d) in [(center.0, along.0), (center.1, along.1)] {
            if d.abs() > 1e-6 {
                let (a, b) = (-c / d, (1.0 - c) / d);
                from = from.max(a.min(b));
                to = to.min(a.max(b));
            }
        }
        let point = |t: f32| orient_point(center.0 + t * along.0, center.1 + t * along.1, orientation);
        // Côté sombre : petit pas perpendiculaire à la ligne, mesuré sur la photo affichée.
        let (w, h) = if orientation >= 5 { (SENSOR.1, SENSOR.0) } else { SENSOR };
        let (x0, y0) = orient_point(center.0, center.1, orientation);
        let (x1, y1) = orient_point(center.0 + sin / SENSOR.0, center.1 - cos / SENSOR.1, orientation);
        let (dx, dy) = ((x1 - x0) * w, (y1 - y0) * h);
        let n = dx.hypot(dy).max(1e-6);
        GndView { line: [point(from), point(to)], dark: (dx / n, dy / n) }
    }
}

impl Meta {
    /// Zone AF en proportions de la photo telle qu'affichée (orientation appliquée).
    pub fn af_display(&self) -> Option<[f32; 4]> {
        self.af.map(|r| orient_rect(r, self.orientation))
    }
}

/// Rectangle en proportions du capteur → proportions de l'image orientée
/// (même correspondance que le shader et `panel::orient`).
fn orient_rect([x, y, w, h]: [f32; 4], orientation: u8) -> [f32; 4] {
    let (a, b) = (orient_point(x, y, orientation), orient_point(x + w, y + h, orientation));
    [a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs()]
}

/// Point en proportions du capteur → proportions de l'image orientée.
fn orient_point(u: f32, v: f32, orientation: u8) -> (f32, f32) {
    match orientation {
        2 => (1.0 - u, v),
        3 => (1.0 - u, 1.0 - v),
        4 => (u, 1.0 - v),
        5 => (v, u),
        6 => (1.0 - v, u),
        7 => (1.0 - v, 1.0 - u),
        8 => (v, 1.0 - u),
        _ => (u, v),
    }
}

/// `next` prolonge-t-elle la rafale de `prev` ?
pub fn same_burst(prev: &Meta, next: &Meta) -> bool {
    match (prev.drive, next.drive) {
        (Some((pm, ps)), Some((nm, ns))) => pm != 0 && nm != 0 && ns == ps + 1,
        // Sans numéro de vue : horodatage à la seconde, une seconde d'écart au plus.
        _ => matches!((prev.time, next.time), (Some(p), Some(n)) if (0..=1).contains(&(n - p))),
    }
}

pub fn spawn(files: Arc<Vec<PathBuf>>, tx: Sender<(usize, Meta)>, wake: Waker) {
    std::thread::Builder::new()
        .name("scan".into())
        .spawn(move || {
            let t0 = std::time::Instant::now();
            for (i, path) in files.iter().enumerate() {
                let meta = read_meta(path).unwrap_or_else(|| {
                    log::warn!("{} : EXIF illisible", path.display());
                    Meta {
                        time: None,
                        clock: String::new(),
                        drive: None,
                        exposure: String::new(),
                        picture_mode: None,
                        noise_filter: None,
                        gradation: None,
                        thumb: None,
                        orientation: 1,
                        af: None,
                        gnd: None,
                    }
                });
                if tx.send((i, meta)).is_err() {
                    return;
                }
                if i % WAKE_EVERY == WAKE_EVERY - 1 {
                    wake();
                }
            }
            wake();
            log::info!(
                "{} en-têtes analysés en {:.0} ms",
                files.len(),
                t0.elapsed().as_secs_f64() * 1000.0
            );
        })
        .expect("création du thread d'analyse");
}

fn read_meta(path: &std::path::Path) -> Option<Meta> {
    let exif = read_exif(path, HEAD).or_else(|| read_exif(path, HEAD_RETRY))?;
    let (time, clock) = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
        .and_then(|f| match &f.value {
            exif::Value::Ascii(v) => v.first(),
            _ => None,
        })
        .and_then(|bytes| exif::DateTime::from_ascii(bytes).ok())
        .map(|d| {
            let days = days_from_civil(d.year as i64, d.month as i64, d.day as i64);
            let secs = d.hour as i64 * 3600 + d.minute as i64 * 60 + d.second as i64;
            (
                Some(days * 86400 + secs),
                format!("{:02}:{:02}:{:02}", d.hour, d.minute, d.second),
            )
        })
        .unwrap_or((None, String::new()));
    let olympus = exif
        .get_field(exif::Tag::MakerNote, exif::In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Undefined(mn, _) => olympus_settings(mn),
            _ => None,
        })
        .unwrap_or_default();
    let exposure = exposure(&exif);
    let thumb = (|| {
        let off = exif
            .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?
            .value
            .get_uint(0)? as usize;
        let len = exif
            .get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?
            .value
            .get_uint(0)? as usize;
        exif.buf().get(off..off + len).map(<[u8]>::to_vec)
    })();
    let orientation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);
    Some(Meta {
        time,
        clock,
        drive: olympus.drive,
        exposure,
        picture_mode: olympus.picture_mode,
        noise_filter: olympus.noise_filter,
        gradation: olympus.gradation,
        thumb,
        orientation,
        af: olympus.af,
        gnd: olympus.gnd,
    })
}

fn read_exif(path: &std::path::Path, limit: u64) -> Option<exif::Exif> {
    let mut head = Vec::with_capacity(limit as usize);
    std::fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut head)
        .ok()?;
    exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(&head))
        .ok()
}

/// Jour de prise de vue, « AAAA-MM-JJ » (heure de l'appareil), d'un JPEG ou d'un
/// RAW. Les RAW sont des TIFF (parfois avec une autre signature, comme « IIRO »
/// pour les ORF) : leur date est lue directement dans les balises, la lecture
/// EXIF complète refusant ces fichiers tronqués à 64 Ko.
///
/// Renvoie aussi l'instant, en secondes depuis 1970 comme si l'heure de
/// l'appareil était en UTC (le fuseau n'est pas connu ici).
pub fn capture_time(path: &std::path::Path) -> Option<(String, i64)> {
    let mut head = Vec::with_capacity(HEAD as usize);
    std::fs::File::open(path).ok()?.take(HEAD).read_to_end(&mut head).ok()?;
    let text = if head.starts_with(b"II") || head.starts_with(b"MM") {
        tiff_date(&head)?
    } else {
        let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&head)).ok()?;
        let field = exif
            .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
            .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;
        let exif::Value::Ascii(v) = &field.value else { return None };
        v.first()?.clone()
    };
    let d = exif::DateTime::from_ascii(&text).ok()?;
    let naive = days_from_civil(d.year as i64, d.month as i64, d.day as i64) * 86400
        + d.hour as i64 * 3600
        + d.minute as i64 * 60
        + d.second as i64;
    (d.year > 1990).then(|| (format!("{:04}-{:02}-{:02}", d.year, d.month, d.day), naive))
}

/// DateTimeOriginal (IFD EXIF), sinon DateTime (IFD0), d'un en-tête TIFF.
fn tiff_date(d: &[u8]) -> Option<Vec<u8>> {
    let le = d[0] == b'I';
    let u16_at = |o: usize| {
        d.get(o..o + 2).map(|b| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_at = |o: usize| {
        d.get(o..o + 4).map(|b| {
            let v = if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) };
            v as usize
        })
    };
    let entry = |ifd: usize, tag: u16| (0..u16_at(ifd)? as usize).map(|k| ifd + 2 + 12 * k).find(|&e| u16_at(e) == Some(tag));
    let ascii = |e: usize| {
        let n = u32_at(e + 4)?;
        let at = if n <= 4 { e + 8 } else { u32_at(e + 8)? };
        let v = d.get(at..at + n)?;
        Some(v.split(|&c| c == 0).next()?.to_vec())
    };
    let ifd0 = u32_at(4)?;
    let original = entry(ifd0, 0x8769)
        .and_then(|e| u32_at(e + 8))
        .and_then(|exif| entry(exif, 0x9003))
        .and_then(ascii);
    original.or_else(|| entry(ifd0, 0x0132).and_then(ascii))
}

/// « 1/500 s   f/4   ISO 12800 » (les éléments absents sont omis).
fn exposure(exif: &exif::Exif) -> String {
    let rational = |tag| match exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(ref v) if !v.is_empty() && v[0].denom != 0 => Some(v[0].to_f64()),
        _ => None,
    };
    let mut parts = Vec::new();
    if let Some(t) = rational(exif::Tag::ExposureTime).filter(|&t| t > 0.0) {
        parts.push(if t < 0.3 {
            format!("1/{} s", (1.0 / t).round())
        } else {
            format!("{} s", (t * 10.0).round() / 10.0).replace('.', ",")
        });
    }
    if let Some(f) = rational(exif::Tag::FNumber).filter(|&f| f > 0.0) {
        parts.push(format!("f/{}", (f * 10.0).round() / 10.0).replace('.', ","));
    }
    if let Some(iso) = exif
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
    {
        parts.push(format!("ISO {iso}"));
    }
    parts.join("   ")
}

#[derive(Default)]
struct Olympus {
    drive: Option<(u16, u16)>,
    picture_mode: Option<String>,
    noise_filter: Option<&'static str>,
    gradation: Option<String>,
    af: Option<[f32; 4]>,
    gnd: Option<Gnd>,
}

/// Réglages lus dans CameraSettings (balise 0x2020) de la MakerNote
/// « OM SYSTEM » (en-tête de 16 octets) ou « OLYMPUS » (12), little-endian,
/// décalages relatifs au début de la MakerNote. Décodage d'après exiftool.
fn olympus_settings(mn: &[u8]) -> Option<Olympus> {
    let start = if mn.starts_with(b"OM SYSTEM\0") {
        16
    } else if mn.starts_with(b"OLYMPUS\0II") {
        12
    } else {
        return None;
    };
    let u16_at = |o: usize| mn.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| {
        mn.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let find = |ifd: usize, tag: u16| {
        (0..u16_at(ifd)? as usize)
            .map(|i| ifd + 2 + 12 * i)
            .find(|&e| u16_at(e) == Some(tag))
    };
    let settings = u32_at(find(start, 0x2020)? + 8)?;
    // Valeurs 16 bits (types 3 et 8) d'une entrée de CameraSettings.
    let values = |tag: u16| -> Option<Vec<i32>> {
        let entry = find(settings, tag)?;
        let ty = u16_at(entry + 2)?;
        let count = u32_at(entry + 4)?;
        if ty != 3 && ty != 8 {
            return None;
        }
        let base = if count * 2 <= 4 { entry + 8 } else { u32_at(entry + 8)? };
        (0..count)
            .map(|k| {
                let v = u16_at(base + 2 * k)?;
                Some(if ty == 8 { v as i16 as i32 } else { v as i32 })
            })
            .collect()
    };

    // DriveMode 0x0600 : [mode, numéro de vue, …], mode 0 = vue unique.
    let drive = values(0x0600)
        .filter(|v| v.len() >= 2)
        .map(|v| (v[0] as u16, v[1] as u16));
    let picture_mode = values(0x0520).and_then(|v| {
        Some(
            match *v.first()? {
                1 => "Vivid",
                2 => "Natural",
                3 => "Muted",
                4 => "Portrait",
                5 => "i-Enhance",
                6 => "e-Portrait",
                7 => "Color Creator",
                8 => "Underwater",
                9 => "Color Profile 1",
                10 => "Color Profile 2",
                11 => "Color Profile 3",
                12 => "Monochrome Profile 1",
                13 => "Monochrome Profile 2",
                14 => "Monochrome Profile 3",
                17 => "Art Mode",
                18 => "Monochrome Profile 4",
                256 => "Monotone",
                512 => "Sepia",
                n => return Some(format!("mode image {n}")),
            }
            .to_string(),
        )
    });
    // NoiseFilter 0x0527 : [valeur, min, max].
    // Gradation 0x050f (« Luminosité » dans le menu) : [valeur, min, max, …],
    // -1 basse, 0 normale, 1 haute (relevé sur OM-1 Mk II). Le mode Auto n'a pas
    // encore été vu : une autre valeur est affichée telle quelle.
    let gradation = values(0x050f).and_then(|v| {
        Some(match *v.first()? {
            -1 => "luminosité basse".to_string(),
            0 => "luminosité normale".to_string(),
            1 => "luminosité haute".to_string(),
            n => format!("luminosité {n}"),
        })
    });
    let noise_filter = values(0x0527).and_then(|v| match v.as_slice() {
        [-2, -2, 1] => Some("coupé"),
        [-1, -2, 1] => Some("faible"),
        [0, -2, 1] => Some("standard"),
        [1, -2, 1] => Some("fort"),
        _ => None,
    });
    // Zone AF 0x030a : [largeur, hauteur du repère (640 × 480), x, y, l, h, …],
    // relevée sur OM-1 Mk II ; elle suit le sujet (visage détecté) vue par vue.
    let af = values(0x030a).and_then(|v| {
        let [fw, fh, x, y, w, h, ..] = v[..] else { return None };
        (fw > 0 && fh > 0 && w > 0 && h > 0 && x + w <= fw && y + h <= fh).then(|| {
            let (fw, fh) = (fw as f32, fh as f32);
            [x as f32 / fw, y as f32 / fh, w as f32 / fw, h as f32 / fh]
        })
    });
    // Live GND, relevé sur OM-1 Mk II. Mode empilé 0x0804 (deux valeurs 32 bits) :
    // [13, densité]. Paramètres dans ImageProcessing (0x2040), tag 0x1700, valeurs
    // 16 bits : [0] transition, [6] et [7] position, [8] angle.
    let gnd = (|| {
        let stacked = find(settings, 0x0804)?;
        if u16_at(stacked + 2)? != 4 || u32_at(stacked + 4)? != 2 {
            return None;
        }
        let base = u32_at(stacked + 8)?;
        if u32_at(base)? != 13 {
            return None;
        }
        let density = u32_at(base + 4)?;
        if ![2, 4, 8].contains(&density) {
            return None;
        }
        let processing = u32_at(find(start, 0x2040)? + 8)?;
        let params = u32_at(find(processing, 0x1700)? + 8)?;
        let at = |k: usize| u16_at(params + 2 * k);
        let transition = match at(0)? {
            6553 => 'S',
            3276 => 'M',
            1638 => 'H',
            _ => return None,
        };
        Some(Gnd {
            density: density as u8,
            transition,
            angle: at(8)?,
            px: at(6)? as i16,
            py: at(7)? as i16,
        })
    })();
    Some(Olympus {
        drive,
        picture_mode,
        noise_filter,
        gradation,
        af,
        gnd,
    })
}

/// Nombre de jours depuis le 1970-01-01 (algorithme de H. Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::{Gnd, olympus_settings, orient_rect};

    fn gnd(angle: u16, px: i16, py: i16) -> Gnd {
        Gnd { density: 4, transition: 'S', angle, px, py }
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn gnd_geometrie() {
        // Reset du boîtier (PB070100) : sombre en haut, ligne horizontale au milieu.
        let v = gnd(0, 0, 0).view(1);
        assert!(close(v.dark, (0.0, -1.0)));
        assert!(close(v.line[0], (0.0, 0.5)) && close(v.line[1], (1.0, 0.5)));
        // Sombre en bas, ligne tout en bas (PB070087, position 60).
        let v = gnd(180, 3, 60).view(1);
        assert!(close(v.dark, (0.0, 1.0)));
        assert!((v.line[0].1 - 1.0).abs() < 1e-3 && (v.line[1].1 - 1.0).abs() < 1e-3);
        // PB070091 : 345°, la ligne monte vers la droite, sombre en haut à gauche.
        let v = gnd(345, 0, -36).view(1);
        assert!(v.dark.0 < 0.0 && v.dark.1 < 0.0);
        let (l, r) = if v.line[0].0 < v.line[1].0 { (v.line[0], v.line[1]) } else { (v.line[1], v.line[0]) };
        assert!(r.1 < l.1);
        // PB070092 : 150°, sombre en bas à droite.
        let v = gnd(150, 0, 20).view(1);
        assert!(v.dark.0 > 0.0 && v.dark.1 > 0.0);
        // PB070009 : 270° sur une photo en portrait (orientation 8), sombre en bas.
        let v = gnd(270, -12, 4).view(8);
        assert!(close(v.dark, (0.0, 1.0)));
    }

    #[test]
    fn gnd_lu_dans_la_makernote() {
        // MakerNote minimale : racine à 16, CameraSettings à 60, ImageProcessing à 100.
        let mut mn = vec![0u8; 300];
        mn[..10].copy_from_slice(b"OM SYSTEM\0");
        let mut u16_put = |o: usize, v: u16| mn[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let entry = |mn: &mut Vec<u8>, o: usize, tag: u16, ty: u16, n: u32, v: u32| {
            mn[o..o + 2].copy_from_slice(&tag.to_le_bytes());
            mn[o + 2..o + 4].copy_from_slice(&ty.to_le_bytes());
            mn[o + 4..o + 8].copy_from_slice(&n.to_le_bytes());
            mn[o + 8..o + 12].copy_from_slice(&v.to_le_bytes());
        };
        u16_put(16, 2);
        u16_put(60, 1);
        u16_put(100, 1);
        entry(&mut mn, 18, 0x2020, 13, 1, 60);
        entry(&mut mn, 30, 0x2040, 13, 1, 100);
        entry(&mut mn, 62, 0x0804, 4, 2, 140);
        entry(&mut mn, 102, 0x1700, 7, 976, 160);
        mn[140..144].copy_from_slice(&13u32.to_le_bytes());
        mn[144..148].copy_from_slice(&8u32.to_le_bytes());
        mn[160..162].copy_from_slice(&3276u16.to_le_bytes());
        mn[172..174].copy_from_slice(&(-36i16).to_le_bytes());
        mn[174..176].copy_from_slice(&20i16.to_le_bytes());
        mn[176..178].copy_from_slice(&345u16.to_le_bytes());
        let g = olympus_settings(&mn).unwrap().gnd.unwrap();
        assert_eq!(g, Gnd { density: 8, transition: 'M', angle: 345, px: -36, py: 20 });
        assert_eq!(g.label(), "GND8-M");
        // Pas de GND : mode empilé 0 0 (ou HDR).
        mn[140..144].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(olympus_settings(&mn).unwrap().gnd, None);
    }

    #[test]
    fn zone_af_orientee() {
        let r = [0.5, 0.25, 0.1, 0.2];
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(close(orient_rect(r, 1), r));
        // Rotation de 90° (6) : le haut du capteur passe à droite.
        let [x, y, w, h] = orient_rect(r, 6);
        assert!((x - 0.55).abs() < 1e-6 && (y - 0.5).abs() < 1e-6 && (w - 0.2).abs() < 1e-6 && (h - 0.1).abs() < 1e-6);
        // Rotation de 270° (8) : le haut du capteur passe à gauche.
        let [x, y, ..] = orient_rect(r, 8);
        assert!((x - 0.25).abs() < 1e-6 && (y - 0.4).abs() < 1e-6);
    }
}
