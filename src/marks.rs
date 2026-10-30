//! Marquage des photos dans un fichier XMP à côté de chaque photo (PHOTO.JPG.xmp),
//! aux conventions les plus répandues :
//!
//! - `xmp:Rating` : 0 à 5 étoiles, -1 = rejetée (darktable, Lightroom, digiKam…) ;
//! - `xmp:Label` : couleur en toutes lettres, « Red »… « Purple » (darktable, Lightroom) ;
//! - `digiKam:PickLabel` (1 = rejetée, 3 = acceptée) et `digiKam:ColorLabel`.
//!
//! Un XMP écrit par un autre logiciel (darktable y range son historique de
//! retouches, par exemple) est lu mais jamais réécrit.

use std::path::{Path, PathBuf};

/// Signature de nos fichiers : seuls ceux-là sont réécrits ou supprimés.
const CREATOR: &str = "rafale-viewer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::Red,
        Color::Yellow,
        Color::Green,
        Color::Blue,
        Color::Purple,
    ];

    /// Nom Lightroom / darktable (`xmp:Label`).
    fn label(self) -> &'static str {
        match self {
            Color::Red => "Red",
            Color::Yellow => "Yellow",
            Color::Green => "Green",
            Color::Blue => "Blue",
            Color::Purple => "Purple",
        }
    }

    /// Numéro digiKam (`digiKam:ColorLabel`) : 1 rouge, 3 jaune, 4 vert, 5 bleu, 6 magenta.
    fn digikam(self) -> u8 {
        match self {
            Color::Red => 1,
            Color::Yellow => 3,
            Color::Green => 4,
            Color::Blue => 5,
            Color::Purple => 6,
        }
    }

    pub fn name_fr(self) -> &'static str {
        match self {
            Color::Red => "rouge",
            Color::Yellow => "jaune",
            Color::Green => "vert",
            Color::Blue => "bleu",
            Color::Purple => "violet",
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Color::Red => [225, 75, 70],
            Color::Yellow => [230, 200, 60],
            Color::Green => [90, 190, 90],
            Color::Blue => [80, 140, 230],
            Color::Purple => [175, 100, 210],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    /// 0 à 5 étoiles.
    pub rating: u8,
    pub rejected: bool,
    pub color: Option<Color>,
    /// XMP écrit par un autre logiciel : lecture seule.
    pub foreign: bool,
}

impl Mark {
    /// Retenue = au moins une étoile et pas rejetée.
    pub fn retained(&self) -> bool {
        !self.rejected && self.rating > 0
    }

    fn is_empty(&self) -> bool {
        self.rating == 0 && !self.rejected && self.color.is_none()
    }
}

pub fn sidecar(photo: &Path) -> PathBuf {
    let mut name = photo.as_os_str().to_owned();
    name.push(".xmp");
    PathBuf::from(name)
}

/// Lit le marquage d'une photo (marquage vide s'il n'y a pas de XMP).
pub fn load(photo: &Path) -> Mark {
    let Ok(text) = std::fs::read_to_string(sidecar(photo)) else {
        return Mark::default();
    };
    let rating: i32 = value(&text, "xmp:Rating")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let pick = value(&text, "digiKam:PickLabel").and_then(|v| v.trim().parse::<u8>().ok());
    let color = value(&text, "xmp:Label")
        .and_then(|l| Color::ALL.into_iter().find(|c| c.label() == l.trim()))
        .or_else(|| {
            let n = value(&text, "digiKam:ColorLabel")?.trim().parse::<u8>().ok()?;
            Color::ALL.into_iter().find(|c| c.digikam() == n)
        });
    Mark {
        rating: rating.clamp(0, 5) as u8,
        rejected: rating < 0 || pick == Some(1),
        color,
        foreign: !text.contains(CREATOR),
    }
}

/// Enregistre le marquage : écrit le XMP, ou le supprime si le marquage est vide.
/// Refuse de toucher à un XMP d'un autre logiciel.
pub fn save(photo: &Path, mark: &Mark) -> std::io::Result<()> {
    let path = sidecar(photo);
    if mark.foreign {
        return Err(std::io::Error::other("XMP d'un autre logiciel, non modifié"));
    }
    if mark.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let rating = if mark.rejected { -1 } else { mark.rating as i32 };
    let pick = if mark.rejected {
        1
    } else if mark.retained() {
        3
    } else {
        0
    };
    let mut attrs = vec![
        format!("xmp:CreatorTool=\"{CREATOR}\""),
        format!("xmp:Rating=\"{rating}\""),
        format!("digiKam:PickLabel=\"{pick}\""),
    ];
    if let Some(c) = mark.color {
        attrs.push(format!("xmp:Label=\"{}\"", c.label()));
        attrs.push(format!("digiKam:ColorLabel=\"{}\"", c.digikam()));
    }
    let xml = format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="{CREATOR}">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:digiKam="http://www.digikam.org/ns/1.0/"
    {}/>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#,
        attrs.join("\n    ")
    );
    // Écriture atomique : fichier temporaire puis renommage.
    let tmp = path.with_extension("xmp.tmp");
    std::fs::write(&tmp, xml)?;
    std::fs::rename(&tmp, &path)
}

/// Valeur d'une propriété XMP, en attribut (`nom="v"`) ou en élément (`<nom>v</nom>`).
fn value<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let attr = format!("{name}=\"");
    if let Some(i) = text.find(&attr) {
        let rest = &text[i + attr.len()..];
        return rest.find('"').map(|j| &rest[..j]);
    }
    let open = format!("<{name}>");
    let i = text.find(&open)? + open.len();
    let rest = &text[i..];
    rest.find('<').map(|j| &rest[..j])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_photo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rafale-marks-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("P0001.JPG")
    }

    #[test]
    fn aller_retour() {
        let photo = temp_photo("aller-retour");
        for mark in [
            Mark { rating: 3, color: Some(Color::Green), ..Default::default() },
            Mark { rating: 0, color: Some(Color::Purple), ..Default::default() },
            Mark { rejected: true, ..Default::default() },
            Mark { rating: 5, ..Default::default() },
        ] {
            save(&photo, &mark).unwrap();
            assert_eq!(load(&photo), mark);
        }
        // Marquage vide : le XMP disparaît.
        save(&photo, &Mark::default()).unwrap();
        assert!(!sidecar(&photo).exists());
        assert_eq!(load(&photo), Mark::default());
    }

    #[test]
    fn xmp_etranger_intouchable() {
        let photo = temp_photo("etranger");
        let foreign = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:darktable="http://darktable.sf.net/"
 xmp:Rating="4" xmp:Label="Blue" darktable:xmp_version="5"/></rdf:RDF></x:xmpmeta>"#;
        std::fs::write(sidecar(&photo), foreign).unwrap();
        let mut mark = load(&photo);
        assert_eq!((mark.rating, mark.color, mark.foreign), (4, Some(Color::Blue), true));
        mark.rating = 1;
        assert!(save(&photo, &mark).is_err());
        assert_eq!(std::fs::read_to_string(sidecar(&photo)).unwrap(), foreign);
    }

    #[test]
    fn element_et_rejet_digikam() {
        let photo = temp_photo("element");
        std::fs::write(
            sidecar(&photo),
            "<xmp:Rating>2</xmp:Rating><digiKam:PickLabel>1</digiKam:PickLabel><digiKam:ColorLabel>5</digiKam:ColorLabel>",
        )
        .unwrap();
        let mark = load(&photo);
        assert!(mark.rejected);
        assert_eq!(mark.color, Some(Color::Blue));
    }
}
