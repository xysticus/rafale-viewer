//! Import d'une carte mémoire : les JPEG et les RAW de `DCIM/*/` sont copiés
//! dans `~/Images/AAAA-MM-JJ/` (jour de prise de vue), les RAW dans `raw/`.
//! La carte n'est jamais modifiée ; un fichier déjà importé (même nom, même
//! taille) est sauté ; chaque copie passe par un fichier temporaire renommé
//! à la fin, pour qu'une interruption ne laisse pas de fichier tronqué.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Dossier DCIM d'une carte : la racine de la carte, DCIM lui-même ou l'un de
/// ses sous-dossiers (100OMSYS…).
pub fn find_dcim(path: &Path) -> Option<PathBuf> {
    let is_dcim = |p: &Path| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("DCIM"));
    if is_dcim(path) {
        return Some(path.to_path_buf());
    }
    if let Some(parent) = path.parent().filter(|p| is_dcim(p)) {
        return Some(parent.to_path_buf());
    }
    std::fs::read_dir(path).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.is_dir() && is_dcim(p))
}

/// Dossier des images de l'utilisateur (~/Images en français).
pub fn pictures_dir() -> PathBuf {
    Command::new("xdg-user-dir")
        .arg("PICTURES")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim_end()))
        .filter(|p| p.is_absolute() && p.is_dir())
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Images"))
}

struct Copy {
    src: PathBuf,
    dest: PathBuf,
    size: u64,
}

pub struct Plan {
    copies: Vec<Copy>,
    /// Jour → (JPEG, RAW) à copier.
    days: BTreeMap<String, (usize, usize)>,
    already: usize,
    /// Même nom déjà présent avec une autre taille : non copié, signalé.
    conflicts: Vec<PathBuf>,
    /// Ni JPEG ni RAW (vidéos…), ou date illisible.
    ignored: usize,
    undated: usize,
    bytes: u64,
    dest_root: PathBuf,
}

fn is_jpeg(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
}

/// Recense la carte : où va chaque fichier, ce qui est déjà là.
pub fn plan(dcim: &Path, dest_root: &Path) -> Plan {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dcim)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().filter_map(|e| e.ok()).map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut plan = Plan {
        copies: Vec::new(),
        days: BTreeMap::new(),
        already: 0,
        conflicts: Vec::new(),
        ignored: 0,
        undated: 0,
        bytes: 0,
        dest_root: dest_root.to_path_buf(),
    };
    // Jour des JPEG, réutilisé pour le RAW de même nom (lecture plus sûre).
    let mut day_of: HashMap<(PathBuf, String), String> = HashMap::new();
    let (jpegs, others): (Vec<PathBuf>, Vec<PathBuf>) = files.into_iter().partition(|p| is_jpeg(p));
    for (path, raw) in jpegs.into_iter().map(|p| (p, false)).chain(others.into_iter().map(|p| (p, true))) {
        if raw && !crate::raw::is_raw(&path) {
            plan.ignored += 1;
            continue;
        }
        let key = (path.parent().unwrap_or(Path::new("")).to_path_buf(), stem(&path));
        let day = if raw { day_of.get(&key).cloned() } else { None }.or_else(|| crate::meta::capture_day(&path));
        let Some(day) = day else {
            plan.undated += 1;
            continue;
        };
        if !raw {
            day_of.insert(key, day.clone());
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let mut dest = dest_root.join(&day);
        if raw {
            dest.push("raw");
        }
        dest.push(path.file_name().unwrap_or_default());
        match dest.metadata() {
            Ok(m) if m.len() == size => {
                plan.days.entry(day).or_default();
                plan.already += 1;
            }
            Ok(_) => plan.conflicts.push(dest),
            Err(_) => {
                let count = plan.days.entry(day).or_default();
                if raw { count.1 += 1 } else { count.0 += 1 }
                plan.bytes += size;
                plan.copies.push(Copy { src: path, dest, size });
            }
        }
    }
    plan
}

fn stem(p: &Path) -> String {
    p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_owned()
}

impl Plan {
    /// Jour le plus récent concerné (copié ou déjà présent), à ouvrir ensuite.
    pub fn latest_day(&self) -> Option<PathBuf> {
        self.days.keys().next_back().map(|d| self.dest_root.join(d))
    }

    pub fn is_empty(&self) -> bool {
        self.copies.is_empty()
    }

