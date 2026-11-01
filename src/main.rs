mod help;
mod loader;
mod marks;
mod raw;
mod meta;
mod panel;
mod render;

use std::num::NonZeroUsize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use loader::{Decoded, Job, Loader, Quality};
use marks::{Color, Filter, Mark};
use meta::Meta;
use panel::{Cell, Controls, CopyKind, Entry, Hit, Line, Panel, Row, Track, View, Widget};
use render::{Rect, Renderer};

/// Délai avant le défilé continu quand on maintient la touche (un appui bref = une image).
const HOLD_DELAY: Duration = Duration::from_millis(300);
/// Temps d'arrêt sur une image avant de lancer le décodage pleine résolution.
const FULL_DELAY: Duration = Duration::from_millis(100);
/// Préchargement en résolution réduite de part et d'autre de l'image courante.
const PREFETCH: usize = 10;
const DEFAULT_FPS: f64 = 25.0;
/// La vue à plat (une vignette par photo) n'est proposée qu'en dessous de ce nombre de photos.
const FLAT_MAX: usize = 300;
/// Vue au lancement : à plat, sauf si une rafale dépasse ce nombre de photos.
const FLAT_BURST_MAX: usize = 5;

struct Burst {
    start: usize,
    len: usize,
    /// Rythme mesuré, en images/s.
    rate: Option<f32>,
}

/// Géométrie de l'ascenseur de la liste, en pixels écran.
#[derive(Clone, Copy)]
struct Scrollbar {
    track: Rect,
    thumb: Rect,
    /// Première ligne pour un décalage nul (ligne courante au premier tiers).
    anchor: isize,
    /// Indice maximal de la première ligne affichée.
    last_first: isize,
}

/// Largeur de l'ascenseur et marge de tolérance au clic de part et d'autre.
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_GRAB: f32 = 8.0;

struct App {
    files: Arc<Vec<PathBuf>>,
    marks: Vec<Mark>,
    /// RAW associé à chaque JPEG (même nom), et si son XMP appartient à un autre logiciel.
    raws: Vec<Option<PathBuf>>,
    raw_foreign: Vec<bool>,
    /// Filtre de navigation, et images qui le passent (triées ; vide sans filtre).
    filter: Filter,
    visible: Vec<usize>,
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
    /// Loupe figée : point de la photo (proportions 0..1) où elle reste d'une vue
    /// à l'autre, au lieu de suivre le pointeur.
    pin: Option<(f32, f32)>,

