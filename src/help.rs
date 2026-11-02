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
boîtier (OM System) : elle suit le sujet, souvent le visage détecté. Masqué au \
démarrage, C l'affiche ou le masque. La touche A \
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
marquage. Un clic va à la photo ; ↑ et ↓ montent ou descendent d'une rangée. Les \
photos y sont isolées : pas de blocage ni de marquage des rafales.

JUGER LES RAFALES

• Une rafale a son propre marquage, indépendant de celui de ses photos : Maj+1 à \
Maj+5 la notent (Maj+0 efface), Maj+Espace la retient, Maj+X la rejette. La rangée \
« Rafale », sous celle de la photo, fait la même chose à la souris. Rejeter une \
rafale ne masque rien et ne touche pas à ses photos.
• Ce marquage va dans un petit fichier texte par rafale marquée, dans le dossier de \
la séance : PA290128-PA290142.rafale (première et dernière vue). Les autres \
logiciels ne le lisent pas.
• Entrée bloque la rafale de la photo affichée (bandeau bleu et cadenas) : ← → et \
le défilé ne sortent plus de la rafale, Début et Fin vont à sa première et à sa \
dernière vue, ↑ et ↓ passent à la rafale précédente ou suivante en restant \
bloqué. La liste devient une grille des vues de la rafale (régulièrement espacées \
si elle est longue). Entrée ou Échap la débloque.
• Rafale signale (⚠ dans la liste, consigne dans le panneau) une rafale retenue \
sans photo retenue, une rafale notée X sans photo à X étoiles ou plus, et une \
rafale rejetée qui contient des photos retenues. Retenir une photo ne retient pas \
sa rafale : on fait les deux. Ctrl+C et « Retoucher… » demandent confirmation \
tant qu'il reste des rafales signalées.

MARQUER

• Deux choses distinctes : les étoiles (1 à 5) notent la qualité, durablement ; \
la coche ✓ (Espace) dit « je la retouche ». Dix photos également bonnes gardent \
toutes leurs 3 étoiles, et une seule reçoit ✓. Espace coche ou décoche sans \
toucher aux étoiles, X ou R rejette, F1 à F5 posent une couleur. Les commandes en haut du panneau font la même chose \
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
• Rafale bloquée et rejetée : le bouton devient « Corbeille : rafale 12 (15 photos) » \
et met à la corbeille toute la rafale (photos, RAW, XMP, PP3 et son fichier \
.rafale), une rafale à la fois, après la même confirmation.

IMPORTER UNE CARTE

• Ouvre la carte mémoire avec Rafale (Fichiers : clic droit › Ouvrir avec › Rafale). \
Après un résumé et une confirmation, ses JPEG sont copiés dans ~/Images/AAAA-MM-JJ \
(jour de prise de vue) et ses RAW dans raw/, puis Rafale s'ouvre sur le jour \
importé. Les fichiers déjà importés sont sautés ; la carte n'est pas modifiée.

FILTRER

• La barre du bas filtre les photos par note minimale, couleur, retenue ou rejet, \
et, sur sa seconde rangée « Rafales », les rafales par note minimale ou retenue. \
Les deux se cumulent. Le défilé, les flèches, Début, Fin et la liste ne montrent \
alors que les photos qui passent le filtre.

COPIER LA SÉLECTION

• Ctrl+C, ou le bouton « Copier … », met la sélection dans le presse-papiers : \
les photos filtrées si un filtre est actif, sinon les photos retenues. Un Ctrl+V \
dans un dossier ouvert dans Fichiers y copie les fichiers.
• Le choix JPEG / RAW / Les deux décide des fichiers copiés. On peut ainsi donner \
à n'importe quel logiciel de retouche un dossier qui ne contient que les photos \
choisies, sans dépendre de sa façon de lire notes et étiquettes.

RETOUCHER

