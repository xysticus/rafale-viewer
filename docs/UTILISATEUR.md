# Rafale — guide d'utilisation

Rafale est un visionneur fait pour trier des rafales de JPEG. Tu maintiens une flèche et les photos défilent en continu, sans en sauter aucune. Tu relâches, et la photo s'affiche en pleine résolution, avec une loupe 100 % pour vérifier la netteté. Les photos que tu retiens sont marquées dans des fichiers XMP, que les logiciels de retouche savent lire. Les rafales ont leur propre marquage, pour choisir d'abord les séquences qui valent le coup, puis la meilleure vue de chacune.

Rafale est conçu pour les boîtiers **OM System / Olympus** et le bureau **GNOME**. Avec une autre marque ou un autre bureau, l'essentiel fonctionne, mais la détection des rafales, les réglages de l'appareil et la copie vers le gestionnaire de fichiers sont à adapter : voir le [README](../README.md).

## Installation

Il faut Rust (`cargo`), ainsi que `cmake` et `nasm` pour compiler libjpeg-turbo :

```
sudo dnf install cargo cmake nasm
cd rafale-viewer
cargo build --release
```

Le programme se trouve ensuite dans `target/release/rafale-viewer`.

**Intégration à GNOME** (sans sudo, dans ton dossier personnel) :

```
./packaging/install.sh
```

Le script copie le programme dans `~/.local/bin`, et installe l'icône et l'entrée de menu dans `~/.local/share`. Relance-le après chaque `cargo build --release` pour mettre le programme à jour. Les applications par défaut ne changent pas : Fichiers ouvre toujours les dossiers, et la visionneuse d'images les JPEG.

## Lancement

- **Menu des applications :** « Rafale ». Une fenêtre demande le dossier à trier ; elle s'ouvre sur le dernier dossier ouvert, qu'il suffit de valider pour le reprendre.
- **Fichiers :** clic droit sur un dossier, sur une photo ou sur le fond de la fenêtre, puis **Ouvrir dans Rafale**. Sur une photo, Rafale ouvre son dossier en commençant par elle. Cette entrée demande le paquet `nautilus-python` (`sudo dnf install nautilus-python`, puis `./packaging/install.sh` et `nautilus -q`) ; sans lui, passe par *Ouvrir avec… › Rafale*.
- **Terminal :**

```
rafale-viewer [dossier | photo.jpg] [--fps N]
```

- `dossier` : le dossier qui contient les JPEG. Seuls les fichiers `.jpg` et `.jpeg` sont pris, dans l'ordre alphabétique de leur nom. Sans argument, une fenêtre demande le dossier.
- `photo.jpg` : ouvre le dossier de cette photo, en commençant par elle.

**Reprise :** à la réouverture d'un dossier, Rafale repart de la dernière photo vue, pour un tri fait en plusieurs fois. Ouvrir une photo précise l'emporte. La position est gardée hors du dossier des photos, dans `~/.local/state/rafale/positions` (les 200 derniers dossiers).
- `--fps N` : cadence du défilé quand une flèche est maintenue. Elle vaut 25 images/s par défaut.
- `--bench-scroll S` : mesure du défilé. Rafale attend que la première photo soit prête, maintient → pendant S secondes, relâche, puis affiche la cadence obtenue, la régularité des images et le délai de la pleine résolution, et se ferme. Utile pour comparer deux machines ou vérifier qu'une évolution ne ralentit pas le défilé, par exemple `rafale-viewer ~/Images/2026-10-29 --bench-scroll 5 --fps 25`.
- `--version` : affiche la version. Elle figure aussi en tête de la fenêtre « Principes ».

Exemple :

```
rafale-viewer ~/Images/2026-10-29
```

## Importer une carte mémoire

Insère la carte, ouvre-la dans Fichiers, puis clic droit sur la carte (ou sur son dossier `DCIM`) › *Ouvrir avec… › Rafale*. En terminal : `rafale-viewer /run/media/$USER/<carte>`.