    /// En-têtes analysés en arrière-plan, dans l'ordre des fichiers.
    scan_rx: crossbeam_channel::Receiver<(usize, Meta)>,
    metas: Vec<Option<Meta>>,
    scanned: usize,
    bursts: Vec<Burst>,
    /// Rafale de chaque fichier déjà analysé.
    burst_of: Vec<usize>,
    /// Vue à plat : une vignette par photo au lieu d'une ligne par rafale.
    flat: bool,
    /// Vue choisie au bouton : elle n'est plus changée automatiquement.
    flat_chosen: bool,
    /// Fichiers copiés par Ctrl+C, et compte rendu de la dernière copie.
    copy_kind: CopyKind,
    copy_message: String,
    modifiers: Modifiers,
    /// Fenêtre d'aide ouverte (zenity) : la fenêtre principale ignore alors les entrées.
    help: Option<Child>,
    /// Décalage de la liste à la molette (en lignes), remis à zéro au changement
    /// de ligne courante (rafale, ou rangée en vue à plat).
    list_offset: isize,
    list_group: Option<(bool, usize)>,
    list_key: String,
    list_row_h: f32,
    /// Zones cliquables : marquage (relatives au panneau), lignes ou vignettes de
    /// la liste et commandes du bas (relatives à la liste).
    info_hits: Vec<Hit>,
    list_hits: Vec<Hit>,
    bar_hits: Vec<Hit>,
    tracks: Vec<Track>,
    scrollbar: Option<Scrollbar>,
    /// Glissement de l'ascenseur en cours : décalage du pointeur dans le curseur.
    dragging: Option<f32>,

    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl App {
    fn new(dir: PathBuf, files: Vec<PathBuf>, proxy: EventLoopProxy<()>, fps: f64) -> Self {
        let files = Arc::new(files);
        let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
        meta::spawn(files.clone(), scan_tx, proxy.clone());
        // Un seul parcours du dossier pour trouver les XMP existants.
        let sidecars: std::collections::HashSet<PathBuf> = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        let marks = files
            .iter()
            .map(|f| {
                if sidecars.contains(&marks::sidecar(f)) {
                    marks::load(f)
                } else {
                    Mark::default()
                }
            })
            .collect::<Vec<_>>();
        let raws = raw::find(&dir, &files);
        let raw_foreign = sync_raws(&marks, &raws);
        let now = Instant::now();
        App {
            metas: (0..files.len()).map(|_| None).collect(),
            files,
            marks,
            raws,
            raw_foreign,
            filter: Filter::default(),
            visible: Vec::new(),
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
            pin: None,
            scan_rx,
            scanned: 0,
            bursts: Vec::new(),
            burst_of: Vec::new(),
            flat: true,
            flat_chosen: false,
            copy_kind: CopyKind::default(),
            copy_message: String::new(),
            modifiers: Modifiers::default(),
            help: None,
            list_offset: 0,
            list_group: None,
            list_key: String::new(),
            list_row_h: 1.0,
            info_hits: Vec::new(),
            list_hits: Vec::new(),
            bar_hits: Vec::new(),
            tracks: Vec::new(),
            scrollbar: None,
            dragging: None,
            window: None,
            renderer: None,
        }
    }

    fn is_ready(&self, i: usize) -> bool {
        self.reduced.contains(&i) || self.full.contains(&i)
    }

    /// Recalcule les images qui passent le filtre (après un changement de filtre ou de marquage).
    fn refresh_visible(&mut self) {
        self.visible = if self.filter.active() {
            (0..self.files.len())
                .filter(|&i| self.filter.matches(&self.marks[i]))
                .collect()
        } else {
            Vec::new()
        };
    }

    /// `k`-ième image visible après (`dir` = 1) ou avant (`dir` = -1) l'image `from`.
    fn neighbor(&self, from: usize, dir: isize, k: usize) -> Option<usize> {
        if !self.filter.active() {
            return from
                .checked_add_signed(dir * k as isize)
                .filter(|&i| i < self.files.len());
        }
        if dir > 0 {
            let pos = self.visible.partition_point(|&v| v <= from);
            self.visible.get(pos + k - 1).copied()
        } else {
            let pos = self.visible.partition_point(|&v| v < from);
            pos.checked_sub(k).map(|p| self.visible[p])
        }
    }

    fn first_visible_in(&self, start: usize, len: usize) -> Option<usize> {
        if self.filter.active() {
            visible_in(&self.visible, start, len).first().copied()
        } else {
            (len > 0).then_some(start)
        }
    }

    /// Change le filtre ; si l'image courante n'y répond plus, va à la plus proche qui y répond.
    fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.refresh_visible();
        if self.filter.active() && self.visible.binary_search(&self.cur).is_err() {
            if let Some(i) = self.neighbor(self.cur, 1, 1).or_else(|| self.neighbor(self.cur, -1, 1)) {
                self.jump(i, Instant::now());
            }
        }
        self.list_offset = 0;
    }

