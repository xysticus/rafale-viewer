mod loader;
mod panel;
mod render;

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use loader::{Decoded, Job, Loader, Quality};
use panel::{Line, Panel};
use render::Renderer;

/// Délai avant le défilé continu quand on maintient la touche (un appui bref = une image).
const HOLD_DELAY: Duration = Duration::from_millis(300);
/// Temps d'arrêt sur une image avant de lancer le décodage pleine résolution.
const FULL_DELAY: Duration = Duration::from_millis(100);
/// Préchargement en résolution réduite de part et d'autre de l'image courante.
const PREFETCH: isize = 10;
const DEFAULT_FPS: f64 = 25.0;
const MARK_DIR: &str = "retenus";

struct App {
    dir: PathBuf,
    files: Arc<Vec<PathBuf>>,
    marked: HashSet<usize>,
    proxy: EventLoopProxy<()>,
    loader: Option<Loader>,
    reduced: LruCache<usize, Arc<Decoded>>,
    full: LruCache<usize, Arc<Decoded>>,

    cur: usize,
    /// Sens du dernier déplacement (+1 / -1), pour orienter le préchargement.
    heading: isize,
    forward_held: bool,
    backward_held: bool,
    /// Sens du défilé continu en cours (0 = aucun).
    scrolling: isize,
    /// Pas demandés par des appuis mais pas encore faits (image pas encore décodée).
    pending: isize,
    next_step_at: Instant,
    last_change: Instant,
    step_interval: Duration,