Rafale recense la carte et affiche un résumé avant de copier : nombre de JPEG et de RAW, taille, et pour chaque jour de prise de vue le dossier de destination. Après confirmation, une barre de progression suit la copie, puis Rafale s'ouvre sur le jour importé (le plus récent s'il y en a plusieurs).

- **Destination :** `~/Images/AAAA-MM-JJ/` d'après la date de prise de vue de chaque photo ; une carte qui couvre plusieurs jours donne plusieurs dossiers. Les RAW vont dans `raw/`.
- **Déjà importé :** un fichier de même nom et de même taille est sauté. On peut relancer l'import sans risque, par exemple après une interruption.
- **Non copiés, et signalés dans le résumé :** les vidéos et autres fichiers, les fichiers sans date lisible, et un fichier de même nom mais de taille différente déjà présent.
- **La carte n'est jamais modifiée.** Formate-la dans l'appareil quand tu veux.
- Chaque fichier est copié sous un nom temporaire (`.part`) puis renommé : une interruption ne laisse jamais de photo tronquée. Chaque fichier copié est daté de sa prise de vue (heure inscrite par l'appareil), et non de l'import. Rafale n'utilise pas la date des fichiers de la carte, souvent décalée d'une heure ou plus, car les cartes FAT ne connaissent pas les fuseaux horaires.

## L'écran

```
┌──────────────────────────────────────┬──────────────────────┐
│                                      │ ★★★☆☆ ■■■ [✓][✕] ◐GND│
│                                      │ Rafale ★★☆☆☆  [✓] [✕]│
│                                      │ PA290123.JPG  123/235│
│              photo                   │ 1/500 s f/4 ISO … ·… │
│        (calée à gauche ;             │ RAW : raw/…  Rafale 1│
│     portrait : centrée dans          │ Netteté AF …    Loupe│
│        la zone libre)                │ ⚠ consigne éventuelle│
│                                      ├──────────────────────┤
│                                      │ Rafale 11  20:45:07 ▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ Rafale 12  …  25 i/s▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ ━━━ (position)       │
│                                      ├──────────────────────┤
│                                      │ ★★☆☆☆ ■■■■■ [ ] [ ] │ ← filtre photos
│                                      │ Rafales ★☆☆☆☆    [ ] │ ← filtre rafales
│                                      │ [JPEG][RAW][…] Copier│ ← copie
│                                      │ [Rafales][À plat]  … │ ← vue, aide
└──────────────────────────────────────┴──────────────────────┘
```

Le panneau de droite occupe la place que laisse une photo 4:3. Il comprend trois parties.

**Tout en haut, les outils** : **L** loupe, **A** loupe AF, **C** cadre AF, **N** vue la plus nette, **G** ligne du Live GND. Un clic fait la même chose que la touche. Fond bleu : actif ; contour : coupé ; grisé : sans effet sur cette photo (pas de zone AF, pas de rafale à mesurer, pas de Live GND). N est une action, sans état.

**En haut, la photo affichée :**
- les commandes de marquage de la photo (étoiles, couleur, retenue, rejet), l'état de la photo en toutes lettres et, s'il a servi, le Live GND (voir « Live GND ») ; dessous, celles de sa rafale (rangée « Rafale » : étoiles, retenue, rejet) ;
- le nom du fichier et, à droite, la position dans le dossier ;
- l'exposition (vitesse, ouverture, ISO), le mode image et le filtre de bruit de l'appareil ;
- le RAW associé et, à droite, la rafale, la place dans la rafale et le rythme ;
- la netteté AF et, à droite, l'état de la loupe ;
- en orange, si la rafale ne respecte pas une règle de cohérence, ce qui ne va pas et quoi faire (voir « Juger les rafales »).

**La liste des rafales :** chaque ligne indique le numéro, l'heure, le nombre d'images, le rythme quand il est connu, puis à droite le marquage de la rafale (étoiles et ✓, ou ✕), un ⚠ si elle est signalée, et « ✓ n » s'il y a des photos retenues. Elle montre 4 vignettes réparties sur la rafale. La rafale courante est surlignée, et un petit trait bleu sous ses vignettes montre où tu en es.

**En bas, la barre de filtre** des photos, puis celle des rafales (voir « Souris »), puis la ligne de copie (voir « Copier la sélection »), puis une ligne de boutons : **Rafales / À plat** à gauche, **Principes** et **Raccourcis** à droite.

**Vue au lancement :** À plat, sauf si Rafale détecte une rafale de plus de 5 photos ; dans ce cas, vue Rafales. Les boutons permettent toujours de changer.

**Vue À plat.** Pour un dossier de moins de 300 photos, le bouton « À plat » remplace la liste des rafales par une grille : une vignette par photo, avec son marquage (étoiles ou croix de rejet en haut à gauche, couleur en haut à droite). Un cadre bleu entoure la photo affichée. Un clic sur une vignette affiche la photo. Le filtre et le marquage des photos fonctionnent comme en vue Rafales. ↑ et ↓ montent ou descendent d'une rangée. Les photos y sont isolées : le blocage, le marquage et le filtre des rafales y sont désactivés.

**Principes et Raccourcis** ouvrent une fenêtre d'aide dont on peut sélectionner et copier le texte. Tant qu'elle est ouverte, la fenêtre de Rafale ne réagit plus ; ferme l'aide pour reprendre.

## Clavier

| Touche | Action |
|---|---|
| → ou Page Suiv. | Photo suivante. Maintenue : défilé continu. |
| ← ou Page Préc. | Photo précédente. Maintenue : défilé continu en arrière. |
| ↓ | Début de la rafale suivante (vue À plat : rangée suivante) |
| ↑ | Début de la rafale précédente (vue À plat : rangée précédente) |
| Début / Fin | Première / dernière photo (rafale bloquée : de la rafale) |
| Entrée | Bloque ↔ débloque la rafale de la photo affichée |
| Échap | Débloque la rafale ; sinon, quitte le plein écran |
| Espace | Retenue ✓ ↔ non retenue (les étoiles ne changent pas) |
| 1 à 5 | Nombre d'étoiles (rangée du haut, sans Maj, ou pavé numérique) |
| 0 | Efface les étoiles |
| X ou R | Rejetée ↔ non rejetée |
| F1 à F5 | Étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève) |
| Maj+Espace | Rafale retenue ↔ non retenue |
| Maj+1 à Maj+5 | Note de la rafale ; Maj+0 l'efface |
| Maj+X ou Maj+R | Rafale rejetée ↔ non rejetée |
| Ctrl+C | Copie la sélection dans le presse-papiers (voir « Copier la sélection ») |
| L | Active ↔ coupe la loupe (coupée au démarrage) |
| A | Active la loupe sur la zone AF de chaque vue ↔ loupe sous le pointeur |
| N | Vue la plus nette de la rafale (voir « Netteté AF ») |
| C | Affiche ↔ masque le cadre vert de la zone AF |
| G | Affiche ↔ masque la ligne de transition du Live GND sur la photo |
| F ou F11 | Plein écran |
| Ctrl+O | Ouvrir un autre dossier, sans fermer la fenêtre |
| Ctrl+Q | Quitter |

