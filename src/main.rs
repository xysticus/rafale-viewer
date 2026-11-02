mod bench;
mod help;
mod import;
mod loader;
mod marks;
mod raw;
mod meta;
mod panel;
mod render;
mod retouch;
mod sharp;

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
use marks::{BurstFile, BurstMark, Color, Filter, Issue, Mark};
use meta::Meta;
use panel::{Cell, Controls, CopyKind, Entry, Hit, Line, Panel, Row, Track, View, Widget};
use render::{LoupeKind, Rect, Renderer};

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

/// Action qui attend la réponse d'une fenêtre zenity.
enum Pending {
    /// Fichiers à mettre à la corbeille si la confirmation est acceptée.
    Trash(Vec<PathBuf>),
    /// Choix du logiciel de retouche.
    Editor,
    /// Copie (Ctrl+C) ou retouche (avec ou sans choix du logiciel) confirmée
    /// malgré des rafales signalées.
    Copy,
    Retouch(bool),
}

struct Burst {
    start: usize,
    len: usize,
    /// Rythme mesuré, en images/s.
    rate: Option<f32>,
    mark: BurstMark,
    /// Fichier `.rafale` qui porte le marquage sur le disque.
    file: Option<PathBuf>,
}

impl Burst {
    fn range(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.len
    }
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
    dir: PathBuf,
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
    /// Indice de netteté sur la zone AF (calculé à l'arrêt, en tâche de fond), et
    /// calcul déjà fait (même s'il n'a rien donné : pas de zone AF exploitable).
    sharpness: Vec<Option<f32>>,
    sharp_done: Vec<bool>,
    /// Rafale où la loupe a été figée : elle est libérée quand on en sort.
    pin_burst: Option<usize>,
    /// Loupe AF (touche A) : la loupe se place sur la zone AF de chaque vue.
    af_loupe: bool,
    /// Cadre vert de la zone AF sur la photo (touche C).
    af_frame: bool,

    /// En-têtes analysés en arrière-plan, dans l'ordre des fichiers.
    scan_rx: crossbeam_channel::Receiver<(usize, Meta)>,
    metas: Vec<Option<Meta>>,
    scanned: usize,
    bursts: Vec<Burst>,
    /// Rafale de chaque fichier déjà analysé.
    burst_of: Vec<usize>,
    /// Fichiers `.rafale` lus au lancement, rattachés à leur rafale à la fin de l'analyse.
    burst_files: Vec<BurstFile>,
    /// Rafale bloquée (Entrée) : on ne parcourt qu'elle, et la liste montre ses vues.
    locked: Option<usize>,
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
    /// Action en attente de la réponse d'une fenêtre zenity, puis réponse reçue
    /// (acceptée, et ce que la fenêtre a écrit sur sa sortie).
    modal: Option<Pending>,
    answer: Option<(bool, String)>,
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
    /// Défilé automatique chronométré (--bench-scroll S).
    bench: Option<bench::Bench>,
}