• « Retoucher… » copie la même sélection que Ctrl+C (JPEG, RAW ou les deux, avec \
leurs XMP) dans un dossier voisin de la séance, propre au logiciel choisi, par \
exemple 2026-10-29-rawtherapee (2026-10-29-retouche pour Fichiers), puis y lance \
le logiciel. Chaque logiciel a ainsi sa copie, pour les essayer séparément. Il ne voit que les photos choisies, et ses \
fichiers restent dans la copie : la séance triée n'est pas touchée. Le logiciel \
est demandé au premier usage, puis retenu ; Maj+clic pour en changer. Une photo \
déjà copiée est sautée, et un XMP déjà présent n'est jamais écrasé.

MÉTHODE CONSEILLÉE

1. Premier passage, toute la séance : défile touche maintenue. Sur une rafale qui \
accroche, Entrée pour la bloquer, la parcourir et regarder la grille ; note-la \
(Maj+1 à 5), retiens-la (Maj+Espace) ou rejette-la (Maj+X) ; Entrée, et continue.
2. Second passage : filtre « Rafales ✓ », bloque la première, puis ↓ de rafale en \
rafale. Dans chacune, trouve la meilleure vue (N pour la plus nette, loupe AF, \
grille), note-la et coche-la avec Espace.
3. Les ⚠ disent ce qu'il reste à faire ; quand il n'y en a plus, « Retoucher… » \
ou Ctrl+C part sans avertissement : seules les photos cochées partent.
4. Ménage : bloque chaque rafale rejetée et mets-la à la corbeille.

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
Début / Fin : première / dernière photo (rafale bloquée : de la rafale)

RAFALE BLOQUÉE

Entrée : bloque ↔ débloque la rafale de la photo affichée
Échap : débloque la rafale (sinon : quitte le plein écran)

MARQUAGE

Espace : retenue ✓ ↔ non retenue (les étoiles ne changent pas)
1 à 5 : nombre d'étoiles (rangée du haut sans Maj, ou pavé numérique)
0 : efface les étoiles
X ou R : rejetée ↔ non rejetée
F1 à F5 : étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève)

MARQUAGE DE LA RAFALE

Maj+Espace : rafale retenue ↔ non retenue
Maj+1 à Maj+5 : note de la rafale ; Maj+0 l'efface
Maj+X ou Maj+R : rafale rejetée ↔ non rejetée

COPIE

Ctrl+C : copie la sélection (filtrées, sinon retenues) ; Ctrl+V dans Fichiers

LOUPE

A : loupe sur la zone AF (cadre vert) ↔ loupe sous le pointeur
C : affiche ↔ masque le cadre vert de la zone AF
N : vue la plus nette de la rafale (netteté de la zone AF)

FENÊTRE

F ou F11 : plein écran
Ctrl+Q : quitter

SOURIS

Pointeur sur la photo, à l'arrêt : loupe 100 %
Clic sur la photo : fige la loupe à cet endroit (un autre clic la déplace)
Clic droit : libère la loupe figée (libérée aussi en changeant de rafale)
Étoiles, couleurs, ✓, ✕ en haut du panneau : marquage de la photo affichée
(re-clic sur la même étoile ou couleur : l'enlève)
Rangée « Rafale » : marquage de la rafale de la photo affichée
Mêmes commandes en bas : filtre des photos, puis rangée « Rafales » : filtre des
rafales ; « effacer » supprime les deux
Clic sur une rafale ou une vignette : y aller
Molette sur le panneau, ou ascenseur : faire défiler la liste
JPEG / RAW / Les deux, puis « Copier … » : comme Ctrl+C
« Retoucher… » : copie la sélection à côté de la séance et lance le logiciel de retouche
Maj+clic sur « Retoucher… » : choisir un autre logiciel
Rafales / À plat : vue de la liste (À plat : dossiers de moins de 300 photos)
Corbeille : N rejetées : met les photos rejetées à la corbeille (après confirmation)
Corbeille : rafale N : rafale bloquée et rejetée, toute la rafale à la corbeille
";