**Changer de dossier (Ctrl+O).** La fenêtre de choix s'ouvre sur le dossier parent de la séance, là où sont les autres séances. Rafale reprend à la dernière photo vue du nouveau dossier, et retiendra celle de la séance quittée. Le filtre et la vue repartent de zéro ; la loupe, le cadre AF, la ligne GND et le choix JPEG / RAW restent. Un dossier sans JPEG laisse la séance en cours, avec un message à côté du bouton de copie. Une carte mémoire s'importe toujours en l'ouvrant depuis Fichiers.

**Le défilé.** Un appui bref avance d'une seule photo. Si tu maintiens la touche plus de 0,3 s, le défilé continu démarre. Il ne saute jamais d'image : si le disque ou le décodage ne suivent pas, il ralentit au lieu de sauter des photos. Pendant le défilé, les photos sont affichées en aperçu réduit. Au relâchement, la pleine résolution arrive en environ un quart de seconde.

**Les touches ↑ et ↓** font un saut par appui. Les maintenir ne fait pas défiler les rafales. Rafale bloquée, elles passent à la rafale précédente ou suivante, qui est bloquée à son tour.

## Souris

- **Loupe 100 %.** Elle est coupée au démarrage : **L** l'active, et la coupe à nouveau (loupe figée et loupe AF comprises). À l'arrêt, une fois la pleine résolution chargée, place le pointeur sur l'image : un carré de 600 px montre les pixels réels autour du pointeur, par exemple le visage de la cavalière. Le pointeur est masqué tant que la loupe est affichée. La loupe disparaît dès que le pointeur sort de l'image ou que le défilé reprend.
- **Loupe figée.** Loupe activée, un **clic sur la photo** la fige à cet endroit : elle y reste quand tu passes d'une vue à l'autre avec ← et →, ce qui permet de comparer la netteté du même visage sur toute une rafale sans bouger la souris. Elle est alors bordée de bleu, et le panneau indique « Loupe figée ». Un autre clic la déplace ; un **clic droit** la libère. Elle est aussi libérée dès que tu passes à une autre rafale, dont le cadrage est différent.
- **Zone AF.** Un cadre vert montre sur la photo la zone de mise au point enregistrée par le boîtier (OM System / Olympus). Elle suit le sujet vue après vue, le plus souvent le visage détecté par l'appareil. La touche **C** l'affiche ou la masque (masquée au lancement) ; la loupe AF fonctionne dans les deux cas.
- **Loupe AF (touche A).** A active la loupe, qui se place d'elle-même sur la zone AF de chaque vue, et suit donc le visage dans toute la rafale, même s'il se déplace dans le cadre. Sa bordure est verte, et le panneau indique « Loupe sur la zone AF ». Une photo sans zone AF retombe sur la loupe sous le pointeur. Un clic sur la photo fige la loupe, ce qui l'emporte sur la loupe AF ; A la coupe.
- **Netteté AF.** À l'arrêt, Rafale mesure en tâche de fond la netteté de la zone AF de chaque vue de la rafale, en commençant par la photo affichée puis ses voisines. Le panneau affiche « Netteté AF : 87 % de la plus nette · rang 4 / 54 », en vert pour la plus nette, avec « (mesure 30 / 54) » tant que toute la rafale n'est pas mesurée. **N** va à la vue la plus nette (parmi celles qui passent le filtre). La mesure ne ralentit pas le défilé : elle s'interrompt dès que tu reprends. Deux limites : l'indice ne compare que les vues d'une même rafale (rien pour une vue isolée), et il mesure ce qui est dans la zone AF : si l'AF a visé le col plutôt que le visage, l'indice baisse aussi. Regarde alors le cadre vert. Comme la loupe normale, elle s'affiche à l'arrêt, une fois la pleine résolution chargée.
- **Commandes en haut du panneau** (marquage de la photo affichée) : clic sur une étoile = cette note (re-clic sur la même = aucune étoile) ; clic sur une couleur = cette étiquette (re-clic = aucune) ; case ✓ = retenue / non retenue, comme Espace ; case ✕ = rejetée / non rejetée, comme X. Une case décochée montre sa coche ou sa croix en gris. La rangée « Rafale », en dessous, marque de la même façon la rafale de la photo affichée.
- **Barre de filtre en bas de la liste** (mêmes commandes) : seules les photos qui passent le filtre sont montrées, dans la liste comme au défilé (←, →, ↑, ↓, Début, Fin). Une étoile = au moins cette note ; plusieurs couleurs = l'une ou l'autre ; ✓ et ✕ = retenues, rejetées, ou les deux si les deux sont cochées. Les critères se cumulent. La rangée « Rafales », en dessous, filtre les rafales : une étoile = rafales notées au moins autant, ✓ = rafales retenues ; seules les photos des rafales qui passent sont montrées, et le filtre des photos s'y ajoute. « effacer » supprime les deux. Si la photo affichée ne passe plus le filtre après un marquage, elle reste à l'écran jusqu'au prochain déplacement.
- **Clic sur une ligne de la liste :** affiche la première photo de cette rafale (la première qui passe le filtre). En vue À plat, ou dans la grille d'une rafale bloquée, un clic sur une vignette affiche cette photo.
- **Molette au-dessus du panneau :** fait défiler la liste. Elle se recentre dès que tu changes de rafale.
- **Ascenseur à droite de la liste :** tu peux le faire glisser. Un clic sur la piste y amène directement le curseur.