impl App {
    fn new(dir: PathBuf, files: Vec<PathBuf>, start: usize, proxy: EventLoopProxy<()>, fps: f64, bench: Option<bench::Bench>) -> Self {
        let n = files.len();
        let files = Arc::new(files);
        let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
        meta::spawn(files.clone(), scan_tx, waker(&proxy));
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
        let burst_files = sidecars
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == marks::BURST_EXT))
            .filter_map(|p| marks::load_burst(p))
            .collect();
        let raws = raw::find(&dir, &files);
        let raw_foreign = sync_raws(&marks, &raws);
        let now = Instant::now();
        App {
            dir,
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
            cur: start,
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
            pin_burst: None,
            sharpness: vec![None; n],
            sharp_done: vec![false; n],
            af_loupe: false,
            af_frame: true,
            scan_rx,
            scanned: 0,
            bursts: Vec::new(),
            burst_of: Vec::new(),
            burst_files,
            locked: None,
            flat: true,
            flat_chosen: false,
            copy_kind: CopyKind::default(),
            copy_message: String::new(),
            modifiers: Modifiers::default(),
            help: None,
            modal: None,
            answer: None,
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
            bench,
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

    /// `k`-ième image visible après (`dir` = 1) ou avant (`dir` = -1) l'image `from`,
    /// sans sortir de la rafale bloquée.
    fn neighbor(&self, from: usize, dir: isize, k: usize) -> Option<usize> {
        let i = if !self.filter.active() {
            from.checked_add_signed(dir * k as isize).filter(|&i| i < self.files.len())
        } else if dir > 0 {
            let pos = self.visible.partition_point(|&v| v <= from);
            self.visible.get(pos + k - 1).copied()
        } else {
            let pos = self.visible.partition_point(|&v| v < from);
            pos.checked_sub(k).map(|p| self.visible[p])
        };
        i.filter(|&i| self.nav_range().contains(&i))
    }

    /// Images parcourues : la rafale bloquée, ou toute la séance.
    fn nav_range(&self) -> std::ops::Range<usize> {
        match self.locked {
            Some(b) => self.bursts[b].range(),
            None => 0..self.files.len(),
        }
    }

    /// Entrée : bloque la rafale de la photo affichée, ou la débloque.
    fn toggle_lock(&mut self) {
        self.locked = match self.locked {
            Some(_) => None,
            None => self.markable_burst(),
        };
        self.list_offset = 0;
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
            let shift = self.modifiers.state().shift_key();
            match c.to_lowercase().as_str() {
                "c" if pressed && ctrl => return self.copy_selection(),
                // Fonction cachée : seulement les XMP de la sélection, pour donner
                // les marquages à un logiciel qu'on essaie sur une copie des photos.
                "x" if pressed && ctrl => return self.copy_xmp(),
                _ if ctrl => return,
                "f" if pressed => return self.toggle_fullscreen(),
                "n" if pressed => return self.jump_sharpest(),
                "c" if pressed => {
                    self.af_frame = !self.af_frame;
                    return;
                }
                "a" if pressed => {
                    self.af_loupe = !self.af_loupe;
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
                "q" if pressed => return event_loop.exit(),
                // Rejet : X comme Lightroom, R comme darktable ; avec Maj, la rafale.
                "x" | "r" if pressed && shift => return self.edit_burst_mark(BurstMark::toggle_rejected),
                "x" | "r" if pressed => {
                    return self.edit_mark(|m| {
                        m.rejected = !m.rejected;
                        m.rating = 0;
                        m.picked = false;
                    });
                }
                _ => {}
            }
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let now = Instant::now();
        // Chiffre = étoiles de la photo ; avec Maj, de la rafale (0 efface).
        let shift = self.modifiers.state().shift_key();
        if let Some(stars) = digit(code).filter(|_| pressed) {
            return if shift { self.set_burst_rating(stars) } else { self.set_rating(stars) };
        }
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
                let r = self.nav_range();
                if let Some(i) = self.first_visible_in(r.start, r.len()) {
                    self.jump(i, now);
                }
            }
            KeyCode::End if pressed => {
                let r = self.nav_range();
                let target = if self.filter.active() {
                    visible_in(&self.visible, r.start, r.len()).last().copied()
                } else {
                    r.end.checked_sub(1)
                };
                if let Some(i) = target {
                    self.jump(i, now);
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter if pressed => self.toggle_lock(),
            KeyCode::Space if pressed && shift => self.edit_burst_mark(BurstMark::toggle_retained),
            KeyCode::Space if pressed => self.edit_mark(Mark::toggle_retained),
            KeyCode::F1 if pressed => self.toggle_color(Color::Red),
            KeyCode::F2 if pressed => self.toggle_color(Color::Yellow),
            KeyCode::F3 if pressed => self.toggle_color(Color::Green),
            KeyCode::F4 if pressed => self.toggle_color(Color::Blue),
            KeyCode::F5 if pressed => self.toggle_color(Color::Purple),
            KeyCode::F11 if pressed => self.toggle_fullscreen(),
            // Échap débloque d'abord la rafale, plutôt que de fermer Rafale.
            KeyCode::Escape if pressed && self.locked.is_some() => self.toggle_lock(),
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

    /// Ctrl+C, bouton de copie : après confirmation s'il reste des rafales signalées.
    fn copy_selection(&mut self) {
        if self.help_open() || self.confirm_bursts(Pending::Copy) {
            return;
        }
        self.copy_selection_now();
    }

    /// Met les fichiers de la sélection dans le presse-papiers, comme une copie
    /// dans Fichiers : un Ctrl+V dans un dossier les y copie.
    fn copy_selection_now(&mut self) {
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

    /// Ctrl+X : met dans le presse-papiers les XMP de la sélection (ceux des JPEG,
    /// des RAW ou les deux, selon le choix de copie), comme une copie dans Fichiers.
    fn copy_xmp(&mut self) {
        let mut paths = Vec::new();
        for i in self.selection() {
            if self.copy_kind != CopyKind::Raw {
                paths.push(marks::sidecar(&self.files[i]));
            }
            if self.copy_kind != CopyKind::Jpeg {
                paths.extend(self.raws[i].as_deref().map(marks::sidecar));
            }
        }
        paths.retain(|p| p.is_file());
        self.copy_message = if paths.is_empty() {
            "aucun XMP à copier".into()
        } else {
            match clipboard_files(&paths) {
                Ok(()) => format!("{} XMP copiés : Ctrl+V dans Fichiers", paths.len()),
                Err(e) => {
                    log::error!("presse-papiers : {e}");
                    format!("échec : {e}")
                }
            }
        };
        log::info!("copie des XMP : {}", self.copy_message);
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

    /// Fenêtre modale (aide, confirmation, choix) encore ouverte ? À sa fermeture,
    /// la réponse d'une action en attente est notée pour `update`.
    fn help_open(&mut self) -> bool {
        let Some(child) = self.help.as_mut() else { return false };
        let status = match child.try_wait() {
            Ok(None) => return true,
            Ok(Some(status)) => status.success(),
            Err(_) => false,
        };
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            use std::io::Read;
            let _ = stdout.read_to_string(&mut out);
        }
        self.help = None;
        if self.modal.is_some() {
            self.answer = Some((status, out.trim().to_owned()));
        }
        false
    }

    /// « Retoucher… » : copie la sélection à côté de la séance et y lance le
    /// logiciel retenu ; au premier usage, ou avec Maj, demande lequel.
    fn retouch(&mut self, choose: bool) {
        if self.help_open() || self.confirm_bursts(Pending::Retouch(choose)) {
            return;
        }
        self.retouch_now(choose);
    }

    fn retouch_now(&mut self, choose: bool) {
        match retouch::saved_editor().filter(|_| !choose) {
            Some(editor) => self.retouch_with(&editor),
            None if retouch::installed_editors().is_empty() => {
                self.copy_message = "aucun logiciel de retouche trouvé".into();
            }
            None => match retouch::chooser().stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
                Ok(c) => {
                    self.help = Some(c);
                    self.modal = Some(Pending::Editor);
                }
                Err(e) => log::warn!("zenity : {e}"),
            },
        }
    }

    fn retouch_with(&mut self, editor: &str) {
        let mut photos = Vec::new();
        for i in self.selection() {
            if self.copy_kind != CopyKind::Raw {
                photos.push(self.files[i].clone());
            }
            if self.copy_kind != CopyKind::Jpeg {
                photos.extend(self.raws[i].clone());
            }
        }
        if photos.is_empty() {
            self.copy_message = "sélection vide".into();
            return;
        }
        let dest = retouch::destination(&self.dir, editor);
        let folder = dest.file_name().unwrap_or_default().to_string_lossy().into_owned();
        self.copy_message = match retouch::copy(&photos, &dest).and_then(|c| retouch::launch(editor, &dest).map(|()| c)) {
            Ok(c) => {
                let already = if c.already > 0 { format!(" ({} déjà là)", c.already) } else { String::new() };
                format!("{} fichiers copiés{already} dans {folder} : {}", c.photos, retouch::editor_name(editor))
            }
            Err(e) => {
                log::error!("retouche : {e}");
                format!("échec : {e}")
            }
        };
        log::info!("retouche : {}", self.copy_message);
    }

    /// Tous les fichiers des photos `photos` : JPEG, RAW et fichiers
    /// d'accompagnement de chacun.
    fn photo_files(&self, photos: &[usize]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for &i in photos {
            files.extend(companions(&self.files[i]));
            if let Some(raw) = &self.raws[i] {
                files.extend(companions(raw));
            }
        }
        files.sort();
        files.dedup();
        files
    }

    /// Rafale que le bouton « Corbeille » vise : la rafale bloquée, si elle est rejetée.
    fn trash_burst(&self) -> Option<usize> {
        self.locked.filter(|&b| self.bursts[b].mark.rejected)
    }

    /// Bouton « Corbeille » : demande confirmation (fenêtre zenity, non bloquante).
    /// Il vise la rafale bloquée et rejetée, sinon les photos rejetées.
    fn ask_trash(&mut self) {
        if self.help_open() {
            return;
        }
        let (question, warning, files) = match self.trash_burst() {
            Some(b) => {
                let burst = &self.bursts[b];
                let photos: Vec<usize> = burst.range().collect();
                let mut files = self.photo_files(&photos);
                files.extend(burst.file.clone());
                let retained = photos.iter().filter(|&&i| self.marks[i].retained()).count();
                let warning = match retained {
                    0 => String::new(),
                    1 => "\n\nAttention : une photo de cette rafale est retenue.".to_string(),
                    n => format!("\n\nAttention : {n} photos de cette rafale sont retenues."),
                };
                (format!("Mettre à la corbeille la rafale {} ({} photos) ?", b + 1, burst.len), warning, files)
            }
            None => {
                let rejected: Vec<usize> = (0..self.files.len()).filter(|&i| self.marks[i].rejected).collect();
                let files = self.photo_files(&rejected);
                (format!("Mettre à la corbeille les {} photos rejetées ?", rejected.len()), String::new(), files)
            }
        };
        if files.is_empty() {
            return;
        }
        let is_photo = |p: &PathBuf| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| !["xmp", "pp3", marks::BURST_EXT].iter().any(|x| e.eq_ignore_ascii_case(x)))
        };
        let images = files.iter().filter(|p| is_photo(p)).count();
        let bytes: u64 = files.iter().filter_map(|p| p.metadata().ok()).map(|m| m.len()).sum();
        let text = format!(
            "{question}\n\n\
             {images} fichiers photo (JPEG et RAW) et {} fichiers d'accompagnement \
             (XMP, PP3, rafale), soit {:.0} Mo.{warning}\n\nIls restent récupérables dans la corbeille de GNOME.",
            files.len() - images,
            bytes as f64 / 1e6
        );
        let child = Command::new("zenity")
            .args(["--question", "--title", "Rafale", "--icon", "user-trash", "--text", &text])
            .args(["--ok-label", "Mettre à la corbeille", "--cancel-label", "Annuler"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(c) => {
                self.help = Some(c);
                self.modal = Some(Pending::Trash(files));
            }
            Err(e) => log::warn!("zenity : {e}"),
        }
    }

    /// Réponse d'une fenêtre zenity : corbeille confirmée, ou logiciel choisi.
    fn finish_pending(&mut self, event_loop: &ActiveEventLoop) {
        let (Some((ok, out)), Some(pending)) = (self.answer.take(), self.modal.take()) else {
            return;
        };
        match pending {
            Pending::Trash(files) if ok => self.finish_trash(files, event_loop),
            Pending::Editor if ok && !out.is_empty() => {
                retouch::save_editor(&out);
                self.retouch_with(&out);
            }
            Pending::Copy if ok => self.copy_selection_now(),
            Pending::Retouch(choose) if ok => self.retouch_now(choose),
            _ => {}
        }
    }

    /// Après la confirmation : corbeille, puis rechargement de la séance.
    fn finish_trash(&mut self, files: Vec<PathBuf>, event_loop: &ActiveEventLoop) {
        let status = Command::new("gio").arg("trash").arg("--").args(&files).status();
        match status {
            Ok(s) if s.success() => log::info!("{} fichiers mis à la corbeille", files.len()),
            Ok(s) => log::error!("gio trash : {s}"),
            Err(e) => log::error!("gio trash : {e}"),
        }
        // Repartir de la photo affichée, ou de la suivante qui reste.
        let start = self.files[self.cur..].iter().chain(self.files[..self.cur].iter().rev()).find(|p| p.exists()).cloned();
        self.reload(start, event_loop);
    }

    /// Relit le dossier (photos retirées) sans fermer la fenêtre ; garde le filtre,
    /// la vue et les réglages de copie et de loupe.
    fn reload(&mut self, start: Option<PathBuf>, event_loop: &ActiveEventLoop) {
        let files = list_jpegs(&self.dir).unwrap_or_default();
        if files.is_empty() {
            return event_loop.exit();
        }
        let start = start.and_then(|s| files.iter().position(|f| *f == s)).unwrap_or(0);
        let fps = 1.0 / self.step_interval.as_secs_f64();
        let mut next = App::new(self.dir.clone(), files, start, self.proxy.clone(), fps, None);
        next.window = self.window.take();
        next.renderer = self.renderer.take();
        next.filter = self.filter;
        next.flat = self.flat;
        next.flat_chosen = self.flat_chosen;
        next.copy_kind = self.copy_kind;
        next.af_loupe = self.af_loupe;
        next.af_frame = self.af_frame;
        next.refresh_visible();
        // L'ancien chargeur s'arrête (ses threads finissent leur tâche) à la suppression.
        *self = next;
        self.loader = Some(Loader::new(self.files.clone(), waker(&self.proxy)));
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Début de la rafale précédente ou suivante (qui a des images visibles) ;
    /// une rafale bloquée cède la place à celle-là, bloquée à son tour.
    fn burst_step(&mut self, dir: isize, now: Instant) {
        let Some(&b) = self.burst_of.get(self.cur) else {
            return;
        };
        let mut t = b;
        while let Some(next) = t.checked_add_signed(dir).filter(|&n| n < self.bursts.len()) {
            t = next;
            let burst = &self.bursts[t];
            if let Some(i) = self.first_visible_in(burst.start, burst.len) {
                if self.locked.is_some() {
                    self.locked = Some(t);
                }
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
                Quality::Sharpness(_) => self.sharp_done[index],
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
            // En dernier : netteté de la photo affichée, puis des autres vues de
            // la rafale, des plus proches aux plus lointaines.
            if let Some(&b) = self.burst_of.get(cur) {
                let burst = &self.bursts[b];
                let mut members: Vec<usize> = (burst.start..burst.start + burst.len).collect();
                members.sort_by_key(|&i| i.abs_diff(cur));
                for i in members {
                    if let Some(af) = self.metas[i].as_ref().and_then(|m| m.af) {
                        let af = af.map(|v| (v * 10_000.0).round() as u16);
                        want(Some(i), Quality::Sharpness(af));
                    }
                }
            }
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
        let was_retained = mark.retained();
        f(mark);
        let now_retained = mark.retained();
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
        // Retenir une photo retient sa rafale (et lève son rejet) ; l'inverse
        // n'est pas automatique.
        if now_retained && !was_retained {
            self.edit_burst_mark(|m| {
                m.picked = true;
                m.rejected = false;
            });
        }
    }

    /// Clic sur une commande de marquage (en haut du panneau).
    fn mark_widget(&mut self, widget: Widget) {
        match widget {
            Widget::Star(n) => {
                let n = if self.marks[self.cur].rating == n { 0 } else { n };
                self.set_rating(n);
            }
            Widget::Color(c) => self.toggle_color(c),
            Widget::Check => self.edit_mark(Mark::toggle_retained),
            Widget::Reject => self.edit_mark(|m| {
                m.rejected = !m.rejected;
                m.rating = 0;
                m.picked = false;
            }),
            Widget::BurstStar(n) => {
                let Some(b) = self.markable_burst() else { return };
                let n = if self.bursts[b].mark.rating == n { 0 } else { n };
                self.set_burst_rating(n);
            }
            Widget::BurstCheck => self.edit_burst_mark(BurstMark::toggle_retained),
            Widget::BurstReject => self.edit_burst_mark(BurstMark::toggle_rejected),
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
                if self.flat_view() {
                    self.locked = None;
                }
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
            Widget::Trash => return self.ask_trash(),
            Widget::Retouch => return self.retouch(self.modifiers.state().shift_key()),
            Widget::Photo(_) | Widget::BurstStar(_) | Widget::BurstCheck | Widget::BurstReject => return,
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

    /// Vues de la rafale de `i` qui passent le filtre.
    fn burst_members(&self, i: usize) -> Vec<usize> {
        let Some(&b) = self.burst_of.get(i) else { return Vec::new() };
        let burst = &self.bursts[b];
        if self.filter.active() {
            visible_in(&self.visible, burst.start, burst.len).to_vec()
        } else {
            (burst.start..burst.start + burst.len).collect()
        }
    }

    /// Netteté de la photo affichée, relative à la plus nette de sa rafale :
    /// (pourcentage, rang, vues mesurées, vues mesurables). None pour une vue isolée.
    fn sharpness_rank(&self) -> Option<(Option<(u32, usize)>, usize, usize)> {
        let members = self.burst_members(self.cur);
        if members.len() < 2 {
            return None;
        }
        let measurable = members.iter().filter(|&&i| self.metas[i].as_ref().is_some_and(|m| m.af.is_some())).count();
        if measurable == 0 {
            return None;
        }
        let scores: Vec<f32> = members.iter().filter_map(|&i| self.sharpness[i]).collect();
        let best = scores.iter().copied().fold(0.0f32, f32::max);
        let cur = self.sharpness[self.cur].filter(|_| best > 0.0).map(|s| {
            let rank = 1 + scores.iter().filter(|&&o| o > s).count();
            ((100.0 * s / best).round() as u32, rank)
        });
        Some((cur, scores.len(), measurable))
    }

    /// N : vue la plus nette (parmi celles déjà mesurées) de la rafale.
    fn jump_sharpest(&mut self) {
        let best = self
            .burst_members(self.cur)
            .into_iter()
            .filter_map(|i| Some((i, self.sharpness[i]?)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            self.jump(i, Instant::now());
        }
    }

    /// Zone AF de la photo affichée, en proportions de la photo.
    fn shown_af(&self) -> Option<[f32; 4]> {
        let index = self.shown?.index;
        self.metas[index].as_ref()?.af_display()
    }

    /// Où placer la loupe (pixels écran) et de quel type : figée, sur l'AF, ou
    /// sous le pointeur (la loupe AF revient au pointeur si la vue n'a pas de zone AF).
    fn loupe_target(&self) -> Option<((f32, f32), LoupeKind)> {
        let r = self.renderer.as_ref()?;
        if let Some(uv) = self.pin {
            return r.screen_point(uv).map(|p| (p, LoupeKind::Pinned));
        }
        if self.af_loupe {
            if let Some([x, y, w, h]) = self.shown_af() {
                return r.screen_point((x + w / 2.0, y + h / 2.0)).map(|p| (p, LoupeKind::Af));
            }
        }
        self.cursor.map(|p| (p, LoupeKind::Pointer))
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
        let (sharp_text, sharp_color) = match self.sharpness_rank() {
            None => (String::new(), [150, 150, 150]),
            Some((cur, done, total)) => {
                let progress = if done < total { format!("   (mesure {done} / {total})") } else { String::new() };
                match cur {
                    Some((pct, rank)) => (
                        format!("Netteté AF : {pct} % de la plus nette · rang {rank} / {total}{progress}"),
                        if rank == 1 { [110, 210, 120] } else { [175, 175, 175] },
                    ),
                    None if self.sharp_done[self.cur] => ("Netteté AF : non mesurable".to_string(), [150, 150, 150]),
                    None => (format!("Netteté AF : mesure…{progress}"), [150, 150, 150]),
                }
            }
        };
        let (issue, advice) = self
            .markable_burst()
            .and_then(|b| self.burst_issues(b).first().copied())
            .map(issue_text)
            .unwrap_or_default();
        let burst = self.markable_burst().map(|b| {
            let bm = self.bursts[b].mark;
            Controls { rating: bm.rating, colors: [false; 5], check: bm.retained(), reject: bm.rejected }
        });
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
                } else if self.af_loupe {
                    if meta.is_some_and(|m| m.af.is_some()) {
                        "Loupe sur la zone AF · A pour la couper".to_string()
                    } else {
                        "Loupe AF : pas de zone AF pour cette photo".to_string()
                    }
                } else {
                    String::new()
                },
                size: 16.0,
                color: [90, 160, 255],
                gap: 3.0,
            },
            Line { text: sharp_text, size: 16.0, color: sharp_color, gap: 3.0 },
            Line {
                text: format!("{} / {}", self.cur + 1, self.files.len()),
                size: 17.0,
                color: [150, 150, 150],
                gap: 3.0,
            },
            Line {
                text: match self.burst_of.get(self.cur) {
                    Some(&b) => format!(
                        "Rafale {}{}   vue {} / {}{}",
                        b + 1,
                        if self.locked == Some(b) { " bloquée" } else { "" },
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
            // Règle de cohérence non respectée par la rafale, et quoi faire.
            Line { text: issue, size: 16.0, color: panel::WARNING, gap: 8.0 },
            Line { text: advice, size: 16.0, color: [190, 160, 110], gap: 2.0 },
        ];
        let key = format!(
            "{width}|{controls:?}|{burst:?}|{}",
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("|")
        );
        if key == self.panel_key {
            return;
        }
        let (pixels, w, h, hits) = self.panel.render_info(width, &controls, burst.as_ref(), &lines);
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
                mark: BurstMark::default(),
                file: None,
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
            self.attach_burst_files();
            let rates: Vec<f32> = self.bursts.iter().filter_map(|b| b.rate).collect();
            log::info!(
                "{} rafales, rythme mesuré pour {} : {:?} i/s",
                self.bursts.len(),
                rates.len(),
                rates.iter().map(|r| format!("{r:.1}")).collect::<std::collections::BTreeSet<_>>()
            );
        }
    }

    /// Fin de l'analyse : chaque fichier `.rafale` va à la rafale qui contient sa
    /// première vue, ou à défaut sa dernière.
    fn attach_burst_files(&mut self) {
        for file in std::mem::take(&mut self.burst_files) {
            let Some(b) = burst_for(&self.files, &self.burst_of, &file.first, &file.last) else {
                log::warn!("{} : aucune de ses vues n'est dans la séance", file.path.display());
                continue;
            };
            let burst = &mut self.bursts[b];
            if burst.file.is_some() {
                log::warn!("{} : rafale {} déjà marquée par un autre fichier", file.path.display(), b + 1);
                continue;
            }
            burst.mark = file.mark;
            burst.file = Some(file.path);
        }
    }

    /// Rafale de la photo affichée, si son marquage est possible : vue par rafales,
    /// analyse terminée (les bornes des rafales sont connues).
    fn markable_burst(&self) -> Option<usize> {
        if self.flat_view() || self.scanned < self.files.len() {
            return None;
        }
        self.burst_of.get(self.cur).copied()
    }

    /// Modifie le marquage de la rafale affichée et l'enregistre dans son fichier `.rafale`.
    fn edit_burst_mark(&mut self, f: impl FnOnce(&mut BurstMark)) {
        let Some(b) = self.markable_burst() else { return };
        let burst = &mut self.bursts[b];
        let before = burst.mark;
        f(&mut burst.mark);
        if burst.mark == before {
            return;
        }
        let (first, last) = (&self.files[burst.start], &self.files[burst.start + burst.len - 1]);
        match marks::save_burst(burst.file.as_deref(), first, last, burst.len, &burst.mark) {
            Ok(file) => burst.file = file,
            Err(e) => log::error!("rafale {} : {e}", b + 1),
        }
    }

    /// Règles de cohérence non respectées par la rafale `b` (rien avant la fin
    /// de l'analyse : les bornes des rafales ne sont pas encore connues).
    fn burst_issues(&self, b: usize) -> Vec<Issue> {
        if self.scanned < self.files.len() {
            return Vec::new();
        }
        let burst = &self.bursts[b];
        marks::check_burst(&burst.mark, &self.marks[burst.range()])
    }

    /// Récapitulatif des rafales signalées, pour la confirmation de copie (vide s'il n'y en a pas).
    fn issues_summary(&self) -> String {
        let mut empty = Vec::new();
        let mut unmet = Vec::new();
        let mut rejected = Vec::new();
        for b in 0..self.bursts.len() {
            for issue in self.burst_issues(b) {
                match issue {
                    Issue::NoRetained => empty.push(b + 1),
                    Issue::RatingUnmet(_) => unmet.push(b + 1),
                    Issue::RejectedWithRetained(_) => rejected.push(b + 1),
                }
            }
        }
        let line = |numbers: &[usize], one: &str, many: &str| {
            let list = numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
            match numbers.len() {
                0 => None,
                1 => Some(format!("1 {one} : {list}")),
                n => Some(format!("{n} {many} : {list}")),
            }
        };
        [
            line(&empty, "rafale retenue sans photo retenue", "rafales retenues sans photo retenue"),
            line(&unmet, "rafale notée sans photo à sa note", "rafales notées sans photo à leur note"),
            line(&rejected, "rafale rejetée avec des photos retenues", "rafales rejetées avec des photos retenues"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// Avant une copie ou une retouche : s'il reste des rafales signalées, ouvre
    /// la confirmation et renvoie true (l'action attend la réponse).
    fn confirm_bursts(&mut self, pending: Pending) -> bool {
        let summary = self.issues_summary();
        if summary.is_empty() {
            return false;
        }
        let text = format!("Des rafales ne sont pas cohérentes avec leurs photos :\n\n{summary}");
        let child = Command::new("zenity")
            .args(["--question", "--title", "Rafale", "--icon", "dialog-warning", "--text", &text])
            .args(["--ok-label", "Copier quand même", "--cancel-label", "Annuler"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(c) => {
                self.help = Some(c);
                self.modal = Some(pending);
                true
            }
            Err(e) => {
                log::warn!("zenity : {e}");
                false
            }
        }
    }

    fn set_burst_rating(&mut self, stars: u8) {
        self.edit_burst_mark(|m| m.set_rating(stars));
    }

    /// Liste autour de la ligne courante (rafales, ou rangées de photos en vue à
    /// plat) ; recomposée seulement si elle change.
    fn update_list(&mut self) {
        let members = self.locked.map(|b| self.burst_members(self.bursts[b].start)).unwrap_or_default();
        let trash_burst = self.trash_burst();
        let warned: Vec<bool> = (0..self.bursts.len()).map(|b| !self.burst_issues(b).is_empty()).collect();
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let width = renderer.panel_width();
        let (lx, ly) = renderer.list_origin();
        let (_, wh) = renderer.window_size();
        let height = (wh - ly).max(0.0) as u32;
        let locked = self.locked;
        let mut status = if self.scanned < self.files.len() {
            format!("Analyse des rafales…  {} / {}", self.scanned, self.files.len())
        } else {
            String::new()
        };
        // Ligne d'état en haut (pendant l'analyse, ou rafale bloquée), filtre et boutons en bas.
        let status_h = if status.is_empty() && locked.is_none() { 0.0 } else { panel::PADDING };
        let header = panel::BOTTOM_BAR + status_h;
        let flat = self.flat && self.files.len() < FLAT_MAX;
        // Grille de photos : vue à plat, ou vues de la rafale bloquée.
        let grid = flat || locked.is_some();
        let row_h = if grid { Panel::grid_row_height(width) } else { Panel::row_height(width) };
        self.list_row_h = row_h;
        let fit = ((height as f32 - header) / row_h).max(0.0) as isize;
        let filtering = self.filter.active();
        let per = panel::THUMBS_PER_ROW;

        // Rafale bloquée : ses vues visibles, toutes si elles tiennent, sinon
        // régulièrement espacées. Vue à plat : les photos visibles, par rangées.
        // Sinon : les rafales qui ont au moins une image visible.
        let photos: Vec<usize> = if let Some(b) = locked {
            let room = fit.max(1) as usize * per;
            let shown = if members.len() > room { panel::spread(&members, room) } else { members.clone() };
            status = if shown.len() < members.len() {
                format!("Rafale {} bloquée · {} vues sur {} · Entrée pour débloquer", b + 1, shown.len(), members.len())
            } else {
                format!("Rafale {} bloquée · Entrée pour débloquer", b + 1)
            };
            shown
        } else if !flat {
            Vec::new()
        } else if filtering {
            self.visible.clone()
        } else {
            (0..self.files.len()).collect()
        };
        let listed: Vec<usize> = if grid {
            Vec::new()
        } else {
            (0..self.bursts.len())
                .filter(|&i| !filtering || !visible_in(&self.visible, self.bursts[i].start, self.bursts[i].len).is_empty())
                .collect()
        };
        let cur_burst = self.burst_of.get(self.cur).copied();
        let (total, cur_pos) = if grid {
            (photos.len().div_ceil(per), photos.partition_point(|&i| i < self.cur) / per)
        } else {
            (listed.len(), cur_burst.map_or(0, |b| listed.partition_point(|&i| i < b)))
        };
        let group = Some((grid, cur_pos));
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
        let entries: Vec<Entry> = if grid {
            // Vues espacées : chaque vignette tient lieu des vues jusqu'à la suivante,
            // et prend le cadre quand on les parcourt.
            let end = locked.map_or(0, |b| self.bursts[b].range().end);
            lines
                .map(|r| {
                    let cells = (r * per..((r + 1) * per).min(photos.len()))
                        .map(|k| {
                            let index = photos[k];
                            let span = if locked.is_some() { photos.get(k + 1).copied().unwrap_or(end) - index } else { 1 };
                            let m = &self.marks[index];
                            Cell { index, span, rating: m.rating, rejected: m.rejected, picked: m.retained(), color: m.color }
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
                        picks: panel::spread(indices, panel::THUMBS_PER_ROW),
                        first: indices[0],
                        number: i + 1,
                        rate: b.rate,
                        marked: self.marks[b.start..b.start + b.len]
                            .iter()
                            .filter(|m| m.retained())
                            .count(),
                        mark: b.mark,
                        warn: warned[i],
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
        let rejected = self.marks.iter().filter(|m| m.rejected).count();
        let view = View {
            trash_label: match trash_burst {
                Some(b) => Some(format!("Corbeille : rafale {} ({} photos)", b + 1, self.bursts[b].len)),
                None => (rejected > 0).then(|| format!("Corbeille : {rejected} rejetée{}", if rejected > 1 { "s" } else { "" })),
            },
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
                self.pin_burst = self.burst_of.get(self.cur).copied();
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
            format!("  {}{}", "★".repeat(m.rating as usize), if m.picked { "  ✓" } else { "" })
        };
        let quality = match self.shown {
            Some(Job { index, quality: Quality::Full }) if index == self.cur => "",
            Some(Job { index, .. }) if index == self.cur => "  · réduit",
            _ => "  · chargement…",
        };
        let locked = self.locked.map(|b| format!("  · rafale {} bloquée", b + 1)).unwrap_or_default();
        let title = format!("{name}  {}/{}{mark}{locked}{quality}", self.cur + 1, self.files.len());
        // set_title redessine les décorations, ce qui relance la boucle : seulement si changé.
        if title != self.title {
            window.set_title(&title);
            self.title = title;
        }
    }

    /// Une itération de la logique, après chaque lot d'événements.
    fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.help_open();
        if self.answer.is_some() {
            return self.finish_pending(event_loop);
        }
        while let Ok((index, meta)) = self.scan_rx.try_recv() {
            self.add_meta(index, meta);
        }
        let Some(loader) = &self.loader else { return };
        while let Some(done) = loader.try_recv() {
            let i = done.job.index;
            match (done.job.quality, done.image) {
                (Quality::Reduced, Some(img)) => _ = self.reduced.put(i, img),
                (Quality::Full, Some(img)) => _ = self.full.put(i, img),
                _ => {
                    self.sharpness[i] = done.score;
                    self.sharp_done[i] = true;
                }
            }
        }

        let now = Instant::now();
        self.step(now);
        // Autre rafale, autre cadrage : la loupe figée est libérée.
        if self.pin.is_some() && self.burst_of.get(self.cur).copied() != self.pin_burst {
            self.pin = None;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
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
                if let Some(b) = &mut self.bench {
                    b.on_shown(now, quality == Quality::Full);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
        let af = self.shown_af().filter(|_| self.af_frame);
        if let Some(r) = &mut self.renderer {
            if r.set_af(af) {
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

        // Défilé automatique : appui et relâchement simulés.
        let mut deadline = None;
        if let Some(action) = self.bench.as_mut().map(|b| b.tick(now)) {
            match action {
                bench::Action::Press => {
                    self.forward_held = true;
                    self.direction_key(1, true, now);
                }
                bench::Action::Release => {
                    self.forward_held = false;
                    self.direction_key(1, false, now);
                }
                bench::Action::Exit => {
                    if let Some(b) = &self.bench {
                        println!("{}", b.report("winit + wgpu", self.step_interval));
                    }
                    event_loop.exit();
                }
                bench::Action::None => {}
            }
            deadline = self.bench.as_ref().map(|b| now + b.poll_interval());
        }
        // Prochain réveil : pas de défilé, ou lancement de la pleine résolution.
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
                    {
                        // Identifiant d'application = nom du .desktop : GNOME associe
                        // la fenêtre à son icône.
                        use winit::platform::wayland::WindowAttributesExtWayland;
                        Window::default_attributes()
                            .with_title("rafale")
                            .with_maximized(true)
                            .with_name("rafale-viewer", "rafale-viewer")
                    },
                )
                .expect("création de la fenêtre"),
        );
        self.renderer = Some(Renderer::new(
            window.clone(),
            event_loop.owned_display_handle(),
        ));
        self.window = Some(window);
        self.loader = Some(Loader::new(self.files.clone(), waker(&self.proxy)));
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
                let target = self.loupe_target().filter(|_| self.loupe_allowed);
                let kind = target.map_or(LoupeKind::Pointer, |(_, k)| k);
                if let Some(r) = &mut self.renderer {
                    // Pointeur masqué sous la loupe, pour ne pas cacher le visage.
                    let hide = r.render(target.map(|(p, _)| p), kind);
                    if let Some(b) = &mut self.bench {
                        b.on_frame(Instant::now());
                    }
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
                let follows = matches!(self.loupe_target(), Some((_, LoupeKind::Pointer)));
                if self.loupe_allowed && follows {
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
            WindowEvent::Focused(false) if self.bench.is_none() => {
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
        // Reprise à cette photo à la prochaine ouverture du dossier (pas en mesure).
        if self.bench.is_none() {
            if let Some(name) = self.files[self.cur].file_name().and_then(|n| n.to_str()) {
                positions::save(&self.dir, name);
            }
        }
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

/// Réveil de la boucle winit, pour le chargeur et l'analyse des en-têtes.
fn waker(proxy: &EventLoopProxy<()>) -> loader::Waker {
    let proxy = std::sync::Mutex::new(proxy.clone());
    Arc::new(move || {
        let _ = proxy.lock().unwrap().send_event(());
    })
}

/// Une photo et ses fichiers d'accompagnement, dans son dossier : `NOM.EXT`,
/// `NOM.EXT.xmp` (Rafale, darktable, digiKam…), `NOM.EXT_01.xmp` (doublons
/// darktable), `NOM.xmp` (autres logiciels) et `NOM.EXT.pp3` (RawTherapee).
fn companions(photo: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name), Some(stem)) = (
        photo.parent(),
        photo.file_name().and_then(|n| n.to_str()),
        photo.file_stem().and_then(|s| s.to_str()),
    ) else {
        return vec![photo.to_path_buf()];
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![photo.to_path_buf()];
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.file_name().to_str().is_some_and(|n| is_companion(n, name, stem)))
        .map(|e| e.path())
        .collect()
}

fn is_companion(file: &str, name: &str, stem: &str) -> bool {
    if file == name || file.eq_ignore_ascii_case(&format!("{stem}.xmp")) {
        return true;
    }
    let Some(rest) = file.strip_prefix(name) else {
        return false;
    };
    let rest = rest.to_ascii_lowercase();
    let rest = match rest.strip_prefix('_') {
        Some(r) => r.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => rest.as_str(),
    };
    rest == ".xmp" || rest == ".pp3"
}

/// Dernière photo vue dans chaque dossier, gardée hors du dossier des photos :
/// `~/.local/state/rafale/positions`, une ligne « dossier<TAB>photo » par dossier,
/// la plus récente en tête.
mod positions {
    use std::path::{Path, PathBuf};

    const KEEP: usize = 200;

    fn file() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        Some(base.join("rafale/positions"))
    }

    fn key(dir: &Path) -> Option<String> {
        let dir = std::fs::canonicalize(dir).ok()?;
        dir.to_str().filter(|s| !s.contains(['\t', '\n'])).map(str::to_owned)
    }

    fn read() -> Vec<(String, String)> {
        let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
        text.lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(d, n)| (d.to_owned(), n.to_owned()))
            .collect()
    }

    pub fn load(dir: &Path) -> Option<String> {
        let key = key(dir)?;
        read().into_iter().find(|(d, _)| *d == key).map(|(_, n)| n)
    }

    pub fn save(dir: &Path, name: &str) {
        let (Some(key), Some(file)) = (key(dir), file()) else { return };
        if name.contains(['\t', '\n']) {
            return;
        }
        let mut lines = vec![(key.clone(), name.to_owned())];
        lines.extend(read().into_iter().filter(|(d, _)| *d != key).take(KEEP - 1));
        let text: String = lines.iter().map(|(d, n)| format!("{d}\t{n}\n")).collect();
        let ok = file.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok())
            && std::fs::write(file.with_extension("tmp"), text).is_ok()
            && std::fs::rename(file.with_extension("tmp"), &file).is_ok();
        if !ok {
            log::warn!("position non enregistrée dans {}", file.display());
        }
    }
}

/// Images visibles (liste triée) parmi `start..start + len`.
/// Signalement d'une rafale dans le panneau (le ⚠ de tête est dessiné), et
/// ce qu'il faut faire pour le lever.
fn issue_text(issue: Issue) -> (String, String) {
    let plural = |n: usize| if n > 1 { "s" } else { "" };
    match issue {
        Issue::NoRetained => (
            "⚠ Rafale retenue : aucune photo retenue.".into(),
            "Coche la meilleure (Espace), ou décoche la rafale (Maj+Espace).".into(),
        ),
        Issue::RatingUnmet(n) => (
            format!("⚠ Rafale notée {n} : aucune photo à {n} étoile{} ou plus.", plural(n as usize)),
            "Trouve-la, ou baisse la note de la rafale.".into(),
        ),
        Issue::RejectedWithRetained(n) => (
            format!("⚠ Rafale rejetée : {n} photo{} retenue{}.", plural(n), plural(n)),
            format!("Décoche-l{}, ou réintègre la rafale (Maj+X).", if n > 1 { "es" } else { "a" }),
        ),
    }
}

/// Étoiles d'une touche chiffre (rangée du haut ou pavé numérique).
fn digit(code: KeyCode) -> Option<u8> {
    Some(match code {
        KeyCode::Digit0 | KeyCode::Numpad0 => 0,
        KeyCode::Digit1 | KeyCode::Numpad1 => 1,
        KeyCode::Digit2 | KeyCode::Numpad2 => 2,
        KeyCode::Digit3 | KeyCode::Numpad3 => 3,
        KeyCode::Digit4 | KeyCode::Numpad4 => 4,
        KeyCode::Digit5 | KeyCode::Numpad5 => 5,
        _ => return None,
    })
}

/// Rafale qui contient la vue nommée `first`, ou à défaut `last`.
fn burst_for(files: &[PathBuf], burst_of: &[usize], first: &str, last: &str) -> Option<usize> {
    let find = |name: &str| files.iter().position(|f| f.file_name().is_some_and(|n| n == name));
    let i = find(first).or_else(|| find(last))?;
    burst_of.get(i).copied()
}

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

    let mut target = None;
    let mut fps = DEFAULT_FPS;
    let mut bench = None;
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
            "--bench-scroll" => {
                let secs: f64 = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage());
                bench = Some(bench::Bench::new(Duration::from_secs_f64(secs)));
            }
            "-h" | "--help" => usage(),
            "-V" | "--version" => {
                println!("rafale-viewer {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            _ if target.is_none() => target = Some(PathBuf::from(arg)),
            _ => usage(),
        }
    }
    // Sans argument (lancé depuis le menu) : choix du dossier.
    let Some(target) = target.or_else(pick_folder) else {
        return;
    };
    // Une carte mémoire (dossier DCIM) : import, puis ouverture du jour importé.
    let target = match target.is_dir().then(|| import::find_dcim(&target)).flatten() {
        Some(dcim) => match import_card(&dcim) {
            Some(day) => day,
            None => return,
        },
        None => target,
    };
    // Une photo donnée : son dossier, en commençant par elle.
    let (dir, start_file) = if target.is_file() {
        let dir = target.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        (dir, Some(target))
    } else {
        (target, None)
    };
    let files = match list_jpegs(&dir) {
        Ok(f) if !f.is_empty() => f,
        Ok(_) => fail(&format!("Aucune photo JPEG dans {}", dir.display())),
        Err(e) => fail(&format!("{} : {e}", dir.display())),
    };
    // Départ : la photo demandée, sinon la dernière vue dans ce dossier, sinon la première.
    let wanted = match start_file {
        Some(f) => f.file_name().map(|n| n.to_os_string()),
        None if bench.is_none() => positions::load(&dir).map(Into::into),
        None => None,
    };
    let start = wanted
        .and_then(|name| files.iter().position(|p| p.file_name() == Some(name.as_os_str())))
        .unwrap_or(0);
    log::info!("{} JPEG dans {}", files.len(), dir.display());

    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .expect("boucle d'événements");
    let mut app = App::new(dir, files, start, event_loop.create_proxy(), fps, bench);
    event_loop.run_app(&mut app).expect("boucle d'événements");
}

/// Import d'une carte : résumé et confirmation, copie avec progression.
/// Renvoie le dossier du jour à ouvrir ensuite, ou None (annulé, rien à ouvrir).
fn import_card(dcim: &Path) -> Option<PathBuf> {
    let plan = import::plan(dcim, &import::pictures_dir());
    let latest = plan.latest_day();
    let ask = |text: &str, ok: &str| {
        Command::new("zenity")
            .args(["--question", "--no-markup", "--title", "Rafale — carte mémoire", "--width", "560"])
            .args(["--text", text, "--ok-label", ok, "--cancel-label", "Annuler"])
            .status()
            .is_ok_and(|s| s.success())
    };
    if plan.is_empty() {
        let open = latest.as_ref().filter(|d| d.is_dir())?;
        return ask(&format!("{}\n\nOuvrir {} ?", plan.summary(), open.display()), "Ouvrir").then(|| open.clone());
    }
    if !ask(&plan.summary(), "Importer") {
        return None;
    }
    match plan.run() {
        Ok(n) => {
            log::info!("{n} fichiers importés");
            latest
        }
        Err(e) if e == "annulé" => {
            let _ = Command::new("zenity")
                .args(["--info", "--no-markup", "--title", "Rafale", "--text", "Import interrompu. Les fichiers déjà copiés sont complets ; relancer l'import copiera le reste."])
                .status();
            None
        }
        Err(e) => fail(&format!("Import : {e}")),
    }
}

/// Choix d'un dossier de photos (zenity) ; None si l'utilisateur annule.
fn pick_folder() -> Option<PathBuf> {
    let out = Command::new("zenity")
        .args(["--file-selection", "--directory", "--title", "Rafale — dossier de photos à trier"])
        .output()
        .map_err(|e| log::error!("zenity : {e}"))
        .ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    let path = path.trim_end_matches('\n');
    (out.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

/// Erreur fatale : sur le terminal, et dans une fenêtre si on a été lancé sans terminal.
fn fail(message: &str) -> ! {
    use std::io::IsTerminal;
    eprintln!("{message}");
    if !std::io::stderr().is_terminal() {
        let _ = Command::new("zenity").args(["--error", "--title", "Rafale", "--text", message]).status();
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    #[test]
    fn fichiers_d_accompagnement() {
        use super::is_companion as c;
        let (n, s) = ("PA290001.ORF", "PA290001");
        for f in ["PA290001.ORF", "PA290001.ORF.xmp", "PA290001.ORF_01.xmp", "PA290001.xmp", "PA290001.ORF.pp3", "PA290001.ORF.XMP"] {
            assert!(c(f, n, s), "{f}");
        }
        for f in ["PA290002.ORF", "PA290001.JPG", "PA290001.ORF.jpg", "PA2900011.ORF.xmp", "PA290001_v1.JPG", "PA290001.ORF_x.xmp"] {
            assert!(!c(f, n, s), "{f}");
        }
    }

    #[test]
    fn position_par_dossier() {
        let base = std::env::temp_dir().join(format!("rafale-positions-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        // Seul test à toucher à cette variable.
        unsafe { std::env::set_var("XDG_STATE_HOME", base.join("state")) };
        assert_eq!(super::positions::load(&a), None);
        super::positions::save(&a, "P1.JPG");
        super::positions::save(&b, "P2.JPG");
        super::positions::save(&a, "P3.JPG");
        assert_eq!(super::positions::load(&a).as_deref(), Some("P3.JPG"));
        assert_eq!(super::positions::load(&b).as_deref(), Some("P2.JPG"));
        let text = std::fs::read_to_string(base.join("state/rafale/positions")).unwrap();
        assert_eq!(text.lines().count(), 2, "une ligne par dossier, la plus récente en tête : {text}");
        assert!(text.starts_with(a.canonicalize().unwrap().to_str().unwrap()));
    }

    #[test]
    fn uri_encodee() {
        let p = std::path::Path::new("/home/a/Images/Pastré lumière/P#1.JPG");
        assert_eq!(super::file_uri(p), "file:///home/a/Images/Pastr%C3%A9%20lumi%C3%A8re/P%231.JPG");
    }

    #[test]
    fn fichier_rafale_rattache() {
        use super::burst_for;
        use std::path::PathBuf;
        let files: Vec<PathBuf> = ["P1.JPG", "P2.JPG", "P3.JPG", "P4.JPG"].iter().map(|n| PathBuf::from("/s").join(n)).collect();
        let burst_of = [0, 0, 1, 1];
        assert_eq!(burst_for(&files, &burst_of, "P3.JPG", "P4.JPG"), Some(1));
        // Première vue supprimée : la dernière désigne la rafale.
        assert_eq!(burst_for(&files, &burst_of, "P0.JPG", "P2.JPG"), Some(0));
        assert_eq!(burst_for(&files, &burst_of, "P0.JPG", "P9.JPG"), None);
    }
}

fn usage() -> ! {
    eprintln!("usage : rafale-viewer [dossier | photo.jpg] [--fps N] [--bench-scroll S] | --version");
    std::process::exit(2);
}