    /// Résumé pour la confirmation.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        if self.copies.is_empty() {
            s += "Rien de nouveau à importer";
        } else {
            let (j, r) = self.days.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            s += &format!("Importer {j} JPEG et {r} RAW ({:.1} Go) :\n", self.bytes as f64 / 1e9);
            for (day, (j, r)) in self.days.iter().filter(|(_, c)| **c != (0, 0)) {
                s += &format!("\n   {}/{day}   {j} JPEG, {r} RAW (dans raw/)", self.dest_root.display());
            }
        }
        let mut notes = Vec::new();
        if self.already > 0 {
            notes.push(format!("{} fichiers déjà importés, sautés", self.already));
        }
        if !self.conflicts.is_empty() {
            notes.push(format!("{} fichiers de même nom mais de taille différente, non copiés (par exemple {})", self.conflicts.len(), self.conflicts[0].display()));
        }
        if self.ignored > 0 {
            notes.push(format!("{} fichiers ni JPEG ni RAW (vidéos…), non copiés", self.ignored));
        }
        if self.undated > 0 {
            notes.push(format!("{} fichiers sans date lisible, non copiés", self.undated));
        }
        if !notes.is_empty() {
            s += "\n\n";
            s += &notes.join("\n");
        }
        s += "\n\nLa carte n'est pas modifiée.";
        s
    }

    /// Copie avec une barre de progression (zenity). Renvoie le nombre de fichiers
    /// copiés, ou une erreur ; Err("annulé") si la fenêtre de progression est fermée.
    pub fn run(&self) -> Result<usize, String> {
        let mut progress = Command::new("zenity")
            .args(["--progress", "--title", "Rafale — import de la carte", "--auto-close", "--percentage", "0"])
            .args(["--text", "Préparation…", "--width", "520"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("zenity : {e}"))?;
        let mut tell = progress.stdin.take().unwrap();
        let mut done_bytes = 0u64;
        let total = self.bytes.max(1);
        let t0 = std::time::Instant::now();
        for (k, c) in self.copies.iter().enumerate() {
            let rate = done_bytes as f64 / t0.elapsed().as_secs_f64().max(0.1) / 1e6;
            let name = c.src.file_name().unwrap_or_default().to_string_lossy();
            let line = format!("# {name}   ({} / {}, {rate:.0} Mo/s)\n{}\n", k + 1, self.copies.len(), 100 * done_bytes / total);
            if tell.write_all(line.as_bytes()).is_err() {
                return Err("annulé".into());
            }
            copy_one(c).map_err(|e| format!("{} : {e}", c.src.display()))?;
            done_bytes += c.size;
        }
        let _ = tell.write_all(b"100\n");
        drop(tell);
        let _ = progress.wait();
        Ok(self.copies.len())
    }
}

/// Copie sous un nom temporaire, vérifie la taille, puis renomme.
fn copy_one(c: &Copy) -> std::io::Result<()> {
    let dir = c.dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = c.dest.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let n = std::fs::copy(&c.src, &tmp)?;
    if n != c.size {
        let _ = std::fs::remove_file(&tmp);
        return Err(std::io::Error::other(format!("copie incomplète ({n} octets sur {})", c.size)));
    }
    std::fs::rename(&tmp, &c.dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carte_vers_jours() {
        let base = std::env::temp_dir().join(format!("rafale-import-{}", std::process::id()));
        let card = base.join("carte/DCIM/100OMSYS");
        std::fs::create_dir_all(&card).unwrap();
        // Un vrai JPEG daté (en-tête EXIF minimal) et son « RAW », plus une vidéo.
        let jpeg = minimal_jpeg("2026:10:29 20:45:07");
        std::fs::write(card.join("PA290001.JPG"), &jpeg).unwrap();
        std::fs::write(card.join("PA290001.ORF"), b"pas un vrai RAW").unwrap();
        std::fs::write(card.join("PA290002.MOV"), b"video").unwrap();
        assert_eq!(find_dcim(&base.join("carte")), Some(base.join("carte/DCIM")));
        assert_eq!(find_dcim(&card), Some(base.join("carte/DCIM")));

        let dest = base.join("Images");
        let p = plan(&base.join("carte/DCIM"), &dest);
        assert_eq!(p.copies.len(), 2, "JPEG, et RAW daté par son JPEG");
        assert_eq!(p.ignored, 1, "la vidéo");
        for c in &p.copies {
            copy_one(c).unwrap();
        }
        assert!(dest.join("2026-10-29/PA290001.JPG").is_file());
        assert!(dest.join("2026-10-29/raw/PA290001.ORF").is_file());
        assert_eq!(p.latest_day(), Some(dest.join("2026-10-29")));

        // Relancé : tout est déjà là.
        let again = plan(&base.join("carte/DCIM"), &dest);
        assert!(again.is_empty());
        assert_eq!(again.already, 2);
    }

    /// JPEG réduit à SOI + APP1 EXIF (DateTimeOriginal) + EOI.
    fn minimal_jpeg(date: &str) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend(b"II*\0");
        tiff.extend(8u32.to_le_bytes());
        // IFD0 : une entrée, pointeur vers l'IFD EXIF.
        tiff.extend(1u16.to_le_bytes());
        tiff.extend(0x8769u16.to_le_bytes());
        tiff.extend(4u16.to_le_bytes());
        tiff.extend(1u32.to_le_bytes());
        tiff.extend(26u32.to_le_bytes());
        tiff.extend(0u32.to_le_bytes());
        // IFD EXIF à 26 : DateTimeOriginal, 20 octets ASCII à 44.
        tiff.extend(1u16.to_le_bytes());
        tiff.extend(0x9003u16.to_le_bytes());
        tiff.extend(2u16.to_le_bytes());
        tiff.extend(20u32.to_le_bytes());
        tiff.extend(44u32.to_le_bytes());
        tiff.extend(0u32.to_le_bytes());
        tiff.extend(date.as_bytes());
        tiff.push(0);
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend(((app1.len() + 2) as u16).to_be_bytes());
        jpeg.extend(app1);
        jpeg.extend([0xFF, 0xD9]);
        jpeg
    }
}