## Live GND

Si la photo a été prise avec le Live GND du boîtier (dégradé calculé par l'appareil, OM-1 Mk II), le haut du panneau affiche son code comme à l'écran de l'appareil : **GND4-S** pour une densité de 4 (2 EV ; GND2 = 1 EV, GND8 = 3 EV) et une transition douce (S ; M moyenne, H dure). Le **disque** à côté montre l'orientation approximative de la ligne : le demi-disque noir est du côté sombre, le blanc du côté clair.

**G** dessine la ligne de transition sur la photo. Elle n'est pas affichée au lancement. Pour une photo recadrée dans un autre format que le 4:3, sa position est approximative. Ces informations ne sont pas documentées par le fabricant : elles ont été relevées sur des vues d'essai, à l'écran du boîtier.

## Marquer les photos

Le marquage est enregistré dans un petit fichier à côté de chaque photo marquée : `PA290123.JPG.xmp`. Les JPEG ne sont jamais modifiés. Si tu enlèves tout le marquage d'une photo (aucune étoile, aucune couleur, pas de rejet), son fichier `.xmp` est supprimé.

**Les étoiles et la retenue sont deux choses distinctes.**

- **Les étoiles (1 à 5) notent la qualité**, et rien ne les change à ta place : tu peux toujours revoir plus tard toutes tes bonnes photos avec le filtre étoiles.
- **La retenue (✓, touche Espace ou case ✓) veut dire « je la retouche ».** Dans une rafale où dix photos sont également bonnes, les dix gardent leurs 3 étoiles et une seule reçoit ✓.
- Le filtre ✓, la copie sans filtre (Ctrl+C), « Retoucher » et le compte « ✓ n » des rafales ne prennent que les photos cochées. Espace coche ou décoche sans toucher aux étoiles ; rejeter (X) enlève la coche.
- Les autres logiciels n'ont pas besoin de connaître la coche : « Retoucher » ne leur donne que les photos cochées, avec leurs étoiles. digiKam l'affiche comme « acceptée ».
- **Séances triées avant cette version :** leurs photos n'ont aucune coche (les étoiles sont conservées).

## Juger les rafales

Le tri se fait à deux niveaux : **quelles rafales valent le coup**, puis **quelle est la meilleure photo de chacune**. Une rafale a donc son propre marquage, indépendant de celui de ses photos.

**Marquer une rafale.** Maj+1 à Maj+5 la notent, Maj+0 efface la note, Maj+Espace la retient, Maj+X (ou Maj+R) la rejette. La rangée « Rafale » du panneau fait la même chose à la souris. Le marquage vise la rafale de la photo affichée, bloquée ou non. Comme pour une photo, rejeter une rafale efface sa note et sa coche, et la noter ou la retenir lève le rejet. Le rejet est une marque : il ne masque rien et ne touche pas aux photos.

**Le fichier `.rafale`.** Chaque rafale marquée a un petit fichier texte dans le dossier de la séance, nommé d'après sa première et sa dernière vue : `PA290128-PA290142.rafale`. Il est lisible et modifiable à la main :

```
rafale-viewer 3
premiere = PA290128.JPG
derniere = PA290142.JPG
vues = 15
note = 3
retenue = oui
rejetee = non
```

Au lancement, il s'applique à la rafale qui contient sa première vue, ou à défaut sa dernière (si la première a été supprimée). Il est renommé à la prochaine écriture si les bornes de la rafale ont changé, et supprimé quand tout son marquage est effacé. Les autres logiciels ne le lisent pas : la note d'une rafale ne sert qu'au tri dans Rafale. Le marquage et le blocage des rafales attendent la fin de l'analyse des rafales (« Analyse des rafales… » en haut de la liste).

**Bloquer une rafale.** Entrée bloque la rafale de la photo affichée. Un bandeau bleu à cadenas en tête de la liste, un cadenas devant « Rafale » et le titre de la fenêtre l'indiquent. Une fois la rafale bloquée :

- ← →, le défilé continu, Début et Fin restent dans la rafale ;
- ↑ ↓ passent à la rafale précédente ou suivante, bloquée à son tour ;
- la liste devient une grille des vues de la rafale : toutes si elles tiennent, sinon autant de rangées de 4 que le panneau en contient, régulièrement espacées (« 16 vues sur 64 »). Chaque vignette porte le marquage de sa vue ; le cadre bleu suit la vue affichée, et un clic va à la vue ;
- le filtre s'applique toujours.

Entrée ou Échap débloque la rafale.

**Règles de cohérence.** Rafale signale une rafale :

1. retenue sans aucune photo retenue ;
2. notée X sans aucune photo notée X ou plus (photos rejetées exclues) ;
3. rejetée qui contient des photos retenues.

Le signalement est un ⚠ sur la ligne de la rafale, et une consigne en orange dans le panneau : « Rafale notée 4 : aucune photo à 4 étoiles ou plus. Trouve-la, ou baisse la note de la rafale. » Retenir une photo ne retient pas sa rafale : Espace et Maj+Espace sont indépendants, et c'est la règle 1 qui rappelle de faire les deux. Tant qu'il reste des rafales signalées, Ctrl+C et « Retoucher » demandent confirmation, avec la liste (« 2 rafales retenues sans photo retenue : 12, 37 ») ; sinon la copie part directement.

**Méthode conseillée.**

1. **Premier passage, toute la séance :** défilé continu. Sur une rafale qui accroche, Entrée pour la bloquer, la parcourir et regarder la grille ; la noter (Maj+1 à 5), la retenir (Maj+Espace) ou la rejeter (Maj+X) ; Entrée, et on continue.
2. **Second passage :** filtre « Rafales ✓ », Entrée sur la première, puis ↓ de rafale retenue en rafale retenue. Dans chacune, trouver la meilleure vue (N pour la plus nette, loupe AF, grille), lui donner sa note et la cocher (Espace).
3. **Les ⚠ disent ce qu'il reste à faire.** Quand il n'y en a plus, « Retoucher » part sans avertissement.
4. **Ménage :** bloquer chaque rafale rejetée et la mettre à la corbeille (voir plus bas).

### Les RAW

Si la photo a un RAW de même nom (`PA290123.ORF`, `.DNG`, `.CR3`, `.NEF`…), dans le même dossier ou dans un sous-dossier direct comme `raw/`, le même marquage est écrit à côté du RAW : `raw/PA290123.ORF.xmp`. Le panneau affiche « RAW : raw/PA290123.ORF », ou « pas de RAW ».

- **Rattrapage :** au lancement, les photos déjà marquées dont le RAW n'a pas encore reçu le marquage sont mises à jour.
- **Ordre conseillé :** fais le tri **avant** d'importer les RAW dans darktable. Dès que tu retouches une photo dans darktable, son `.xmp` contient l'historique : Rafale ne le modifie plus, et le panneau indique « XMP externe, non synchronisé ».

### Dans les logiciels de retouche

| Logiciel | Ce qu'il lit | À savoir |
|---|---|---|
| darktable | étoiles, rejet, couleur | Importer avec **« ajouter à la bibliothèque »**. « Copier et importer » ignore les `.xmp`. Après l'import, c'est la base de données de darktable qui fait foi. |
| digiKam | étoiles, couleur, retenue / rejetée | Activer la lecture des sidecars : *Configuration › Métadonnées › Sidecars*. |
| RawTherapee (5.11 et plus) | étoiles, couleur (pas le rejet) | Activer la synchronisation des notes et couleurs avec les XMP (Préférences › Navigateur de fichiers). Mettre `XMPSidecarStyle=ext` dans la section `[Metadata]` du fichier `options` de RawTherapee (version Flatpak : `~/.var/app/com.rawtherapee.RawTherapee/config/RawTherapee/options` ; paquet classique : `~/.config/RawTherapee/options`), RawTherapee fermé, pour qu'il lise `PA290123.ORF.xmp` au lieu de `PA290123.xmp`. Une photo rejetée y apparaît sans étoile. |
| Lightroom | étoiles, rejet, couleur | Conventions Lightroom respectées (`xmp:Rating`, `xmp:Label`). |

**Le fichier XMP d'un autre logiciel n'est pas modifié s'il contient quelque chose** : une note, une couleur, un historique de retouches darktable, un titre, des mots-clés… Rafale le lit mais n'y touche pas, et le panneau indique « XMP externe, lecture seule ». Les touches de marquage sont alors sans effet sur cette photo.

Un XMP **vierge** d'un autre logiciel, comme ceux que darktable crée pour chaque photo à l'import, est remplacé par celui de Rafale dès que tu marques la photo. Tant que tu ne la marques pas, Rafale n'y touche pas. Tu peux donc essayer un autre logiciel sur une séance sans t'empêcher d'y trier ensuite. darktable ne relit pas tout seul les `.xmp` modifiés : utilise sa commande de relecture des sidecars.

## Copier la sélection

Pour donner à un logiciel de retouche un dossier qui ne contient que les photos choisies, sans dépendre de sa façon de lire notes et étiquettes :

1. Coche en bas les fichiers voulus : **JPEG** (coché au départ), **RAW**, **XMP**, indépendamment. XMP ajoute les `.xmp` des photos copiées, ou ceux des JPEG si ni JPEG ni RAW n'est coché : de quoi donner les marquages à un logiciel essayé sur une copie des photos.
2. **Ctrl+C**, ou clic sur **« Copier »**. La sélection, ce sont les photos qui passent le filtre si un filtre (des photos ou des rafales) est actif, sinon les photos retenues ; leur nombre est écrit devant le bouton (« 12 retenues »). S'il reste des rafales signalées, une confirmation les récapitule d'abord.
3. Dans **Fichiers**, ouvre le dossier de destination et fais **Ctrl+V** : les fichiers y sont copiés.

Le compte rendu s'affiche à droite des cases, par exemple « 24 fichiers copiés, 2 sans RAW ». Le presse-papiers reste rempli même après la fermeture de Rafale, jusqu'à la prochaine copie. Il faut le paquet `wl-clipboard` (installé d'office sur Fedora).

## Retoucher la sélection

Le bouton **« Retoucher »** prépare la retouche en une seule action :

1. il copie la même sélection que Ctrl+C (les photos filtrées si un filtre est actif, sinon les retenues), avec les mêmes cases : JPEG et RAW pour les photos (au moins l'une des deux), **XMP pour leurs notes et couleurs**, dans un dossier **voisin** de la séance, **propre au logiciel** : `~/Images/2026-10-29-rawtherapee/`, `2026-10-29-darktable/`… (`2026-10-29-retouche/` pour Fichiers). Chaque logiciel a ainsi sa propre copie, pour les essayer séparément sans qu'ils se mélangent ;
2. il lance le logiciel de retouche sur ce dossier.

Le logiciel ne voit que les photos choisies, et il écrit ses propres fichiers (`.pp3`, `.xmp` de darktable…) dans la copie : la séance triée et les originaux ne sont jamais touchés.

- **Choix du logiciel :** au premier usage, une liste des logiciels installés (RawTherapee, darktable, Showfoto, XnView MP, gThumb, ou simplement Fichiers). Le choix est retenu (`~/.config/rafale/retouche`), et le bouton **« Logiciel : … »**, entre « Copier » et « Retoucher », permet d'en changer sans lancer de retouche.
- **Plusieurs passages :** une photo déjà copiée est sautée, et un `.xmp` déjà présent dans le dossier de retouche n'est jamais écrasé, car le logiciel a pu y écrire ses réglages.
- darktable importe le dossier dans sa bibliothèque ; gThumb enregistre ses retouches par-dessus le fichier, ici la copie.

## Mettre les rejetées à la corbeille

Quand des photos sont marquées rejetées (croix rouge, touche X ou R), un bouton rouge **« Corbeille : N rejetées »** apparaît en bas du panneau. Il met à la corbeille de GNOME, pour chaque photo rejetée :

- le JPEG et le RAW ;
- tous leurs fichiers d'accompagnement : `.xmp` de Rafale ou d'autres logiciels (y compris les doublons darktable `PA….ORF_01.xmp` et les `PA….xmp`), `.pp3` de RawTherapee.

Une fenêtre demande d'abord confirmation, avec le nombre de fichiers et la place libérée. Seules les photos explicitement rejetées partent : une photo sans étoile n'est pas concernée. Rafale relit ensuite le dossier sans se fermer, en gardant le filtre et la vue. Tout reste récupérable dans la corbeille (Fichiers › Corbeille, ou `gio trash --restore`) jusqu'à ce qu'elle soit vidée.

**Une rafale entière.** Quand la rafale affichée est bloquée et rejetée, le bouton devient **« Corbeille : rafale 12 (15 photos) »** : il met à la corbeille toutes les photos de la rafale (JPEG, RAW, `.xmp`, `.pp3`) et son fichier `.rafale`, après la même confirmation, qui signale aussi les photos retenues qu'elle contiendrait. Les rafales partent ainsi une par une, en connaissance de cause. Rafale reprend ensuite sur la photo qui suit.

## Rafales et réglages de l'appareil

- **Rafales :** elles sont reconnues grâce au numéro de vue que les boîtiers OM System et Olympus inscrivent dans chaque fichier (1, 2, 3…). Une nouvelle rafale commence quand ce numéro repart à 1. Pour les autres appareils, Rafale se sert de l'heure de prise de vue : plus d'une seconde d'écart entre deux photos, et c'est une nouvelle rafale.
- **Rythme (images/s) :** l'heure de prise de vue n'est enregistrée qu'à la seconde près, donc le rythme n'est exact que pour une rafale qui s'étale sur au moins trois secondes distinctes, soit environ 30 images à 25 i/s. Pour les rafales plus courtes, rien n'est affiché.
- **Mode image et filtre de bruit :** ils sont lus dans les données du boîtier (OM System / Olympus uniquement). Les noms de modes sont ceux d'exiftool, en anglais (« Natural », « i-Enhance »). Ils ne correspondent pas aux numéros du menu de l'appareil : par exemple, i-Enhance est le n° 1 du menu et Natural le n° 3.

## Conseils

- **Disque :** le défilé à froid (photos jamais ouvertes) est limité par le disque. Sur le SSD SATA en NTFS, on mesure environ 32 photos/s pour des JPEG de 15 Mo. Sur le NVMe, il y a de la marge. Pour une grosse séance, copier d'abord le dossier sur le NVMe rend le tri plus confortable.
- **Démarrage :** la liste des rafales se remplit en quelques fractions de seconde à quelques secondes, selon le disque. Pendant ce temps, le haut de la liste affiche « Analyse des rafales… ».