    shown: Option<Job>,
    title: String,
    panel: Panel,
    /// Contenu du panneau actuellement affiché (texte et largeur).
    panel_key: String,
    /// Position du pointeur dans la fenêtre, en pixels physiques.
    cursor: Option<(f32, f32)>,
    loupe_allowed: bool,
    cursor_hidden: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl App {
    fn new(dir: PathBuf, files: Vec<PathBuf>, proxy: EventLoopProxy<()>, fps: f64) -> Self {
        let marked = files
            .iter()
            .enumerate()
            .filter(|(_, f)| mark_path(&dir, f).exists())
            .map(|(i, _)| i)
            .collect();
        let now = Instant::now();
        App {
            dir,
            files: Arc::new(files),
            marked,
            proxy,
            loader: None,
            reduced: LruCache::new(NonZeroUsize::new(64).unwrap()),
            full: LruCache::new(NonZeroUsize::new(6).unwrap()),
            cur: 0,
            heading: 1,
            forward_held: false,
            backward_held: false,
            scrolling: 0,
            pending: 0,
            next_step_at: now,
            last_change: now,
            step_interval: Duration::from_secs_f64(1.0 / fps),
            shown: None,
            title: String::new(),
            panel: Panel::load(),
            panel_key: String::new(),
            cursor: None,
            loupe_allowed: false,
            cursor_hidden: false,
            window: None,
            renderer: None,
        }
    }

    fn is_ready(&self, i: usize) -> bool {
        self.reduced.contains(&i) || self.full.contains(&i)
    }

    fn on_key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        // La répétition clavier est ignorée : seuls l'appui et le relâchement comptent.
        if event.repeat {
            return;
        }
        let pressed = event.state == ElementState::Pressed;
        // Les lettres suivent la disposition du clavier (AZERTY), le reste la position physique.
        if let Key::Character(c) = &event.logical_key {
            match c.to_lowercase().as_str() {
                "f" if pressed => return self.toggle_fullscreen(),
                "q" if pressed => return event_loop.exit(),
                _ => {}
            }
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let now = Instant::now();
        match code {
            KeyCode::ArrowRight | KeyCode::PageDown => {
                self.forward_held = pressed;
                self.direction_key(1, pressed, now);
            }
            KeyCode::ArrowLeft | KeyCode::PageUp => {
                self.backward_held = pressed;
                self.direction_key(-1, pressed, now);
            }
            KeyCode::Home if pressed => self.jump(0, now),
            KeyCode::End if pressed => self.jump(self.files.len() - 1, now),
            KeyCode::Space if pressed => self.toggle_mark(),
            KeyCode::F11 if pressed => self.toggle_fullscreen(),
            KeyCode::Escape if pressed => event_loop.exit(),
            _ => {}
        }
    }

    fn toggle_fullscreen(&self) {
        if let Some(w) = &self.window {
            let fs = w.fullscreen().is_none().then(|| Fullscreen::Borderless(None));
            w.set_fullscreen(fs);
        }
    }

    fn direction_key(&mut self, dir: isize, pressed: bool, now: Instant) {
        if pressed {
            self.pending += dir;
            self.scrolling = dir;
            self.next_step_at = now + HOLD_DELAY;
        } else if self.scrolling == dir {
            // Si l'autre touche de direction est encore maintenue, elle reprend la main.
            self.scrolling = if dir == 1 && self.backward_held {
                -1
            } else if dir == -1 && self.forward_held {
                1
            } else {
                0
            };
            self.next_step_at = now + self.step_interval;
        }
    }

    fn jump(&mut self, i: usize, now: Instant) {
        self.pending = 0;
        self.heading = if i >= self.cur { 1 } else { -1 };
        self.cur = i;
        self.last_change = now;
    }

    /// Avance d'une image si c'est le moment et qu'elle est prête — jamais de saut.
    fn step(&mut self, now: Instant) {
        let dir = if self.pending != 0 {
            self.pending.signum()
        } else if self.scrolling != 0 && now >= self.next_step_at {
            self.scrolling
        } else {
            return;
        };
        let Some(target) = self
            .cur
            .checked_add_signed(dir)
            .filter(|&t| t < self.files.len())
        else {
            self.pending = 0;
            return;
        };
        if !self.is_ready(target) {
            return; // on attend le décodeur, qui réveillera la boucle
        }
        if self.pending != 0 {
            self.pending -= dir;
        } else {
            // Cadence régulière, mais pas de rattrapage en rafale après une attente.
            let base = if now.duration_since(self.next_step_at) > self.step_interval {
                now
            } else {
                self.next_step_at
            };
            self.next_step_at = base + self.step_interval;
        }
        self.cur = target;
        self.heading = dir;
        self.last_change = now;
    }

    fn stationary(&self, now: Instant) -> bool {
        self.scrolling == 0 && self.pending == 0 && now >= self.last_change + FULL_DELAY
    }

    /// Tâches de décodage voulues, par priorité décroissante.
    fn wanted(&self, now: Instant) -> Vec<Job> {
        let n = self.files.len() as isize;
        let stationary = self.stationary(now);
        let mut jobs = Vec::with_capacity(2 * PREFETCH as usize + 4);
        let mut want = |i: isize, quality: Quality| {
            if !(0..n).contains(&i) {
                return;
            }
            let index = i as usize;
            let cached = match quality {
                Quality::Reduced => self.is_ready(index),
                Quality::Full => self.full.contains(&index),
            };
            if !cached {
                jobs.push(Job { index, quality });
            }
        };
        let cur = self.cur as isize;
        let h = self.heading;
        want(cur, Quality::Reduced);
        if stationary {
            want(cur, Quality::Full);
        }
        for k in 1..=PREFETCH {
            want(cur + h * k, Quality::Reduced);
        }
        for k in 1..=PREFETCH {
            want(cur - h * k, Quality::Reduced);
        }
        if stationary {
            want(cur + h, Quality::Full);
            want(cur - h, Quality::Full);
        }
        jobs
    }

    fn toggle_mark(&mut self) {
        let src = self.files[self.cur].clone();
        let dst = mark_path(&self.dir, &src);
        if self.marked.remove(&self.cur) {
            if let Err(e) = std::fs::remove_file(&dst) {
                log::warn!("suppression de {} : {e}", dst.display());
            }
        } else {
            self.marked.insert(self.cur);
            // Copie en arrière-plan pour ne pas bloquer l'affichage.
            std::thread::spawn(move || {
                let r = std::fs::create_dir_all(dst.parent().unwrap())
                    .and_then(|_| std::fs::copy(&src, &dst));
                if let Err(e) = r {
                    log::error!("copie vers {} : {e}", dst.display());
                }
            });
        }
        self.update_title();
    }

    /// La loupe ne s'affiche qu'à l'arrêt, sur l'image en pleine résolution.
    fn loupe_allowed(&self) -> bool {
        self.scrolling == 0
            && self.pending == 0
            && self.shown
                == Some(Job {
                    index: self.cur,
                    quality: Quality::Full,
                })
    }

    fn update_panel(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let width = renderer.panel_width();
        let name = self.files[self.cur]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let (mark, mark_color) = if self.marked.contains(&self.cur) {
            ("★ Retenue", [110, 210, 120])
        } else {
            ("Non retenue", [105, 105, 105])
        };
        let status = match self.shown {
            Some(Job { index, quality: Quality::Full }) if index == self.cur => {
                "100 % : loupe sous le pointeur"
            }
            Some(Job { index, .. }) if index == self.cur => "aperçu réduit",
            _ => "chargement…",
        };
        let lines = [
            Line { text: mark.into(), size: 44.0, color: mark_color, gap: 0.0 },
            Line { text: name, size: 30.0, color: [230, 230, 230], gap: 20.0 },
            Line {
                text: format!("{} / {}", self.cur + 1, self.files.len()),
                size: 24.0,
                color: [150, 150, 150],
                gap: 4.0,
            },
            Line { text: status.into(), size: 22.0, color: [125, 125, 125], gap: 20.0 },
        ];
        let key = format!(
            "{width}|{}",
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("|")
        );
        if key == self.panel_key {
            return;
        }
        let (pixels, w, h) = self.panel.render(width, &lines);
        renderer.set_panel(&pixels, w, h);
        self.panel_key = key;
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn update_title(&mut self) {
        let Some(window) = &self.window else { return };
        let name = self.files[self.cur]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mark = if self.marked.contains(&self.cur) { "  ★" } else { "" };
        let quality = match self.shown {
            Some(Job { index, quality: Quality::Full }) if index == self.cur => "",
            Some(Job { index, .. }) if index == self.cur => "  · réduit",
            _ => "  · chargement…",
        };
        let title = format!("{name}  {}/{}{mark}{quality}", self.cur + 1, self.files.len());
        // set_title redessine les décorations, ce qui relance la boucle : seulement si changé.
        if title != self.title {
            window.set_title(&title);
            self.title = title;
        }
    }

    /// Une itération de la logique, après chaque lot d'événements.
    fn update(&mut self, event_loop: &ActiveEventLoop) {
        let Some(loader) = &self.loader else { return };
        while let Some(done) = loader.try_recv() {
            match done.job.quality {
                Quality::Reduced => self.reduced.put(done.job.index, done.image),
                Quality::Full => self.full.put(done.job.index, done.image),
            };
        }

        let now = Instant::now();
        self.step(now);
        let jobs = self.wanted(now);
        self.loader.as_ref().unwrap().set_wanted(jobs);

        // Meilleure version disponible de l'image courante.
        let best = if let Some(img) = self.full.get(&self.cur) {
            Some((Quality::Full, img.clone()))
        } else {
            self.reduced
                .get(&self.cur)
                .map(|img| (Quality::Reduced, img.clone()))
        };
        if let Some((quality, img)) = best {
            let job = Job {
                index: self.cur,
                quality,
            };
            if self.shown != Some(job) {
                if let Some(r) = &mut self.renderer {
                    r.set_image(&img, quality == Quality::Full);
                }
                self.shown = Some(job);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
        let loupe_allowed = self.loupe_allowed();
        if loupe_allowed != self.loupe_allowed {
            self.loupe_allowed = loupe_allowed;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        self.update_panel();
        self.update_title();

        // Prochain réveil : pas de défilé, ou lancement de la pleine résolution.
        let mut deadline = None;
        if self.scrolling != 0 && self.next_step_at > now {
            deadline = Some(self.next_step_at);
        }
        if !self.stationary(now) && self.scrolling == 0 && self.pending == 0 {
            let t = self.last_change + FULL_DELAY;
            deadline = Some(deadline.map_or(t, |d: Instant| d.min(t)));
        }
        event_loop.set_control_flow(match deadline {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("rafale")
                        .with_maximized(true),
                )
                .expect("création de la fenêtre"),
        );
        self.renderer = Some(Renderer::new(
            window.clone(),
            event_loop.owned_display_handle(),
        ));
        self.window = Some(window);
        self.loader = Some(Loader::new(self.files.clone(), self.proxy.clone()));
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // Un décodage est terminé ; il sera traité dans `about_to_wait`.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let loupe_at = self.cursor.filter(|_| self.loupe_allowed);
                if let Some(r) = &mut self.renderer {
                    // Pointeur masqué sous la loupe, pour ne pas cacher le visage.
                    let hide = r.render(loupe_at);
                    if hide != self.cursor_hidden {
                        self.cursor_hidden = hide;
                        if let Some(w) = &self.window {
                            w.set_cursor_visible(!hide);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x as f32, position.y as f32));
                if self.loupe_allowed {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::Focused(false) => {
                // Un relâchement hors fenêtre ne nous parvient pas.
                self.forward_held = false;
                self.backward_held = false;
                self.scrolling = 0;
            }
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event, event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.update(event_loop);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Arrête les threads de décodage avant de libérer le GPU.
        self.loader = None;
    }
}

fn mark_path(dir: &Path, file: &Path) -> PathBuf {
    dir.join(MARK_DIR).join(file.file_name().unwrap())
}

fn list_jpegs(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut dir = None;
    let mut fps = DEFAULT_FPS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fps" => {
                fps = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&f: &f64| f > 0.0)
                    .unwrap_or_else(|| usage());
            }
            "-h" | "--help" => usage(),
            _ if dir.is_none() => dir = Some(PathBuf::from(arg)),
            _ => usage(),
        }
    }
    let dir = dir.unwrap_or_else(|| usage());
    let files = match list_jpegs(&dir) {
        Ok(f) if !f.is_empty() => f,
        Ok(_) => {
            eprintln!("aucun JPEG dans {}", dir.display());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{} : {e}", dir.display());
            std::process::exit(1);
        }
    };
    log::info!("{} JPEG dans {}", files.len(), dir.display());

    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .expect("boucle d'événements");
    let mut app = App::new(dir, files, event_loop.create_proxy(), fps);
    event_loop.run_app(&mut app).expect("boucle d'événements");
}

fn usage() -> ! {
    eprintln!("usage : rafale-viewer <dossier> [--fps N]");
    std::process::exit(2);
}
