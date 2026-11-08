//! « Retoucher… » : la sélection (JPEG, RAW ou les deux, avec leurs XMP) est
//! copiée dans un dossier voisin de la séance, suffixé du nom du logiciel
//! (`<séance>-rawtherapee/`, `<séance>-darktable/`… ; `<séance>-retouche/` pour
//! Fichiers), pour essayer chaque logiciel sur sa propre copie, puis le
//! logiciel de retouche choisi est lancé sur ce dossier. Il ne voit que les
//! photos choisies et écrit ses fichiers dans la copie : la séance triée et les
//! originaux restent intacts.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Logiciels proposés, s'ils sont installés : (identifiant du .desktop, nom,
/// suffixe du dossier de copie). Tous acceptent un dossier en argument. Un même
/// logiciel peut avoir deux identifiants (paquet Fedora, Flatpak) : le premier
/// installé est proposé.
const EDITORS: &[(&str, &str, &str)] = &[
    ("com.rawtherapee.RawTherapee", "RawTherapee", "rawtherapee"),
    ("org.darktable.darktable", "darktable (importe le dossier dans sa bibliothèque)", "darktable"),
    ("org.darktable.Darktable", "darktable (importe le dossier dans sa bibliothèque)", "darktable"),
    ("us.pixls.art.ART", "ART", "art"),
    ("org.kde.digikam-showfoto", "Showfoto (éditeur de digiKam)", "showfoto"),
    ("com.xnview.XnViewMP", "XnView MP", "xnview"),
    ("org.gnome.gThumb", "gThumb (attention : enregistre par-dessus l'original)", "gthumb"),
    ("org.gnome.Nautilus", "Fichiers (ouvrir le dossier seulement)", "retouche"),
];

/// Logiciels installés, d'après les dossiers d'applications (Flatpak compris).
pub fn installed_editors() -> Vec<(&'static str, &'static str)> {
    let mut dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".into())
        .split(':')
        .map(PathBuf::from)
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    let mut seen = Vec::new();
    EDITORS
        .iter()
        .filter(|(id, _, _)| dirs.iter().any(|d| d.join("applications").join(format!("{id}.desktop")).is_file()))
        .filter(|(_, _, suffix)| {
            let first = !seen.contains(suffix);
            seen.push(*suffix);
            first
        })
        .map(|&(id, name, _)| (id, name))
        .collect()
}

pub fn editor_name(id: &str) -> &str {
    EDITORS.iter().find(|(i, _, _)| *i == id).map_or(id, |(_, n, _)| n.split(" (").next().unwrap_or(n))
}

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("rafale/retouche"))
}

/// Logiciel retenu au dernier choix, s'il est toujours installé.
pub fn saved_editor() -> Option<String> {
    let id = std::fs::read_to_string(config_file()?).ok()?.trim().to_owned();
    installed_editors().iter().any(|(i, _)| *i == id).then_some(id)
}

pub fn save_editor(id: &str) {
    let Some(file) = config_file() else { return };
    let ok = file.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok()) && std::fs::write(&file, id).is_ok();
    if !ok {
        log::warn!("choix du logiciel non enregistré dans {}", file.display());
    }
}

/// Fenêtre de choix du logiciel (zenity) ; la réponse, l'identifiant, sort sur stdout.
/// Liste des logiciels installés ; `then_retouch` : le bouton lance la retouche,
/// sinon il retient seulement le choix.
pub fn chooser(then_retouch: bool) -> Command {
    let mut cmd = Command::new("zenity");
    let ok = if then_retouch { "Retoucher" } else { "Choisir" };
    cmd.args(["--list", "--title", "Rafale — retoucher avec", "--text", "Logiciel lancé sur la copie des photos choisies :"])
        .args(["--column", "Logiciel", "--column", "id", "--hide-column", "2", "--print-column", "2"])
        .args(["--width", "520", "--height", "360", "--ok-label", ok, "--cancel-label", "Annuler"]);
    for (id, name) in installed_editors() {
        cmd.args([name, id]);
    }
    cmd
}

