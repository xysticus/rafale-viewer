//! Essai : la navigation de Rafale avec une interface GTK4 / libadwaita (photo
//! dans un GtkPicture, panneau en widgets Adwaita), pour mesurer si GTK ralentit
//! le défilé. Même chargeur, même décodage, même logique de pas que la version
//! winit ; seul l'affichage change.
//!
//! rafale-gtk <dossier> [--fps N] [--bench-scroll S]
//!
//! Simplifications de l'essai : pas d'orientation EXIF, pas de loupe, pas de
//! liste des rafales ni de marquage ; le panneau est là pour son coût de mise à jour.

#[allow(dead_code)]
#[path = "../bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../loader.rs"]
mod loader;
#[allow(dead_code)]
#[path = "../meta.rs"]
mod meta;
#[allow(dead_code)]
#[path = "../sharp.rs"]
mod sharp;

use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::{gdk, glib};
use lru::LruCache;

use loader::{Decoded, Job, Loader, Quality};
use meta::Meta;

const HOLD_DELAY: Duration = Duration::from_millis(300);
const FULL_DELAY: Duration = Duration::from_millis(100);
const PREFETCH: usize = 10;

struct Ui {
    picture: gtk::Picture,
    title: adw::WindowTitle,
    name: gtk::Label,
    exposure: gtk::Label,
    position: gtk::Label,
}

struct State {
    files: Arc<Vec<PathBuf>>,
    loader: Loader,
    reduced: LruCache<usize, gdk::Texture>,
    full: LruCache<usize, gdk::Texture>,
    scan_rx: crossbeam_channel::Receiver<(usize, Meta)>,
    metas: Vec<Option<Meta>>,
    cur: usize,
    heading: isize,
    forward_held: bool,
    backward_held: bool,
    scrolling: isize,
    pending: isize,
    next_step_at: Instant,
    last_change: Instant,
    step_interval: Duration,
    shown: Option<Job>,
    shown_index: Option<usize>,
    bench: Option<bench::Bench>,
    ui: Ui,
}

impl State {
    fn is_ready(&self, i: usize) -> bool {
        self.reduced.contains(&i) || self.full.contains(&i)
    }

    fn neighbor(&self, from: usize, dir: isize, k: usize) -> Option<usize> {
        from.checked_add_signed(dir * k as isize).filter(|&i| i < self.files.len())
    }

    fn direction_key(&mut self, dir: isize, pressed: bool, now: Instant) {
        if pressed {
            // GTK répète l'appui tant que la touche est tenue : seul le premier compte.
            let held = if dir > 0 { &mut self.forward_held } else { &mut self.backward_held };
            if *held {
                return;
            }
            *held = true;
            self.pending += dir;
            self.scrolling = dir;
            self.next_step_at = now + HOLD_DELAY;
        } else {
            if dir > 0 {
                self.forward_held = false;
            } else {
                self.backward_held = false;
            }
            if self.scrolling == dir {
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
    }

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
            return;
        }
        if self.pending != 0 {
            self.pending -= dir;
        } else {
            let base = if now.duration_since(self.next_step_at) > self.step_interval { now } else { self.next_step_at };
            self.next_step_at = base + self.step_interval;
        }
        self.cur = target;
        self.heading = dir;
        self.last_change = now;
    }

    fn stationary(&self, now: Instant) -> bool {
        self.scrolling == 0 && self.pending == 0 && now >= self.last_change + FULL_DELAY
    }

