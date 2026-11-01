//! Textes des fenêtres d'aide, affichés par zenity (texte sélectionnable).

pub const PRINCIPLES_TITLE: &str = concat!("Rafale ", env!("CARGO_PKG_VERSION"), " — principes");

pub const PRINCIPLES: &str = concat!("\
RAFALE — version ", env!("CARGO_PKG_VERSION"), "

Rafale sert à trier vite une séance de JPEG, y compris de très longues rafales \
(25 images/s ou plus).

DÉFILER SANS RIEN MANQUER

• Maintiens → ou ← : les photos défilent en continu, à la cadence de la rafale \
(25 i/s par défaut, option --fps). Aucune n'est sautée : si le disque ne suit pas, \
le défilé ralentit au lieu de sauter des photos.
• En rouvrant un dossier, Rafale repart de la dernière photo vue.
• Pendant le défilé, les photos sont en aperçu réduit. Au relâchement, la pleine \
résolution arrive en un quart de seconde.
• À l'arrêt, pose le pointeur sur la photo : une loupe 100 % montre les pixels réels. \
C'est là qu'on vérifie la netteté, sur un visage par exemple.
• Un clic sur la photo fige la loupe à cet endroit : elle y reste d'une vue à \
l'autre (← →), pour comparer la netteté du même visage sur toute la rafale. \
Clic droit pour la libérer ; elle l'est aussi en changeant de rafale.
• Le cadre vert sur la photo est la zone de mise au point enregistrée par le \
boîtier (OM System) : elle suit le sujet, souvent le visage détecté. C l'affiche \
ou le masque. La touche A \
place la loupe sur cette zone, vue après vue : elle suit le visage dans toute la \
rafale, même s'il se déplace dans le cadre.
• À l'arrêt, Rafale mesure en tâche de fond la netteté de la zone AF de chaque vue \
de la rafale, et affiche celle de la photo : « 87 % de la plus nette · rang 4 / 54 ». \
N va à la vue la plus nette. L'indice mesure ce qui est dans la zone AF : si \
l'AF a visé le col au lieu du visage, il baisse aussi. La mesure ne ralentit \
jamais le défilé : elle s'interrompt dès qu'il reprend.

RAFALES ET VUE À PLAT

• Les rafales sont reconnues toutes seules : numéro de vue des boîtiers OM System et \
Olympus, sinon heure de prise de vue. ↑ et ↓ sautent d'une rafale à l'autre.
• La liste à droite montre chaque rafale avec 4 vignettes réparties du début à la fin. \
Un clic sur une ligne va au début de la rafale.
• Vue « À plat » (dossiers de moins de 300 photos) : une vignette par photo, avec son \
marquage. Un clic va à la photo ; ↑ et ↓ montent ou descendent d'une rangée.

MARQUER

• Une photo est retenue à partir de 2 étoiles ; 1 étoile veut dire « peut-être ». \
Espace retient la photo (2 étoiles) ou la remet à zéro, 1 à 5 donnent une note, X ou R la rejette, \
F1 à F5 posent une couleur. Les commandes en haut du panneau font la même chose \
à la souris.
• Le marquage est écrit dans un petit fichier PHOTO.JPG.xmp à côté de la photo, et \
à côté du RAW s'il existe (même dossier ou sous-dossier comme raw/). Les photos \
elles-mêmes ne sont jamais modifiées.
• darktable, digiKam, RawTherapee et Lightroom lisent ces fichiers. Un XMP qui \
contient le travail d'un autre logiciel (retouches, mots-clés…) n'est jamais modifié.

METTRE LES REJETÉES À LA CORBEILLE

• Le bouton rouge « Corbeille : N rejetées » (en bas, seulement s'il y en a) met \
à la corbeille de GNOME les photos marquées de la croix rouge : JPEG, RAW, et tous \
leurs fichiers d'accompagnement (XMP de Rafale ou d'autres logiciels, PP3 de \
RawTherapee). Une fenêtre demande d'abord confirmation, avec le nombre de fichiers \
et la place libérée. Tout reste récupérable dans la corbeille.

IMPORTER UNE CARTE

• Ouvre la carte mémoire avec Rafale (Fichiers : clic droit › Ouvrir avec › Rafale). \
Après un résumé et une confirmation, ses JPEG sont copiés dans ~/Images/AAAA-MM-JJ \
(jour de prise de vue) et ses RAW dans raw/, puis Rafale s'ouvre sur le jour \
importé. Les fichiers déjà importés sont sautés ; la carte n'est pas modifiée.

FILTRER

• La barre du bas filtre par note minimale, couleur, retenue ou rejet. Le défilé, \
les flèches, Début, Fin et la liste ne montrent alors que les photos qui passent \
le filtre.

COPIER LA SÉLECTION

• Ctrl+C, ou le bouton « Copier … », met la sélection dans le presse-papiers : \
les photos filtrées si un filtre est actif, sinon les photos retenues. Un Ctrl+V \
dans un dossier ouvert dans Fichiers y copie les fichiers.
• Le choix JPEG / RAW / Les deux décide des fichiers copiés. On peut ainsi donner \
à n'importe quel logiciel de retouche un dossier qui ne contient que les photos \
choisies, sans dépendre de sa façon de lire notes et étiquettes.

MÉTHODE CONSEILLÉE

1. Défile chaque rafale touche maintenue, relâche sur les moments forts, vérifie \
à la loupe, Espace sur les meilleures.
2. Filtre sur ✓ et repasse sur les retenues ; affine avec les notes ou les couleurs.
3. Ouvre les retenues (les RAW de préférence) dans ton logiciel de retouche.

À PROPOS

Projet personnel, publié sous licence MIT : https://github.com/xysticus/rafale-viewer
Il cible les boîtiers OM System / Olympus (rafales, réglages de l'appareil) et le \
bureau GNOME (copie vers Fichiers). Avec une autre marque ou un autre bureau, \
l'essentiel fonctionne, mais certaines fonctions sont à adapter.
Rafale est « vibe codé » : écrit par une IA (Claude Code) sous la direction de \
son auteur, et testé à l'usage. La façon conseillée de le modifier est la même : \
demander les changements à un agent de code, en partant de docs/CONCEPTION.md.
");

pub const SHORTCUTS_TITLE: &str = "Rafale — raccourcis";

pub const SHORTCUTS: &str = "\
DÉFILÉ

→ ou Page suivante : photo suivante ; maintenue, défilé continu
← ou Page précédente : photo précédente ; maintenue, défilé en arrière
↓ / ↑ : rafale suivante / précédente (vue À plat : rangée suivante / précédente)
Début / Fin : première / dernière photo

MARQUAGE

Espace : retenue (2 ★) ↔ non retenue (0 ★) ; retenue = 2 étoiles ou plus
1 à 5 : nombre d'étoiles (rangée du haut sans Maj, ou pavé numérique)
0 : efface les étoiles
X ou R : rejetée ↔ non rejetée
F1 à F5 : étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève)

COPIE

Ctrl+C : copie la sélection (filtrées, sinon retenues) ; Ctrl+V dans Fichiers

LOUPE

A : loupe sur la zone AF (cadre vert) ↔ loupe sous le pointeur
C : affiche ↔ masque le cadre vert de la zone AF
N : vue la plus nette de la rafale (netteté de la zone AF)

FENÊTRE

F ou F11 : plein écran
Échap ou Q : quitter

SOURIS

Pointeur sur la photo, à l'arrêt : loupe 100 %
Clic sur la photo : fige la loupe à cet endroit (un autre clic la déplace)
Clic droit : libère la loupe figée (libérée aussi en changeant de rafale)
Étoiles, couleurs, ✓, ✕ en haut du panneau : marquage de la photo affichée
(re-clic sur la même étoile ou couleur : l'enlève)
Mêmes commandes en bas : filtre ; « effacer » supprime le filtre
Clic sur une rafale ou une vignette : y aller
Molette sur le panneau, ou ascenseur : faire défiler la liste
JPEG / RAW / Les deux, puis « Copier … » : comme Ctrl+C
Rafales / À plat : vue de la liste (À plat : dossiers de moins de 300 photos)
Corbeille : N rejetées : met les photos rejetées à la corbeille (après confirmation)
";
