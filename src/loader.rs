//! Pool de décodage JPEG.
//!
//! Un seul thread lit le disque, dans l'ordre de priorité : les lectures
//! concurrentes divisent le débit par deux (mesuré sur NTFS/FUSE). Les octets
//! bruts restent en cache, pour que la pleine résolution ne relise pas le
//! fichier. Les threads de décodage consomment ces octets.
//!
//! Le thread principal remplace à chaque mise à jour la liste complète des
//! tâches voulues, par ordre de priorité : tout ce qui n'y figure plus est
//! de fait annulé (une tâche déjà commencée va jusqu'au bout).

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use lru::LruCache;
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};
use winit::event_loop::EventLoopProxy;

/// Facteur de réduction utilisé pendant le défilé (DCT scaling).
const REDUCED_SCALE: (usize, usize) = (1, 4);
/// Fichiers bruts gardés en mémoire (~15 Mo chacun).
const RAW_CACHE: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    Reduced,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Job {
    pub index: usize,
    pub quality: Quality,
}

/// Image décodée en RGBA, non tournée : l'orientation EXIF est appliquée au rendu.
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Dimensions de l'image pleine résolution (pour un cadrage identique réduit/plein).
    pub full_width: u32,
    pub full_height: u32,
    /// Orientation EXIF (1 à 8).
    pub orientation: u8,
}

impl Decoded {
    /// Image de remplacement pour un fichier illisible : le défilé ne doit pas bloquer dessus.
    fn placeholder() -> Self {
        Decoded {
            width: 1,
            height: 1,
            pixels: vec![60, 0, 0, 255],
            full_width: 3,
            full_height: 2,
            orientation: 1,
        }
    }
}

pub struct Done {
    pub job: Job,
    pub image: Arc<Decoded>,
}

type Raw = Result<Arc<Vec<u8>>, Arc<std::io::Error>>;