    fn wanted(&self, now: Instant) -> Vec<Job> {
        let stationary = self.stationary(now);
        let mut jobs = Vec::new();
        let mut want = |i: Option<usize>, quality: Quality| {
            let Some(index) = i else { return };
            let cached = match quality {
                Quality::Reduced => self.is_ready(index),
                Quality::Full => self.full.contains(&index),
                Quality::Sharpness(_) => true,
            };
            if !cached {
                jobs.push(Job { index, quality });
            }
        };
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

    /// Une image de l'horloge de rendu de GTK : toute la logique de Rafale.
    fn tick(&mut self, app: &adw::Application) {
        let now = Instant::now();
        while let Ok((i, m)) = self.scan_rx.try_recv() {
            self.metas[i] = Some(m);
        }
        while let Some(done) = self.loader.try_recv() {
            let (i, quality) = (done.job.index, done.job.quality);
            if let Some(img) = done.image {
                let tex = texture(img);
                match quality {
                    Quality::Reduced => _ = self.reduced.put(i, tex),
                    _ => _ = self.full.put(i, tex),
                }
            }
        }
        self.step(now);
        let jobs = self.wanted(now);
        self.loader.set_wanted(jobs);

        let best = if let Some(t) = self.full.get(&self.cur) {
            Some((Quality::Full, t.clone()))
        } else {
            self.reduced.get(&self.cur).map(|t| (Quality::Reduced, t.clone()))
        };
        if let Some((quality, tex)) = best {
            let job = Job { index: self.cur, quality };
            if self.shown != Some(job) {
                self.ui.picture.set_paintable(Some(&tex));
                self.shown = Some(job);
                if let Some(b) = &mut self.bench {
                    b.on_shown(now, quality == Quality::Full);
                }
            }
        }
        if self.shown_index != Some(self.cur) {
            self.shown_index = Some(self.cur);
            self.update_panel();
        }

        if let Some(b) = &mut self.bench {
            b.on_frame(now);
        }
        if let Some(action) = self.bench.as_mut().map(|b| b.tick(now)) {
            match action {
                bench::Action::Press => self.direction_key(1, true, now),
                bench::Action::Release => self.direction_key(1, false, now),
                bench::Action::Exit => {
                    if let Some(b) = &self.bench {
                        println!("{}", b.report("GTK4 + libadwaita", self.step_interval));
                    }
                    self.bench = None;
                    app.quit();
                }
                bench::Action::None => {}
            }
        }
    }

    /// Le panneau : des étiquettes GTK changées à chaque photo, comme le panneau de Rafale.
    fn update_panel(&self) {
        let name = self.files[self.cur].file_name().unwrap_or_default().to_string_lossy().into_owned();
        self.ui.title.set_title(&name);
        self.ui.title.set_subtitle(&format!("{} / {}", self.cur + 1, self.files.len()));
        self.ui.name.set_text(&name);
        let meta = self.metas[self.cur].as_ref();
        self.ui.exposure.set_text(meta.map_or("", |m| m.exposure.as_str()));
        self.ui.position.set_text(&format!("{} / {}", self.cur + 1, self.files.len()));
    }
}

/// Image décodée → texture GDK, sans copie quand le chargeur ne la partage plus.
fn texture(img: Arc<Decoded>) -> gdk::Texture {
    let (w, h) = (img.width as i32, img.height as i32);
    let pixels = match Arc::try_unwrap(img) {
        Ok(d) => d.pixels,
        Err(shared) => shared.pixels.clone(),
    };
    let bytes = glib::Bytes::from_owned(pixels);
    gdk::MemoryTexture::new(w, h, gdk::MemoryFormat::R8g8b8a8, &bytes, 4 * w as usize).upcast()
}

fn build(app: &adw::Application, files: Vec<PathBuf>, fps: f64, bench: Option<bench::Bench>) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);

    // Panneau en widgets Adwaita.
    let side = gtk::Box::new(gtk::Orientation::Vertical, 12);
    side.set_width_request(440);
    side.set_margin_top(18);
    side.set_margin_bottom(18);
    side.set_margin_start(18);
    side.set_margin_end(18);
    let stars = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    for _ in 0..5 {
        let b = gtk::ToggleButton::new();
        b.set_icon_name("starred-symbolic");
        b.add_css_class("flat");
        stars.append(&b);
    }
    let name = gtk::Label::new(None);
    name.add_css_class("title-2");
    name.set_xalign(0.0);
    let exposure = gtk::Label::new(None);
    exposure.add_css_class("heading");
    exposure.set_xalign(0.0);
    let position = gtk::Label::new(None);
    position.add_css_class("dim-label");
    position.set_xalign(0.0);
    let kind = adw::ToggleGroup::new();
    for (label, id) in [("JPEG", "jpeg"), ("RAW", "raw"), ("Les deux", "both")] {
        kind.add(adw::Toggle::builder().label(label).name(id).build());
    }
    kind.set_active_name(Some("jpeg"));
    let copy = gtk::Button::with_label("Copier 26 retenues");
    copy.add_css_class("pill");
    copy.add_css_class("suggested-action");
    let view = adw::ToggleGroup::new();
    for (label, id) in [("Rafales", "bursts"), ("À plat", "flat")] {
        view.add(adw::Toggle::builder().label(label).name(id).build());
    }
    view.set_active_name(Some("bursts"));
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    for w in [
        stars.upcast_ref::<gtk::Widget>(),
        name.upcast_ref(),
        exposure.upcast_ref(),
        position.upcast_ref(),
        spacer.upcast_ref(),
        kind.upcast_ref(),
        copy.upcast_ref(),
        view.upcast_ref(),
    ] {
        side.append(w);
    }

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&picture);
    content.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    content.append(&side);

    let title = adw::WindowTitle::new("Rafale (essai GTK)", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));
    header.pack_end(&gtk::Button::from_icon_name("help-about-symbolic"));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Rafale (essai GTK)")
        .default_width(1600)
        .default_height(1000)
        .content(&toolbar)
        .build();

    let files = Arc::new(files);
    let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
    // L'horloge de rendu de GTK tourne en continu : pas besoin de réveil.
    let noop: loader::Waker = Arc::new(|| {});
    meta::spawn(files.clone(), scan_tx, noop.clone());
    let now = Instant::now();
    let n = files.len();
    let state = Rc::new(RefCell::new(State {
        loader: Loader::new(files.clone(), noop),
        files,
        reduced: LruCache::new(NonZeroUsize::new(64).unwrap()),
        full: LruCache::new(NonZeroUsize::new(6).unwrap()),
        scan_rx,
        metas: (0..n).map(|_| None).collect(),
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
        shown_index: None,
        bench,
        ui: Ui { picture: picture.clone(), title, name, exposure, position },
    }));

    let keys = gtk::EventControllerKey::new();
    {
        let state = state.clone();
        let app = app.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let now = Instant::now();
            match key {
                gdk::Key::Right | gdk::Key::Page_Down => state.borrow_mut().direction_key(1, true, now),
                gdk::Key::Left | gdk::Key::Page_Up => state.borrow_mut().direction_key(-1, true, now),
                gdk::Key::Escape | gdk::Key::q => app.quit(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        keys.connect_key_released(move |_, key, _, _| {
            let now = Instant::now();
            match key {
                gdk::Key::Right | gdk::Key::Page_Down => state.borrow_mut().direction_key(1, false, now),
                gdk::Key::Left | gdk::Key::Page_Up => state.borrow_mut().direction_key(-1, false, now),
                _ => {}
            }
        });
    }
    window.add_controller(keys);

    {
        let app = app.clone();
        picture.add_tick_callback(move |_, _| {
            state.borrow_mut().tick(&app);
            glib::ControlFlow::Continue
        });
    }
    window.maximize();
    window.present();
}

fn list_jpegs(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| {
        p.is_file()
            && p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
    });
    files.sort();
    files
}

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut dir = None;
    let mut fps = 25.0;
    let mut bench = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fps" => fps = args.next().and_then(|v| v.parse().ok()).expect("--fps N"),
            "--bench-scroll" => {
                let s: f64 = args.next().and_then(|v| v.parse().ok()).expect("--bench-scroll S");
                bench = Some(bench::Bench::new(Duration::from_secs_f64(s)));
            }
            _ => dir = Some(PathBuf::from(arg)),
        }
    }
    let files = list_jpegs(&dir.expect("usage : rafale-gtk <dossier> [--fps N] [--bench-scroll S]"));
    assert!(!files.is_empty(), "aucun JPEG");

    let app = adw::Application::builder().application_id("fr.xysticus.RafaleEssaiGtk").build();
    let bench = RefCell::new(bench);
    let files = RefCell::new(Some(files));
    app.connect_activate(move |app| {
        if let Some(files) = files.borrow_mut().take() {
            build(app, files, fps, bench.borrow_mut().take());
        }
    });
    // Arguments déjà lus : GTK n'en reçoit aucun.
    app.run_with_args::<&str>(&[])
}