/// Dossier de copie, à côté de la séance et propre au logiciel : `2026-10-29` →
/// `2026-10-29-rawtherapee` (`-retouche` pour Fichiers). Pas un sous-dossier :
/// Rafale y chercherait des RAW.
pub fn destination(session: &Path, editor: &str) -> PathBuf {
    let session = std::fs::canonicalize(session).unwrap_or_else(|_| session.to_path_buf());
    let name = session.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "seance".into());
    let suffix = EDITORS.iter().find(|(i, _, _)| *i == editor).map_or("retouche", |(_, _, s)| s);
    session.with_file_name(format!("{name}-{suffix}"))
}

/// Ce qu'a fait la copie.
#[derive(Debug, Default, PartialEq)]
pub struct Copied {
    pub photos: usize,
    pub already: usize,
    pub sidecars: usize,
}

/// Copie les photos, et leur XMP (`PHOTO.EXT.xmp`) si `xmp`, dans `dest`, à plat.
/// Une photo déjà présente avec la même taille est sautée ; un XMP déjà présent
/// n'est jamais écrasé (le logiciel de retouche a pu y écrire ses réglages).
pub fn copy(photos: &[PathBuf], dest: &Path, xmp: bool) -> std::io::Result<Copied> {
    std::fs::create_dir_all(dest)?;
    let mut done = Copied::default();
    for photo in photos {
        let Some(name) = photo.file_name() else { continue };
        let target = dest.join(name);
        let size = photo.metadata()?.len();
        if target.metadata().is_ok_and(|m| m.len() == size) {
            done.already += 1;
        } else {
            copy_file(photo, &target)?;
            done.photos += 1;
        }
        let sidecar = crate::marks::sidecar(photo);
        let sidecar_target = crate::marks::sidecar(&target);
        if xmp && sidecar.is_file() && !sidecar_target.exists() {
            copy_file(&sidecar, &sidecar_target)?;
            done.sidecars += 1;
        }
    }
    Ok(done)
}

/// Copie via un fichier temporaire renommé, en gardant la date d'origine.
fn copy_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    std::fs::copy(src, &tmp)?;
    if let Ok(t) = src.metadata().and_then(|m| m.modified()) {
        std::fs::File::options().write(true).open(&tmp)?.set_modified(t)?;
    }
    std::fs::rename(&tmp, dest)
}

/// Lance le logiciel sur le dossier (sans attendre qu'il se ferme).
pub fn launch(editor: &str, dir: &Path) -> std::io::Result<()> {
    Command::new("gtk-launch").arg(editor).arg(dir).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copie_sans_ecraser_les_reglages() {
        let base = std::env::temp_dir().join(format!("rafale-retouche-{}", std::process::id()));
        let session = base.join("2026-10-29");
        std::fs::create_dir_all(session.join("raw")).unwrap();
        let jpeg = session.join("P1.JPG");
        let raw = session.join("raw/P1.ORF");
        std::fs::write(&jpeg, b"jpeg").unwrap();
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(crate::marks::sidecar(&raw), b"marquage Rafale").unwrap();

        let base = base.canonicalize().unwrap();
        assert_eq!(destination(&session, "org.gnome.Nautilus"), base.join("2026-10-29-retouche"));
        assert_eq!(destination(&session, "org.darktable.darktable"), base.join("2026-10-29-darktable"));
        let dest = destination(&session, "com.rawtherapee.RawTherapee");
        assert_eq!(dest, base.join("2026-10-29-rawtherapee"));
        let photos = [jpeg, raw];
        // Sans XMP coché : les photos seules.
        let sans = base.join("sans-xmp");
        assert_eq!(copy(&photos, &sans, false).unwrap(), Copied { photos: 2, already: 0, sidecars: 0 });
        assert!(!sans.join("P1.ORF.xmp").exists());
        assert_eq!(copy(&photos, &dest, true).unwrap(), Copied { photos: 2, already: 0, sidecars: 1 });
        assert!(dest.join("P1.JPG").is_file() && dest.join("P1.ORF").is_file());

        // Le logiciel de retouche réécrit le XMP : une nouvelle copie n'y touche pas.
        std::fs::write(dest.join("P1.ORF.xmp"), b"reglages darktable").unwrap();
        assert_eq!(copy(&photos, &dest, true).unwrap(), Copied { photos: 0, already: 2, sidecars: 0 });
        assert_eq!(std::fs::read(dest.join("P1.ORF.xmp")).unwrap(), b"reglages darktable");
    }
}
