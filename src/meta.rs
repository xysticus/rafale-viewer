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
    let drive = exif
        .get_field(exif::Tag::MakerNote, exif::In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Undefined(mn, _) => olympus_drive_mode(mn),
            _ => None,
        });
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
        drive,
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

/// DriveMode (balise 0x0600 de CameraSettings 0x2020) : [mode, numéro de vue, …].
/// MakerNote « OM SYSTEM » (en-tête de 16 octets) ou « OLYMPUS » (12),
/// little-endian, décalages relatifs au début de la MakerNote.
fn olympus_drive_mode(mn: &[u8]) -> Option<(u16, u16)> {
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
    let entry = find(settings, 0x0600)?;
    let count = u32_at(entry + 4)?;
    if u16_at(entry + 2)? != 3 || count < 2 {
        return None; // pas un tableau d'entiers 16 bits
    }
    let values = if count * 2 <= 4 {
        entry + 8
    } else {
        u32_at(entry + 8)?
    };
    Some((u16_at(values)?, u16_at(values + 2)?))
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
