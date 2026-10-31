//! Marquage des photos dans un fichier XMP à côté de chaque photo (PHOTO.JPG.xmp),
//! aux conventions les plus répandues :
//!
//! - `xmp:Rating` : 0 à 5 étoiles, -1 = rejetée (darktable, Lightroom, digiKam…) ;
//! - `xmp:Label` : couleur en toutes lettres, « Red »… « Purple » (darktable, Lightroom) ;
//! - `digiKam:PickLabel` (1 = rejetée, 3 = acceptée) et `digiKam:ColorLabel`.
//!
//! Un XMP écrit par un autre logiciel (darktable y range son historique de
//! retouches, par exemple) est lu mais jamais réécrit, sauf s'il est vierge :
//! ni note, ni couleur, ni retouche, comme ceux que darktable pose à l'import.

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

    /// Rang dans `ALL`.
    pub fn index(self) -> usize {
        Color::ALL.iter().position(|&c| c == self).unwrap()
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
    /// XMP écrit par un autre logiciel, et non vierge : lecture seule.
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

/// Filtre de navigation : les critères actifs se cumulent (et).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Filter {
    /// Note minimale (0 = pas de critère) ; exclut les rejetées.
    pub min_rating: u8,
    /// Couleurs acceptées (aucune cochée = pas de critère).
    pub colors: [bool; 5],
    /// Retenues et/ou rejetées (aucune des deux = pas de critère).
    pub retained: bool,
    pub rejected: bool,
}

impl Filter {
    pub fn active(&self) -> bool {
        *self != Filter::default()
    }

    pub fn matches(&self, m: &Mark) -> bool {
        let rating = self.min_rating == 0 || (!m.rejected && m.rating >= self.min_rating);
        let color = !self.colors.contains(&true) || m.color.is_some_and(|c| self.colors[c.index()]);
        let state = !(self.retained || self.rejected)
            || (self.retained && m.retained())
            || (self.rejected && m.rejected);
        rating && color && state
    }
}

pub fn sidecar(photo: &Path) -> PathBuf {
    let mut name = photo.as_os_str().to_owned();
    name.push(".xmp");
    PathBuf::from(name)
}

/// Ce XMP existe-t-il et vient-il d'un autre logiciel, avec un contenu à préserver ?
pub fn is_foreign(xmp: &Path) -> bool {
    std::fs::read_to_string(xmp).is_ok_and(|text| protected(&text))
}

/// Ce XMP est-il l'un des nôtres ?
pub fn is_ours(xmp: &Path) -> bool {
    std::fs::read_to_string(xmp).is_ok_and(|text| text.contains(CREATOR))
}

/// XMP d'un autre logiciel qui contient autre chose que du vide.
fn protected(text: &str) -> bool {
    !text.contains(CREATOR) && !blank(text)
}

/// XMP étranger vierge : rien qu'on perdrait en le remplaçant. Tout ce qui
/// n'est pas reconnu comme vide (titre, retouches Lightroom, historique
/// digiKam, mots-clés posés à la main…) compte comme du contenu.
fn blank(text: &str) -> bool {
    let zero = |name| value(text, name).is_none_or(|v| v.trim() == "0");
    let none = |name| value(text, name).is_none_or(|v| v.trim().is_empty());
    let keywords_auto = section(text, "lr:hierarchicalSubject")
        .is_none_or(|s| items(s).all(|k| k.starts_with("darktable|")));
    zero("xmp:Rating")
        && none("xmp:Label")
        && zero("digiKam:ColorLabel")
        && zero("digiKam:PickLabel")
        && zero("darktable:history_end")
        && section(text, "darktable:history").is_none_or(|s| items(s).next().is_none())
        && keywords_auto
        && ["crs:", "digiKam:ImageHistory", "<dc:title", "<dc:description", "<dc:rights"]
            .iter()
            .all(|k| !text.contains(k))
}

/// Contenu d'un élément `<nom>…</nom>`.
fn section<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let i = text.find(&open)? + open.len();
    let j = text[i..].find(&close)?;
    Some(&text[i..i + j])
}

/// Valeurs des `<rdf:li>` d'une liste.
fn items(list: &str) -> impl Iterator<Item = &str> {
    list.split("<rdf:li").skip(1).filter_map(|s| {
        let s = &s[s.find('>')? + 1..];
        s.find('<').map(|j| s[..j].trim())
    })
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
        foreign: protected(&text),
    }
}

