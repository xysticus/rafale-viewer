//! « Retoucher… » : la sélection (JPEG, RAW ou les deux, avec leurs XMP) est
//! copiée dans un dossier voisin de la séance (`<séance>-retouche/`), puis le
//! logiciel de retouche choisi est lancé sur ce dossier. Il ne voit que les
//! photos choisies et écrit ses fichiers dans la copie : la séance triée et les
//! originaux restent intacts.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Logiciels proposés, s'ils sont installés : (identifiant du .desktop, nom).
/// Tous acceptent un dossier en argument.
const EDITORS: &[(&str, &str)] = &[
    ("com.rawtherapee.RawTherapee", "RawTherapee"),
    ("org.darktable.darktable", "darktable (importe le dossier dans sa bibliothèque)"),
    ("us.pixls.art.ART", "ART"),
    ("org.kde.digikam-showfoto", "Showfoto (éditeur de digiKam)"),
    ("com.xnview.XnViewMP", "XnView MP"),
    ("org.gnome.gThumb", "gThumb (attention : enregistre par-dessus l'original)"),
    ("org.gnome.Nautilus", "Fichiers (ouvrir le dossier seulement)"),
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
    EDITORS
        .iter()
        .copied()
        .filter(|(id, _)| dirs.iter().any(|d| d.join("applications").join(format!("{id}.desktop")).is_file()))
        .collect()
}

pub fn editor_name(id: &str) -> &str {
    EDITORS.iter().find(|(i, _)| *i == id).map_or(id, |(_, n)| n.split(" (").next().unwrap_or(n))
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
pub fn chooser() -> Command {
    let mut cmd = Command::new("zenity");
    cmd.args(["--list", "--title", "Rafale — retoucher avec", "--text", "Logiciel lancé sur la copie des photos choisies :"])
        .args(["--column", "Logiciel", "--column", "id", "--hide-column", "2", "--print-column", "2"])
        .args(["--width", "520", "--height", "360", "--ok-label", "Retoucher", "--cancel-label", "Annuler"]);
    for (id, name) in installed_editors() {
        cmd.args([name, id]);
    }
    cmd
}

/// Dossier de retouche, à côté de la séance : `2026-10-29` → `2026-10-29-retouche`.
/// Pas un sous-dossier : Rafale y chercherait des RAW.
pub fn destination(session: &Path) -> PathBuf {
    let session = std::fs::canonicalize(session).unwrap_or_else(|_| session.to_path_buf());
    let name = session.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "seance".into());
    session.with_file_name(format!("{name}-retouche"))
}

/// Ce qu'a fait la copie.
#[derive(Debug, Default, PartialEq)]
pub struct Copied {
    pub photos: usize,
    pub already: usize,
    pub sidecars: usize,
}

/// Copie les photos et leur XMP (`PHOTO.EXT.xmp`) dans `dest`, à plat. Une photo
/// déjà présente avec la même taille est sautée ; un XMP déjà présent n'est jamais
/// écrasé (le logiciel de retouche a pu y écrire ses réglages).
pub fn copy(photos: &[PathBuf], dest: &Path) -> std::io::Result<Copied> {
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
        let xmp = crate::marks::sidecar(photo);
        let xmp_target = crate::marks::sidecar(&target);
        if xmp.is_file() && !xmp_target.exists() {
            copy_file(&xmp, &xmp_target)?;
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

        let dest = destination(&session);
        assert_eq!(dest, base.canonicalize().unwrap().join("2026-10-29-retouche"));
        let photos = [jpeg, raw];
        assert_eq!(copy(&photos, &dest).unwrap(), Copied { photos: 2, already: 0, sidecars: 1 });
        assert!(dest.join("P1.JPG").is_file() && dest.join("P1.ORF").is_file());

        // Le logiciel de retouche réécrit le XMP : une nouvelle copie n'y touche pas.
        std::fs::write(dest.join("P1.ORF.xmp"), b"reglages darktable").unwrap();
        assert_eq!(copy(&photos, &dest).unwrap(), Copied { photos: 0, already: 2, sidecars: 0 });
        assert_eq!(std::fs::read(dest.join("P1.ORF.xmp")).unwrap(), b"reglages darktable");
    }
}