struct State {
    /// Tâches voulues, par priorité décroissante.
    queue: Vec<Job>,
    raw: LruCache<usize, Raw>,
    in_flight: HashSet<Job>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

pub struct Loader {
    shared: Arc<Shared>,
    results: Receiver<Done>,
    threads: Vec<JoinHandle<()>>,
}

impl Loader {
    pub fn new(files: Arc<Vec<PathBuf>>, proxy: EventLoopProxy<()>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: Vec::new(),
                raw: LruCache::new(NonZeroUsize::new(RAW_CACHE).unwrap()),
                in_flight: HashSet::new(),
                shutdown: false,
            }),
            cv: Condvar::new(),
        });
        let (tx, results) = crossbeam_channel::unbounded();
        let n = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(4)
            .max(2);
        let mut threads: Vec<_> = (0..n)
            .map(|i| {
                let shared = shared.clone();
                let tx = tx.clone();
                let proxy = proxy.clone();
                std::thread::Builder::new()
                    .name(format!("decode-{i}"))
                    .spawn(move || decoder(&shared, &tx, &proxy))
                    .expect("création du thread de décodage")
            })
            .collect();
        let reader_shared = shared.clone();
        threads.push(
            std::thread::Builder::new()
                .name("reader".into())
                .spawn(move || reader(&reader_shared, &files))
                .expect("création du thread de lecture"),
        );
        log::info!("{n} threads de décodage");
        Loader {
            shared,
            results,
            threads,
        }
    }

    /// Remplace la file d'attente (les tâches absentes de `jobs` sont abandonnées).
    pub fn set_wanted(&self, jobs: impl IntoIterator<Item = Job>) {
        let mut st = self.shared.state.lock().unwrap();
        let State {
            queue, in_flight, ..
        } = &mut *st;
        queue.clear();
        queue.extend(jobs.into_iter().filter(|j| !in_flight.contains(j)));
        let wake = !st.queue.is_empty();
        drop(st);
        if wake {
            self.shared.cv.notify_all();
        }
    }

    pub fn try_recv(&self) -> Option<Done> {
        self.results.try_recv().ok()
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().shutdown = true;
        self.shared.cv.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn reader(shared: &Shared, files: &[PathBuf]) {
    loop {
        let index = {
            let mut st = shared.state.lock().unwrap();
            loop {
                if st.shutdown {
                    return;
                }
                let next = st
                    .queue
                    .iter()
                    .map(|j| j.index)
                    .find(|i| !st.raw.contains(i));
                if let Some(i) = next {
                    break i;
                }
                st = shared.cv.wait(st).unwrap();
            }
        };
        let raw = std::fs::read(&files[index])
            .map(Arc::new)
            .map_err(|e| {
                log::warn!("{}: {e}", files[index].display());
                Arc::new(e)
            });
        let mut st = shared.state.lock().unwrap();
        st.raw.put(index, raw);
        drop(st);
        shared.cv.notify_all();
    }
}

fn decoder(shared: &Shared, tx: &Sender<Done>, proxy: &EventLoopProxy<()>) {
    let mut dec = Decompressor::new().expect("initialisation de turbojpeg");
    loop {
        let (job, raw) = {
            let mut st = shared.state.lock().unwrap();
            loop {
                if st.shutdown {
                    return;
                }
                let State {
                    queue, raw, ..
                } = &mut *st;
                if let Some(pos) = queue.iter().position(|j| raw.contains(&j.index)) {
                    let job = queue.remove(pos);
                    let raw = st.raw.get(&job.index).unwrap().clone();
                    st.in_flight.insert(job);
                    break (job, raw);
                }
                st = shared.cv.wait(st).unwrap();
            }
        };

        let t0 = std::time::Instant::now();
        let image = match raw
            .map_err(|e| e.to_string().into())
            .and_then(|data| decode(&mut dec, &data, job.quality))
        {
            Ok(img) => img,
            Err(e) => {
                log::warn!("image #{}: {e}", job.index);
                Decoded::placeholder()
            }
        };
        log::debug!(
            "{:?} #{} {}x{} en {:.1} ms",
            job.quality,
            job.index,
            image.width,
            image.height,
            t0.elapsed().as_secs_f64() * 1000.0
        );

        // Le résultat part avant le retrait de `in_flight` : le thread principal
        // ne peut donc pas redemander une image déjà décodée mais pas encore reçue.
        let _ = tx.send(Done {
            job,
            image: Arc::new(image),
        });
        shared.state.lock().unwrap().in_flight.remove(&job);
        let _ = proxy.send_event(());
    }
}

fn decode(
    dec: &mut Decompressor,
    data: &[u8],
    quality: Quality,
) -> Result<Decoded, Box<dyn std::error::Error>> {
    let header = dec.read_header(data)?;
    let factor = match quality {
        Quality::Reduced => ScalingFactor::new(REDUCED_SCALE.0, REDUCED_SCALE.1),
        Quality::Full => ScalingFactor::new(1, 1),
    };
    dec.set_scaling_factor(factor)?;
    let scaled = header.scaled(factor);
    let mut image = Image {
        pixels: vec![0u8; 4 * scaled.width * scaled.height],
        width: scaled.width,
        pitch: 4 * scaled.width,
        height: scaled.height,
        format: PixelFormat::RGBA,
    };
    dec.decompress(data, image.as_deref_mut())?;
    Ok(Decoded {
        width: scaled.width as u32,
        height: scaled.height as u32,
        pixels: image.pixels,
        full_width: header.width as u32,
        full_height: header.height as u32,
        orientation: exif_orientation(data),
    })
}

fn exif_orientation(data: &[u8]) -> u8 {
    let mut cursor = std::io::Cursor::new(data);
    exif::Reader::new()
        .read_from_container(&mut cursor)
        .ok()
        .and_then(|e| {
            e.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        })
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1)
}