/// Enregistre le marquage d'une photo dans son XMP.
pub fn save(photo: &Path, mark: &Mark) -> std::io::Result<()> {
    save_to(&sidecar(photo), mark)
}

/// Écrit le XMP `path`, ou supprime le nôtre si le marquage est vide.
/// Refuse de toucher à un XMP d'un autre logiciel qui n'est pas vierge
/// (vérifié sur le disque) ; un XMP étranger vierge est remplacé, jamais supprimé.
pub fn save_to(path: &Path, mark: &Mark) -> std::io::Result<()> {
    if mark.foreign || is_foreign(path) {
        return Err(std::io::Error::other("XMP d'un autre logiciel, non modifié"));
    }
    if mark.is_empty() {
        if !is_ours(path) {
            return Ok(());
        }
        return match std::fs::remove_file(path) {
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
    std::fs::rename(&tmp, path)
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
    fn filtre() {
        let rouge3 = Mark { rating: 3, color: Some(Color::Red), ..Default::default() };
        let rejetee = Mark { rejected: true, ..Default::default() };
        let rien = Mark::default();
        let tout = Filter::default();
        assert!(!tout.active() && tout.matches(&rien) && tout.matches(&rejetee));
        let deux = Filter { min_rating: 2, ..Default::default() };
        assert!(deux.matches(&rouge3) && !deux.matches(&rien) && !deux.matches(&rejetee));
        let mut couleurs = Filter::default();
        couleurs.colors[Color::Blue.index()] = true;
        assert!(!couleurs.matches(&rouge3));
        couleurs.colors[Color::Red.index()] = true;
        assert!(couleurs.matches(&rouge3), "plusieurs couleurs : l'une ou l'autre");
        let tri = Filter { retained: true, rejected: true, ..Default::default() };
        assert!(tri.matches(&rouge3) && tri.matches(&rejetee) && !tri.matches(&rien));
    }

    #[test]
    fn protection_verifiee_sur_disque() {
        // Un XMP étranger apparu après le chargement (import darktable…) n'est pas écrasé.
        let photo = temp_photo("disque");
        let xmp = sidecar(&photo);
        std::fs::write(&xmp, r#"<darktable xmp:Rating="3"/>"#).unwrap();
        let mark = Mark { rating: 2, ..Default::default() };
        assert!(save_to(&xmp, &mark).is_err());
        assert!(save_to(&xmp, &Mark::default()).is_err(), "ni supprimé");
        assert_eq!(std::fs::read_to_string(&xmp).unwrap(), r#"<darktable xmp:Rating="3"/>"#);
    }

    #[test]
    fn xmp_darktable_vierge_remplace() {
        let photo = temp_photo("vierge");
        let xmp = sidecar(&photo);
        let vierge = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description
   xmp:Rating="0" darktable:history_end="0">
   <darktable:history><rdf:Seq/></darktable:history>
   <dc:subject><rdf:Bag><rdf:li>darktable</rdf:li><rdf:li>format</rdf:li></rdf:Bag></dc:subject>
   <lr:hierarchicalSubject><rdf:Bag><rdf:li>darktable|format|jpg</rdf:li></rdf:Bag></lr:hierarchicalSubject>
  </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        std::fs::write(&xmp, vierge).unwrap();
        assert!(!load(&photo).foreign);
        // Marquage vide : le fichier vierge est laissé tel quel.
        save(&photo, &Mark::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&xmp).unwrap(), vierge);
        let mark = Mark { rating: 2, ..Default::default() };
        save(&photo, &mark).unwrap();
        assert_eq!(load(&photo), mark);
        assert!(is_ours(&xmp));

        // Le moindre contenu le protège.
        for extra in [
            r#"xmp:Rating="1""#,
            r#"xmp:Label="Red""#,
            r#"darktable:history_end="3""#,
            "<lr:hierarchicalSubject><rdf:Bag><rdf:li>chevaux|Pastré</rdf:li></rdf:Bag></lr:hierarchicalSubject>",
            r#"<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Titre</rdf:li></rdf:Alt></dc:title>"#,
        ] {
            let text = format!("<x:xmpmeta {extra}></x:xmpmeta>");
            assert!(!blank(&text), "{extra}");
        }
        assert!(!blank("<darktable:history><rdf:Seq><rdf:li darktable:operation=\"exposure\"/></rdf:Seq></darktable:history>"));
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
