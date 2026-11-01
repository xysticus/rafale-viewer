//! Association de chaque JPEG à son RAW : même nom, extension RAW, cherché dans
//! le dossier des JPEG puis dans ses sous-dossiers directs (raw/, RAW/, ORF/…).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const RAW_EXTENSIONS: &[&str] = &[
    "orf", "ori", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "rw2",
    "rwl", "pef", "srw", "3fr", "iiq", "x3f", "erf", "mef", "mos", "kdc",
];

/// RAW de chaque fichier de `files` (dans le même ordre), s'il existe.
/// À nom égal, le dossier des JPEG l'emporte sur ses sous-dossiers.
pub fn find(dir: &Path, files: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    subdirs.sort();

    let mut by_stem: HashMap<OsString, PathBuf> = HashMap::new();
    for d in std::iter::once(dir.to_path_buf()).chain(subdirs) {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut raws: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| is_raw(p) && p.is_file())
            .collect();
        raws.sort();
        for p in raws {
            if let Some(stem) = p.file_stem() {
                by_stem.entry(stem.to_owned()).or_insert(p);
            }
        }
    }
    files
        .iter()
        .map(|f| f.file_stem().and_then(|s| by_stem.get(s)).cloned())
        .collect()
}

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RAW_EXTENSIONS.iter().any(|r| e.eq_ignore_ascii_case(r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossier_puis_sous_dossiers() {
        let dir = std::env::temp_dir().join(format!("rafale-raw-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("raw")).unwrap();
        for f in ["A.JPG", "B.JPG", "C.JPG", "B.ORF", "raw/A.ORF", "raw/B.ORF", "raw/A.xmp"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let files: Vec<PathBuf> = ["A.JPG", "B.JPG", "C.JPG"].iter().map(|f| dir.join(f)).collect();
        let raws = find(&dir, &files);
        assert_eq!(raws[0], Some(dir.join("raw/A.ORF")));
        assert_eq!(raws[1], Some(dir.join("B.ORF")), "le dossier des JPEG l'emporte");
        assert_eq!(raws[2], None);
    }
}