    fn on_key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        // La répétition clavier est ignorée : seuls l'appui et le relâchement comptent.
        if event.repeat {
            return;
        }
        let pressed = event.state == ElementState::Pressed;
        // Les lettres suivent la disposition du clavier (AZERTY), le reste la position physique.
        if let Key::Character(c) = &event.logical_key {
            let ctrl = self.modifiers.state().control_key();
            match c.to_lowercase().as_str() {
                "c" if pressed && ctrl => return self.copy_selection(),
                _ if ctrl => return,
                "f" if pressed => return self.toggle_fullscreen(),
                "q" if pressed => return event_loop.exit(),
                // Rejet : X comme Lightroom, R comme darktable.
                "x" | "r" if pressed => {
                    return self.edit_mark(|m| {
                        m.rejected = !m.rejected;
                        m.rating = 0;
                    });
                }
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
            KeyCode::ArrowUp if pressed => self.vertical_step(-1, now),
            KeyCode::ArrowDown if pressed => self.vertical_step(1, now),
            KeyCode::Home if pressed => {
                if let Some(i) = self.first_visible_in(0, self.files.len()) {
                    self.jump(i, now);
                }
            }
            KeyCode::End if pressed => {
                let last = self.files.len() - 1;
                let target = if self.filter.active() { self.visible.last().copied() } else { Some(last) };
                if let Some(i) = target {
                    self.jump(i, now);
                }
            }
            KeyCode::Space if pressed => self.edit_mark(|m| {
                m.rating = if m.retained() { 0 } else { 1 };
                m.rejected = false;
            }),
            KeyCode::Digit0 | KeyCode::Numpad0 if pressed => self.set_rating(0),
            KeyCode::Digit1 | KeyCode::Numpad1 if pressed => self.set_rating(1),
            KeyCode::Digit2 | KeyCode::Numpad2 if pressed => self.set_rating(2),
            KeyCode::Digit3 | KeyCode::Numpad3 if pressed => self.set_rating(3),
            KeyCode::Digit4 | KeyCode::Numpad4 if pressed => self.set_rating(4),
            KeyCode::Digit5 | KeyCode::Numpad5 if pressed => self.set_rating(5),
            KeyCode::F1 if pressed => self.toggle_color(Color::Red),
            KeyCode::F2 if pressed => self.toggle_color(Color::Yellow),
            KeyCode::F3 if pressed => self.toggle_color(Color::Green),
            KeyCode::F4 if pressed => self.toggle_color(Color::Blue),
            KeyCode::F5 if pressed => self.toggle_color(Color::Purple),
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

    /// ↑ / ↓ : rangée de la grille en vue à plat, rafale sinon.
    fn vertical_step(&mut self, dir: isize, now: Instant) {
        if self.flat_view() {
            let per = panel::THUMBS_PER_ROW;
            if let Some(i) = (1..=per).rev().find_map(|k| self.neighbor(self.cur, dir, k)) {
                self.jump(i, now);
            }
        } else {
            self.burst_step(dir, now);
        }
    }

    fn flat_view(&self) -> bool {
        self.flat && self.files.len() < FLAT_MAX
    }

    /// Sélection : les photos qui passent le filtre s'il y en a un, sinon les retenues.
    fn selection(&self) -> Vec<usize> {
        if self.filter.active() {
            self.visible.clone()
        } else {
            (0..self.files.len()).filter(|&i| self.marks[i].retained()).collect()
        }
    }

    /// Met les fichiers de la sélection dans le presse-papiers, comme une copie
    /// dans Fichiers : un Ctrl+V dans un dossier les y copie.
    fn copy_selection(&mut self) {
        let selection = self.selection();
        let mut paths = Vec::new();
        let mut no_raw = 0;
        for &i in &selection {
            if self.copy_kind != CopyKind::Raw {
                paths.push(self.files[i].clone());
            }
            if self.copy_kind != CopyKind::Jpeg {
                match &self.raws[i] {
                    Some(raw) => paths.push(raw.clone()),
                    None => no_raw += 1,
                }
            }
        }
        self.copy_message = if paths.is_empty() {
            if selection.is_empty() { "sélection vide".into() } else { "aucun RAW".into() }
        } else {
            match clipboard_files(&paths) {
                Ok(()) => {
                    let missing = if no_raw > 0 { format!(", {no_raw} sans RAW") } else { String::new() };
                    format!("{} fichiers copiés{missing} : Ctrl+V dans Fichiers", paths.len())
                }
                Err(e) => {
                    log::error!("presse-papiers : {e}");
                    format!("échec : {e}")
                }
            }
        };
        log::info!("copie de la sélection : {}", self.copy_message);
    }

    /// Ouvre une fenêtre d'aide à texte sélectionnable (zenity).
    fn show_help(&mut self, title: &str, text: &str) {
        if self.help_open() {
            return;
        }
        let child = Command::new("zenity")
            .args(["--text-info", "--title", title, "--width=780", "--height=760"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(mut c) => {
                // Le texte tient dans le tampon du tube ; fermer l'entrée marque la fin.
                if let Some(mut stdin) = c.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                self.help = Some(c);
            }
            Err(e) => log::warn!("zenity : {e}"),
        }
    }

    /// Fenêtre d'aide encore ouverte ?
    fn help_open(&mut self) -> bool {
        let open = self.help.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !open {
            self.help = None;
        }
        open
    }

    /// Début de la rafale précédente ou suivante (qui a des images visibles).
    fn burst_step(&mut self, dir: isize, now: Instant) {
        let Some(&b) = self.burst_of.get(self.cur) else {
            return;
        };
        let mut t = b;
        while let Some(next) = t.checked_add_signed(dir).filter(|&n| n < self.bursts.len()) {
            t = next;
            let burst = &self.bursts[t];
            if let Some(i) = self.first_visible_in(burst.start, burst.len) {
                return self.jump(i, now);
            }
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
        let Some(target) = self.neighbor(self.cur, dir, 1) else {
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
        let stationary = self.stationary(now);
        let mut jobs = Vec::with_capacity(2 * PREFETCH as usize + 4);
        let mut want = |i: Option<usize>, quality: Quality| {
            let Some(index) = i else { return };
            let cached = match quality {
                Quality::Reduced => self.is_ready(index),
                Quality::Full => self.full.contains(&index),
            };
            if !cached {
                jobs.push(Job { index, quality });
            }
        };
        // Voisines au sens du filtre : on ne précharge que ce qui sera affiché.
        let (cur, h) = (self.cur, self.heading);
        want(Some(cur), Quality::Reduced);
        if stationary {
            want(Some(cur), Quality::Full);
        }
        for k in 1..=PREFETCH {
            want(self.neighbor(cur, h, k), Quality::Reduced);
        }
        for k in 1..=PREFETCH {
            want(self.neighbor(cur, -h, k), Quality::Reduced);
        }
        if stationary {
            want(self.neighbor(cur, h, 1), Quality::Full);
            want(self.neighbor(cur, -h, 1), Quality::Full);
        }
        jobs
    }

    /// Modifie le marquage de l'image courante et l'enregistre dans son XMP.
    fn edit_mark(&mut self, f: impl FnOnce(&mut Mark)) {
        let mark = &mut self.marks[self.cur];
        if mark.foreign {
            log::warn!(
                "{} : XMP d'un autre logiciel, marquage non modifié",
                self.files[self.cur].display()
            );
            return;
        }
        f(mark);
        if let Err(e) = marks::save(&self.files[self.cur], mark) {
            log::error!("{} : {e}", marks::sidecar(&self.files[self.cur]).display());
        }
        // Même marquage sur le RAW, s'il y en a un.
        if let Some(raw) = &self.raws[self.cur] {
            let xmp = marks::sidecar(raw);
            if let Err(e) = marks::save_to(&xmp, mark) {
                log::warn!("{} : {e}", xmp.display());
                self.raw_foreign[self.cur] = marks::is_foreign(&xmp);
            }
        }
        // L'image courante reste affichée même si elle ne passe plus le filtre.
        self.refresh_visible();
    }

    /// Clic sur une commande de marquage (en haut du panneau).
    fn mark_widget(&mut self, widget: Widget) {
        match widget {
            Widget::Star(n) => {
                let n = if self.marks[self.cur].rating == n { 0 } else { n };
                self.set_rating(n);
            }
            Widget::Color(c) => self.toggle_color(c),
            Widget::Check => self.edit_mark(|m| {
                m.rating = if m.retained() { 0 } else { 1 };
                m.rejected = false;
            }),
            Widget::Reject => self.edit_mark(|m| {
                m.rejected = !m.rejected;
                m.rating = 0;
            }),
            _ => {}
        }
    }

    /// Clic sur une commande du bas de la liste : filtre, vue ou aide.
    fn bar_widget(&mut self, widget: Widget) {
        let mut f = self.filter;
        match widget {
            Widget::Star(n) => f.min_rating = if f.min_rating == n { 0 } else { n },
            Widget::Color(c) => f.colors[c.index()] = !f.colors[c.index()],
            Widget::Check => f.retained = !f.retained,
            Widget::Reject => f.rejected = !f.rejected,
            Widget::ClearFilter => f = Filter::default(),
            Widget::Bursts | Widget::Flat => {
                self.flat = widget == Widget::Flat;
                self.flat_chosen = true;
                self.list_offset = 0;
                return;
            }
            Widget::Principles => return self.show_help(help::PRINCIPLES_TITLE, help::PRINCIPLES),
            Widget::Shortcuts => return self.show_help(help::SHORTCUTS_TITLE, help::SHORTCUTS),
            Widget::CopyKind(kind) => {
                self.copy_kind = kind;
                self.copy_message.clear();
                return;
            }
            Widget::Copy => return self.copy_selection(),
            Widget::Photo(_) => return,
        }
        self.set_filter(f);
    }

    /// Chiffre = nombre d'étoiles (0 efface) ; annule un rejet.
    fn set_rating(&mut self, stars: u8) {
        self.edit_mark(|m| {
            m.rating = stars;
            m.rejected = false;
        });
    }

    fn toggle_color(&mut self, color: Color) {
        self.edit_mark(|m| m.color = if m.color == Some(color) { None } else { Some(color) });
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
        let m = self.marks[self.cur];
        let mut colors = [false; 5];
        if let Some(c) = m.color {
            colors[c.index()] = true;
        }
        let controls = Controls {
            rating: if m.rejected { 0 } else { m.rating },
            colors,
            check: m.retained(),
            reject: m.rejected,
        };
        let (mark, mark_color) = if m.foreign {
            ("XMP externe, lecture seule".to_string(), [200, 150, 90])
        } else if m.rejected {
            ("Rejetée".to_string(), [225, 85, 75])
        } else if m.retained() {
            ("Retenue".to_string(), [110, 210, 120])
        } else {
            ("Non retenue".to_string(), [105, 105, 105])
        };
        let meta = self.metas[self.cur].as_ref();
        let raw_text = match &self.raws[self.cur] {
            Some(raw) => {
                let folder = self.files[self.cur].parent().unwrap_or(Path::new(""));
                let rel = raw.strip_prefix(folder).unwrap_or(raw).display();
                if self.raw_foreign[self.cur] {
                    format!("RAW : {rel}   (XMP externe, non synchronisé)")
                } else {
                    format!("RAW : {rel}")
                }
            }
            None => "pas de RAW".to_string(),
        };
        let lines = [
            Line { text: mark, size: 16.0, color: mark_color, gap: 0.0 },
            Line { text: name, size: 21.0, color: [230, 230, 230], gap: 12.0 },
            Line {
                text: meta.map(|m| m.exposure.clone()).unwrap_or_default(),
                size: 17.0,
                color: [205, 205, 205],
                gap: 4.0,
            },
            Line {
                text: meta
                    .map(|m| {
                        let mut parts = Vec::new();
                        parts.extend(m.picture_mode.clone());
                        parts.extend(m.noise_filter.map(|n| format!("filtre bruit {n}")));
                        parts.join("   ·   ")
                    })
                    .unwrap_or_default(),
                size: 16.0,
                color: [150, 150, 150],
                gap: 3.0,
            },
            Line { text: raw_text, size: 16.0, color: [150, 150, 150], gap: 12.0 },
            Line {
                text: if self.pin.is_some() {
                    "Loupe figée · clic droit pour la libérer".to_string()
                } else {
                    String::new()
                },
                size: 16.0,
                color: [90, 160, 255],
                gap: 3.0,
            },
            Line {
                text: format!("{} / {}", self.cur + 1, self.files.len()),
                size: 17.0,
                color: [150, 150, 150],
                gap: 3.0,
            },
            Line {
                text: match self.burst_of.get(self.cur) {
                    Some(&b) => format!(
                        "Rafale {}   vue {} / {}{}",
                        b + 1,
                        self.cur - self.bursts[b].start + 1,
                        self.bursts[b].len,
                        self.bursts[b].rate.map(|r| format!("   {}", panel::format_rate(r))).unwrap_or_default()
                    ),
                    None => String::new(),
                },
                size: 17.0,
                color: [150, 150, 150],
                gap: 3.0,
            },
        ];
        let key = format!(
            "{width}|{controls:?}|{}",
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("|")
        );
        if key == self.panel_key {
            return;
        }
        let (pixels, w, h, hits) = self.panel.render_info(width, &controls, &lines);
        renderer.set_panel(&pixels, w, h);
        self.info_hits = hits;
        self.panel_key = key;
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Rythme d'une rafale, en images/s. L'horodatage est à la seconde : le rythme
    /// n'est exact qu'entre deux changements de seconde, donc pour une rafale qui
    /// couvre au moins trois secondes distinctes.
    fn burst_rate(&self, burst: &Burst) -> Option<f32> {
        let times: Vec<i64> = (burst.start..burst.start + burst.len)
            .map(|i| self.metas[i].as_ref().and_then(|m| m.time))
            .collect::<Option<_>>()?;
        let changes: Vec<usize> = (1..times.len()).filter(|&k| times[k] != times[k - 1]).collect();
        let (&first, &last) = (changes.first()?, changes.last()?);
        (last > first).then(|| (last - first) as f32 / (times[last] - times[first]) as f32)
    }

    fn add_meta(&mut self, index: usize, meta: Meta) {
        debug_assert_eq!(index, self.burst_of.len(), "analyse dans l'ordre");
        let continues = index > 0
            && self.metas[index - 1]
                .as_ref()
                .is_some_and(|prev| meta::same_burst(prev, &meta));
        match self.bursts.last_mut() {
            Some(b) if continues => b.len += 1,
            _ => self.bursts.push(Burst {
                start: index,
                len: 1,
                rate: None,
            }),
        }
        self.burst_of.push(self.bursts.len() - 1);
        // Une vraie rafale détectée : la vue par rafales s'impose (sauf choix au bouton).
        if !self.flat_chosen && self.bursts.last().unwrap().len > FLAT_BURST_MAX {
            self.flat = false;
        }
        self.metas[index] = Some(meta);
        let last = self.bursts.last().unwrap();
        let rate = self.burst_rate(last);
        self.bursts.last_mut().unwrap().rate = rate;
        self.scanned = index + 1;
        if self.scanned == self.files.len() {
            let rates: Vec<f32> = self.bursts.iter().filter_map(|b| b.rate).collect();
            log::info!(
                "{} rafales, rythme mesuré pour {} : {:?} i/s",
                self.bursts.len(),
                rates.len(),
                rates.iter().map(|r| format!("{r:.1}")).collect::<std::collections::BTreeSet<_>>()
            );
        }
    }

    /// Liste autour de la ligne courante (rafales, ou rangées de photos en vue à
    /// plat) ; recomposée seulement si elle change.
    fn update_list(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let width = renderer.panel_width();
        let (lx, ly) = renderer.list_origin();
        let (_, wh) = renderer.window_size();
        let height = (wh - ly).max(0.0) as u32;
        let status = if self.scanned < self.files.len() {
            format!("Analyse des rafales…  {} / {}", self.scanned, self.files.len())
        } else {
            String::new()
        };
        // Ligne d'état en haut (pendant l'analyse), filtre et boutons en bas.
        let status_h = if status.is_empty() { 0.0 } else { panel::PADDING };
        let header = panel::BOTTOM_BAR + status_h;
        let flat = self.flat && self.files.len() < FLAT_MAX;
        let row_h = if flat { Panel::grid_row_height(width) } else { Panel::row_height(width) };
        self.list_row_h = row_h;
        let fit = ((height as f32 - header) / row_h).max(0.0) as isize;
        let filtering = self.filter.active();
        let per = panel::THUMBS_PER_ROW;

        // Vue à plat : les photos visibles, par rangées. Sinon : les rafales qui
        // ont au moins une image visible.
        let photos: Vec<usize> = if !flat {
            Vec::new()
        } else if filtering {
            self.visible.clone()
        } else {
            (0..self.files.len()).collect()
        };
        let listed: Vec<usize> = if flat {
            Vec::new()
        } else {
            (0..self.bursts.len())
                .filter(|&i| !filtering || !visible_in(&self.visible, self.bursts[i].start, self.bursts[i].len).is_empty())
                .collect()
        };
        let cur_burst = self.burst_of.get(self.cur).copied();
        let (total, cur_pos) = if flat {
            (photos.len().div_ceil(per), photos.partition_point(|&i| i < self.cur) / per)
        } else {
            (listed.len(), cur_burst.map_or(0, |b| listed.partition_point(|&i| i < b)))
        };
        let group = Some((flat, cur_pos));
        if group != self.list_group {
            self.list_group = group;
            self.list_offset = 0;
        }
        // Ligne courante au premier tiers, décalable à la molette.
        let anchor = cur_pos as isize - fit / 3;
        let last_first = (total as isize - fit).max(0);
        let first = (anchor + self.list_offset).clamp(0, last_first);
        self.list_offset = first - anchor;
        let lines = first as usize..(first + fit).min(total as isize).max(first) as usize;
        let entries: Vec<Entry> = if flat {
            lines
                .map(|r| {
                    let cells = photos[r * per..((r + 1) * per).min(photos.len())]
                        .iter()
                        .map(|&index| {
                            let m = &self.marks[index];
                            Cell { index, rating: m.rating, rejected: m.rejected, color: m.color }
                        })
                        .collect();
                    Entry::Photos(cells)
                })
                .collect()
        } else {
            listed[lines]
                .iter()
                .map(|&i| {
                    let b = &self.bursts[i];
                    let all: Vec<usize>;
                    let indices: &[usize] = if filtering {
                        visible_in(&self.visible, b.start, b.len)
                    } else {
                        all = (b.start..b.start + b.len).collect();
                        &all
                    };
                    Entry::Burst(Row {
                        start: b.start,
                        len: b.len,
                        shown: filtering.then_some(indices.len()),
                        picks: panel::spread(indices),
                        first: indices[0],
                        number: i + 1,
                        rate: b.rate,
                        marked: self.marks[b.start..b.start + b.len]
                            .iter()
                            .filter(|m| m.retained())
                            .count(),
                        current: Some(i) == cur_burst,
                    })
                })
                .collect()
        };

        let f = self.filter;
        let filter_controls = Controls {
            rating: f.min_rating,
            colors: f.colors,
            check: f.retained,
            reject: f.rejected,
        };
        let filter_text = if filtering {
            format!("{} / {} photos", self.visible.len(), self.files.len())
        } else {
            "Filtre : toutes les photos".to_string()
        };
        let count = if filtering {
            self.visible.len()
        } else {
            self.marks.iter().filter(|m| m.retained()).count()
        };
        let what = if filtering { "filtrées" } else { "retenues" };
        let view = View {
            flat_available: self.files.len() < FLAT_MAX,
            flat,
            copy_kind: self.copy_kind,
            copy_label: format!("Copier {count} {what}"),
            copy_message: self.copy_message.clone(),
        };
        let key = format!("{width}x{height} {status} {filter_controls:?} {filter_text} {view:?} {entries:?}");
        if key != self.list_key {
            let list = self.panel.render_list(
                width,
                height,
                &filter_controls,
                &filter_text,
                &view,
                &entries,
                &self.metas,
                &status,
            );
            renderer.set_list(&list.pixels, list.width, list.height);
            self.list_hits = list.hits;
            self.bar_hits = list.bar_hits;
            self.tracks = list.tracks;
            self.list_key = key;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // Image courante : repère de position dans sa rafale, ou cadre autour de sa vignette.
        let cur = self.cur;
        let track = self.tracks.iter().find(|t| (t.start..t.start + t.len).contains(&cur));
        let frame = track.and_then(|t| t.cell).map(|(x, y, w, h)| Rect { x: lx + x, y: ly + y, w, h });
        let marker = track.filter(|t| t.cell.is_none()).map(|t| {
            let step = t.span / t.len as f32;
            Rect {
                x: lx + t.x + step * (cur - t.start) as f32,
                y: ly + t.y,
                w: step.max(3.0),
                h: 3.0,
            }
        });
        // Ascenseur, seulement si toutes les lignes ne tiennent pas.
        let total = total as f32;
        self.scrollbar = (fit > 0 && total > fit as f32).then(|| {
            let top = ly + status_h;
            let h = fit as f32 * row_h;
            let x = lx + width as f32 - panel::PADDING / 2.0 + 2.0;
            let thumb_h = (h * fit as f32 / total).max(24.0);
            let thumb_y = top + (h - thumb_h) * first as f32 / (total - fit as f32);
            Scrollbar {
                track: Rect { x, y: top, w: SCROLLBAR_WIDTH, h },
                thumb: Rect { x, y: thumb_y, w: SCROLLBAR_WIDTH, h: thumb_h },
                anchor,
                last_first,
            }
        });
        let changed = renderer.set_marker(marker)
            | renderer.set_frame(frame)
            | renderer.set_scrollbar(self.scrollbar.map(|s| (s.track, s.thumb)));
        if changed {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Appui sur l'ascenseur : commence le glissement (un clic sur la piste y amène le curseur).
    fn press_scrollbar(&mut self) -> bool {
        let (Some((cx, cy)), Some(s)) = (self.cursor, self.scrollbar) else {
            return false;
        };
        let t = s.track;
        if cx < t.x - SCROLLBAR_GRAB || cx > t.x + t.w + SCROLLBAR_GRAB || cy < t.y || cy > t.y + t.h {
            return false;
        }
        let grab = if (s.thumb.y..s.thumb.y + s.thumb.h).contains(&cy) {
            cy - s.thumb.y
        } else {
            s.thumb.h / 2.0
        };
        self.dragging = Some(grab);
        self.drag_scrollbar();
        true
    }

    fn drag_scrollbar(&mut self) {
        let (Some(grab), Some((_, cy)), Some(s)) = (self.dragging, self.cursor, self.scrollbar) else {
            return;
        };
        let range = (s.track.h - s.thumb.h).max(1.0);
        let frac = ((cy - grab - s.track.y) / range).clamp(0.0, 1.0);
        let first = (frac * s.last_first as f32).round() as isize;
        self.list_offset = first - s.anchor;
    }

    /// Clic dans le panneau : commandes de marquage, commandes du bas, ou ligne / vignette.
    fn click(&mut self) {
        let (Some((cx, cy)), Some(renderer)) = (self.cursor, &self.renderer) else {
            return;
        };
        let (lx, ly) = renderer.list_origin();
        if cx < lx {
            // Clic sur la photo : la loupe se fige à cet endroit (ou s'y déplace).
            if let Some(uv) = renderer.image_point((cx, cy)) {
                self.pin = Some(uv);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            return;
        }
        if let Some(h) = self.info_hits.iter().find(|h| h.contains((cx - lx, cy))) {
            return self.mark_widget(h.widget);
        }
        let local = (cx - lx, cy - ly);
        if let Some(h) = self.bar_hits.iter().find(|h| h.contains(local)) {
            return self.bar_widget(h.widget);
        }
        if let Some(Widget::Photo(i)) = self.list_hits.iter().find(|h| h.contains(local)).map(|h| h.widget) {
            self.jump(i, Instant::now());
        }
    }

    fn wheel(&mut self, delta: MouseScrollDelta) {
        let (Some((cx, _)), Some(renderer)) = (self.cursor, &self.renderer) else {
            return;
        };
        if cx < renderer.list_origin().0 {
            return;
        }
        let rows = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y.signum() as isize,
            MouseScrollDelta::PixelDelta(p) => -(p.y as f32 / self.list_row_h).round() as isize,
        };
        self.list_offset += rows;
    }

    fn update_title(&mut self) {
        let Some(window) = &self.window else { return };
        let name = self.files[self.cur]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let m = self.marks[self.cur];
        let mark = if m.rejected {
            "  ×".to_string()
        } else {
            format!("  {}", "★".repeat(m.rating as usize))
        };
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
        while let Ok((index, meta)) = self.scan_rx.try_recv() {
            self.add_meta(index, meta);
        }
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
        self.update_list();
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
                if let Some(r) = &mut self.renderer {
                    let loupe_at = match self.pin {
                        Some(uv) => r.screen_point(uv),
                        None => self.cursor,
                    }
                    .filter(|_| self.loupe_allowed);
                    // Pointeur masqué sous la loupe, pour ne pas cacher le visage.
                    let hide = r.render(loupe_at, self.pin.is_some());
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
                self.drag_scrollbar();
                if self.loupe_allowed && self.pin.is_none() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            // Fenêtre d'aide ouverte : elle est modale.
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::KeyboardInput { .. }
                if self.help_open() => {}
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state == ElementState::Released {
                    self.dragging = None;
                } else if !self.press_scrollbar() {
                    self.click();
                }
            }
            // Clic droit : libère la loupe figée.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                if self.pin.take().is_some() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
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
            WindowEvent::ModifiersChanged(m) => self.modifiers = m,
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

/// Recopie le marquage de chaque JPEG dans le XMP de son RAW (rattrapage au
/// démarrage). Renvoie, pour chaque fichier, si le XMP du RAW vient d'un autre logiciel.
fn sync_raws(marks: &[Mark], raws: &[Option<PathBuf>]) -> Vec<bool> {
    let mut synced = 0;
    let foreign = marks
        .iter()
        .zip(raws)
        .map(|(mark, raw)| {
            let Some(raw) = raw else { return false };
            let xmp = marks::sidecar(raw);
            if marks::is_foreign(&xmp) {
                return true;
            }
            // Un JPEG marqué par un autre logiciel n'est pas recopié.
            if !mark.foreign && (*mark != Mark::default() || marks::is_ours(&xmp)) {
                match marks::save_to(&xmp, mark) {
                    Ok(()) => synced += 1,
                    Err(e) => log::warn!("{} : {e}", xmp.display()),
                }
            }
            false
        })
        .collect();
    log::info!(
        "{} RAW trouvés, {synced} XMP de RAW synchronisés",
        raws.iter().flatten().count()
    );
    foreign
}

/// Met des fichiers dans le presse-papiers au format de Fichiers (Nautilus),
/// avec wl-copy, qui reste en arrière-plan pour servir le contenu.
fn clipboard_files(paths: &[PathBuf]) -> std::io::Result<()> {
    let mut text = String::from("copy");
    for p in paths {
        text.push('\n');
        text.push_str(&file_uri(&std::path::absolute(p)?));
    }
    let mut child = Command::new("wl-copy")
        .args(["--type", "x-special/gnome-copied-files"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    // wl-copy rend la main dès qu'il a pris le presse-papiers.
    let status = child.wait()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other(format!("wl-copy : {status}"))) }
}

/// URI `file://` d'un chemin absolu, avec les caractères spéciaux encodés.
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

/// Images visibles (liste triée) parmi `start..start + len`.
fn visible_in(visible: &[usize], start: usize, len: usize) -> &[usize] {
    let a = visible.partition_point(|&v| v < start);
    let b = visible.partition_point(|&v| v < start + len);
    &visible[a..b]
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
            "-V" | "--version" => {
                println!("rafale-viewer {}", env!("CARGO_PKG_VERSION"));
                return;
            }
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

#[cfg(test)]
mod tests {
    #[test]
    fn uri_encodee() {
        let p = std::path::Path::new("/home/a/Images/Pastré lumière/P#1.JPG");
        assert_eq!(super::file_uri(p), "file:///home/a/Images/Pastr%C3%A9%20lumi%C3%A8re/P%231.JPG");
    }
}

fn usage() -> ! {
    eprintln!("usage : rafale-viewer <dossier> [--fps N] | --version");
    std::process::exit(2);
}
