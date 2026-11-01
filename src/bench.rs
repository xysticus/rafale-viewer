//! Défilé automatique chronométré (`--bench-scroll S`), commun aux versions
//! winit et GTK : attend que la photo de départ soit en pleine résolution et
//! que le préchargement ait eu le temps de se faire, « maintient » → pendant
//! S secondes, « relâche », attend la pleine résolution, puis rend compte.

use std::time::{Duration, Instant};

/// Délai après la première pleine résolution avant d'appuyer (préchargement ±10).
const WARMUP: Duration = Duration::from_millis(1500);
/// Début du défilé ignoré dans la cadence (appui bref puis délai de maintien).
const RAMP: Duration = Duration::from_millis(600);
const SETTLE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    Press,
    Release,
    Exit,
}

enum Phase {
    /// En attente de la première pleine résolution, puis du préchargement.
    Waiting(Option<Instant>),
    Holding(Instant),
    Settling(Instant),
    Done,
}

pub struct Bench {
    hold: Duration,
    phase: Phase,
    /// Instants où une nouvelle image a été mise à l'écran pendant le maintien.
    shown: Vec<Instant>,
    press_at: Option<Instant>,
    full_latency: Option<Duration>,
    /// Images d'écran (horloge de rendu), pendant le maintien et après : pour
    /// repérer les images d'écran manquées (GTK, rendu continu).
    frames: Vec<Instant>,
}

impl Bench {
    pub fn new(hold: Duration) -> Self {
        Bench { hold, phase: Phase::Waiting(None), shown: Vec::new(), press_at: None, full_latency: None, frames: Vec::new() }
    }

    /// Une nouvelle image vient d'être donnée à l'affichage.
    pub fn on_shown(&mut self, now: Instant, full: bool) {
        match self.phase {
            Phase::Waiting(None) if full => self.phase = Phase::Waiting(Some(now)),
            Phase::Holding(_) => self.shown.push(now),
            Phase::Settling(t) if full => {
                self.full_latency = Some(now - t);
                self.phase = Phase::Done;
            }
            _ => {}
        }
    }

    /// Une image d'écran vient d'être produite.
    pub fn on_frame(&mut self, now: Instant) {
        if matches!(self.phase, Phase::Holding(_) | Phase::Settling(_)) {
            self.frames.push(now);
        }
    }

    /// À appeler à chaque mise à jour ; dit quoi simuler.
    pub fn tick(&mut self, now: Instant) -> Action {
        match self.phase {
            Phase::Waiting(Some(t)) if now >= t + WARMUP => {
                self.phase = Phase::Holding(now);
                self.press_at = Some(now);
                Action::Press
            }
            Phase::Holding(t) if now >= t + self.hold => {
                self.phase = Phase::Settling(now);
                Action::Release
            }
            Phase::Settling(t) if now >= t + SETTLE_TIMEOUT => {
                self.phase = Phase::Done;
                Action::Exit
            }
            Phase::Done => Action::Exit,
            _ => Action::None,
        }
    }

    /// Prochain réveil nécessaire (la boucle doit tourner pendant la mesure).
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(5)
    }

    pub fn report(&self, name: &str, target: Duration) -> String {
        let Some(press) = self.press_at else { return format!("{name} : mesure non commencée") };
        let steady: Vec<Instant> = self.shown.iter().copied().filter(|&t| t >= press + RAMP).collect();
        let mut out = format!("=== {name} : défilé maintenu {:.1} s, {} images affichées\n", self.hold.as_secs_f64(), self.shown.len());
        if steady.len() >= 2 {
            let span = (*steady.last().unwrap() - steady[0]).as_secs_f64();
            let mut gaps: Vec<f64> = steady.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
            gaps.sort_by(f64::total_cmp);
            let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
            let p = |q: f64| gaps[((gaps.len() - 1) as f64 * q).round() as usize];
            let jerks = gaps.iter().filter(|&&g| g > 1.5 * target.as_secs_f64() * 1000.0).count();
            out += &format!(
                "  cadence établie : {:.1} images/s (cible {:.1})\n  intervalle : moyen {mean:.1} ms, médian {:.1}, p95 {:.1}, max {:.1}\n  à-coups (> 1,5 × {:.0} ms) : {jerks} sur {}\n",
                (steady.len() - 1) as f64 / span,
                1.0 / target.as_secs_f64(),
                p(0.5),
                p(0.95),
                gaps[gaps.len() - 1],
                target.as_secs_f64() * 1000.0,
                gaps.len(),
            );
        }
        if self.frames.len() >= 3 {
            let mut gaps: Vec<f64> = self.frames.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
            gaps.sort_by(f64::total_cmp);
            let median = gaps[gaps.len() / 2];
            let missed = gaps.iter().filter(|&&g| g > 1.5 * median).count();
            out += &format!(
                "  images d'écran : {}, intervalle médian {median:.1} ms, max {:.1} ms, manquées (> 1,5 × médian) : {missed}\n",
                self.frames.len(),
                gaps[gaps.len() - 1],
            );
        }
        out += &match self.full_latency {
            Some(d) => format!("  pleine résolution après le relâchement : {:.0} ms", d.as_secs_f64() * 1000.0),
            None => "  pleine résolution après le relâchement : non reçue".into(),
        };
        out
    }
}
