//! Textes des fenêtres d'aide, affichés par zenity (texte sélectionnable).

pub const PRINCIPLES_TITLE: &str = "Rafale — principes";

pub const PRINCIPLES: &str = "\
Rafale sert à trier vite une séance de JPEG, y compris de très longues rafales \
(25 images/s ou plus).

DÉFILER SANS RIEN MANQUER

• Maintiens → ou ← : les photos défilent en continu, à la cadence de la rafale \
(25 i/s par défaut, option --fps). Aucune n'est sautée : si le disque ne suit pas, \
le défilé ralentit au lieu de sauter des photos.
• Pendant le défilé, les photos sont en aperçu réduit. Au relâchement, la pleine \
résolution arrive en un quart de seconde.
• À l'arrêt, pose le pointeur sur la photo : une loupe 100 % montre les pixels réels. \
C'est là qu'on vérifie la netteté, sur un visage par exemple.

RAFALES ET VUE À PLAT

• Les rafales sont reconnues toutes seules : numéro de vue des boîtiers OM System et \
Olympus, sinon heure de prise de vue. ↑ et ↓ sautent d'une rafale à l'autre.
• La liste à droite montre chaque rafale avec 4 vignettes réparties du début à la fin. \
Un clic sur une ligne va au début de la rafale.
• Vue « À plat » (dossiers de moins de 300 photos) : une vignette par photo, avec son \
marquage. Un clic va à la photo ; ↑ et ↓ montent ou descendent d'une rangée.

MARQUER

• Espace retient la photo (1 étoile), 1 à 5 donnent une note, X ou R la rejette, \
F1 à F5 posent une couleur. Les commandes en haut du panneau font la même chose \
à la souris.
• Le marquage est écrit dans un petit fichier PHOTO.JPG.xmp à côté de la photo, et \
à côté du RAW s'il existe (même dossier ou sous-dossier comme raw/). Les photos \
elles-mêmes ne sont jamais modifiées.
• darktable, digiKam, RawTherapee et Lightroom lisent ces fichiers. Un XMP qui \
contient le travail d'un autre logiciel (retouches, mots-clés…) n'est jamais modifié.

FILTRER

• La barre du bas filtre par note minimale, couleur, retenue ou rejet. Le défilé, \
les flèches, Début, Fin et la liste ne montrent alors que les photos qui passent \
le filtre.

MÉTHODE CONSEILLÉE

1. Défile chaque rafale touche maintenue, relâche sur les moments forts, vérifie \
à la loupe, Espace sur les meilleures.
2. Filtre sur ✓ et repasse sur les retenues ; affine avec les notes ou les couleurs.
3. Ouvre les retenues (les RAW de préférence) dans ton logiciel de retouche.
";

pub const SHORTCUTS_TITLE: &str = "Rafale — raccourcis";

pub const SHORTCUTS: &str = "\
DÉFILÉ

→ ou Page suivante : photo suivante ; maintenue, défilé continu
← ou Page précédente : photo précédente ; maintenue, défilé en arrière
↓ / ↑ : rafale suivante / précédente (vue À plat : rangée suivante / précédente)
Début / Fin : première / dernière photo

MARQUAGE

Espace : retenue (1 ★) ↔ non retenue
1 à 5 : nombre d'étoiles (rangée du haut sans Maj, ou pavé numérique)
0 : efface les étoiles
X ou R : rejetée ↔ non rejetée
F1 à F5 : étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève)

FENÊTRE

F ou F11 : plein écran
Échap ou Q : quitter

SOURIS

Pointeur sur la photo, à l'arrêt : loupe 100 %
Étoiles, couleurs, ✓, ✕ en haut du panneau : marquage de la photo affichée
(re-clic sur la même étoile ou couleur : l'enlève)
Mêmes commandes en bas : filtre ; « effacer » supprime le filtre
Clic sur une rafale ou une vignette : y aller
Molette sur le panneau, ou ascenseur : faire défiler la liste
Rafales / À plat : vue de la liste (À plat : dossiers de moins de 300 photos)
";
