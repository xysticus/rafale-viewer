//! Analyse des en-têtes EXIF en arrière-plan : horodatage, numéro de vue dans
//! la rafale (MakerNote OM System / Olympus) et vignette JPEG intégrée.
//! Seul le début de chaque fichier est lu.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Sender;
use winit::event_loop::EventLoopProxy;

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
    /// Vignette EXIF, encore compressée en JPEG.
    pub thumb: Option<Vec<u8>>,
    pub orientation: u8,
}

/// `next` prolonge-t-elle la rafale de `prev` ?
pub fn same_burst(prev: &Meta, next: &Meta) -> bool {
    match (prev.drive, next.drive) {
        (Some((pm, ps)), Some((nm, ns))) => pm != 0 && nm != 0 && ns == ps + 1,
        // Sans numéro de vue : horodatage à la seconde, une seconde d'écart au plus.
        _ => matches!((prev.time, next.time), (Some(p), Some(n)) if (0..=1).contains(&(n - p))),
    }
}

pub fn spawn(files: Arc<Vec<PathBuf>>, tx: Sender<(usize, Meta)>, proxy: EventLoopProxy<()>) {
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
                        thumb: None,
                        orientation: 1,
                    }
                });
                if tx.send((i, meta)).is_err() {
                    return;
                }
                if i % WAKE_EVERY == WAKE_EVERY - 1 {
                    let _ = proxy.send_event(());
                }
            }
            let _ = proxy.send_event(());
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
        thumb,
        orientation,
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
    let noise_filter = values(0x0527).and_then(|v| match v.as_slice() {
        [-2, -2, 1] => Some("coupé"),
        [-1, -2, 1] => Some("faible"),
        [0, -2, 1] => Some("standard"),
        [1, -2, 1] => Some("fort"),
        _ => None,
    });
    Some(Olympus {
        drive,
        picture_mode,
        noise_filter,
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
