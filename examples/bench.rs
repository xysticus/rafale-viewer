//! Mesure lecture disque et décodage sur un dossier réel.
//!
//! cargo run --release --example bench -- <dossier> [début] [nombre]
//! Choisir une plage jamais ouverte depuis le démarrage pour mesurer « hors cache ».

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

fn decode(dec: &mut Decompressor, data: &[u8], denom: usize) -> (usize, usize) {
    let header = dec.read_header(data).unwrap();
    let f = ScalingFactor::new(1, denom);
    dec.set_scaling_factor(f).unwrap();
    let s = header.scaled(f);
    let mut img = Image {
        pixels: vec![0u8; 4 * s.width * s.height],
        width: s.width,
        pitch: 4 * s.width,
        height: s.height,
        format: PixelFormat::RGBA,
    };
    dec.decompress(data, img.as_deref_mut()).unwrap();
    (s.width, s.height)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("dossier"));
    let start: usize = args.next().map_or(0, |s| s.parse().unwrap());
    let count: usize = args.next().map_or(40, |s| s.parse().unwrap());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
        })
        .collect();
    files.sort();
    let files = &files[start..(start + count).min(files.len())];
    let third = files.len() / 3;

    // 1. Décodage seul (données en mémoire), un thread.
    let data = std::fs::read(&files[0]).unwrap();
    let mut dec = Decompressor::new().unwrap();
    for denom in [8, 4, 2, 1] {
        decode(&mut dec, &data, denom);
        let t = Instant::now();
        let mut dims = (0, 0);
        for _ in 0..5 {
            dims = decode(&mut dec, &data, denom);
        }
        println!(
            "décodage 1/{denom} ({}x{}) : {:.1} ms",
            dims.0,
            dims.1,
            ms(t) / 5.0
        );
    }

    // 2. Lecture séquentielle à froid, un fichier après l'autre.
    let t = Instant::now();
    let mut bytes = 0;
    for f in &files[..third] {
        bytes += std::fs::read(f).unwrap().len();
    }
    let el = ms(t);
    println!(
        "lecture séquentielle : {} fichiers, {:.1} ms/fichier, {:.0} Mo/s",
        third,
        el / third as f64,
        bytes as f64 / 1e6 / (el / 1000.0)
    );

    // 3. Lecture + décodage 1/4 en parallèle, à froid (autre moitié de la plage).
    let threads = std::thread::available_parallelism().unwrap().get() - 1;
    let rest = &files[third..2 * third];
    let next = AtomicUsize::new(0);
    let t = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let mut dec = Decompressor::new().unwrap();
                while let Some(f) = rest.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let data = std::fs::read(f).unwrap();
                    decode(&mut dec, &data, 4);
                }
            });
        }
    });
    let el = ms(t);
    println!(
        "lecture + 1/4 sur {threads} threads : {} fichiers, {:.0} images/s",
        rest.len(),
        rest.len() as f64 / (el / 1000.0)
    );

    // 4. Idem, mais un seul thread lit le disque (séquentiellement) et alimente les décodeurs.
    let rest = &files[2 * third..];
    let (tx, rx) = crossbeam_channel::bounded::<Vec<u8>>(threads);
    let t = Instant::now();
    std::thread::scope(|s| {
        s.spawn(move || {
            for f in rest {
                tx.send(std::fs::read(f).unwrap()).unwrap();
            }
        });
        for _ in 0..threads {
            let rx = rx.clone();
            s.spawn(move || {
                let mut dec = Decompressor::new().unwrap();
                while let Ok(data) = rx.recv() {
                    decode(&mut dec, &data, 4);
                }
            });
        }
    });
    let el = ms(t);
    println!(
        "1 lecteur + {threads} décodeurs : {} fichiers, {:.0} images/s",
        rest.len(),
        rest.len() as f64 / (el / 1000.0)
    );
}
