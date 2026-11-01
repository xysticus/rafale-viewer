# Rafale — guide d'utilisation

Rafale est un visionneur fait pour trier des rafales de JPEG. Tu maintiens une flèche et les photos défilent en continu, sans en sauter aucune. Tu relâches, et la photo s'affiche en pleine résolution, avec une loupe 100 % pour vérifier la netteté. Les photos que tu retiens sont marquées dans des fichiers XMP, que les logiciels de retouche savent lire.

Rafale est conçu pour les boîtiers **OM System / Olympus** et le bureau **GNOME**. Avec une autre marque ou un autre bureau, l'essentiel fonctionne, mais la détection des rafales, les réglages de l'appareil et la copie vers le gestionnaire de fichiers sont à adapter : voir le [README](../README.md).

## Installation

Il faut Rust (`cargo`), ainsi que `cmake` et `nasm` pour compiler libjpeg-turbo :

```
sudo dnf install cargo cmake nasm
cd rafale-viewer
cargo build --release
```

Le programme se trouve ensuite dans `target/release/rafale-viewer`.

## Lancement

```
rafale-viewer <dossier> [--fps N]
```

- `<dossier>` : le dossier qui contient les JPEG. Seuls les fichiers `.jpg` et `.jpeg` sont pris, dans l'ordre alphabétique de leur nom.
- `--fps N` : cadence du défilé quand une flèche est maintenue. Elle vaut 25 images/s par défaut.
- `--version` : affiche la version. Elle figure aussi en tête de la fenêtre « Principes ».

Exemple :

```
rafale-viewer ~/Images/2026-10-29
```

## L'écran

```
┌──────────────────────────────────────┬──────────────────────┐
│                                      │ ★★★☆☆ ■■■■■ [✓] [✕] │
│                                      │ Retenue              │
│                                      │ PA290123.JPG         │
│                                      │ 1/500 s  f/4  ISO …  │
│              photo                   │ Natural · filtre …   │
│        (calée à gauche ;             │ RAW : raw/PA….ORF    │
│     portrait : centrée dans          │ 123 / 2355           │
│        la zone libre)                │ Rafale 12  vue 5/54  │
│                                      ├──────────────────────┤
│                                      │ Rafale 11  20:45:07 ▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ Rafale 12  …  25 i/s▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ ━━━ (position)       │
│                                      ├──────────────────────┤
│                                      │ ★★☆☆☆ ■■■■■ [ ] [ ] │ ← filtre
│                                      │ [JPEG][RAW][…] Copier│ ← copie
│                                      │ [Rafales][À plat]  … │ ← vue, aide
└──────────────────────────────────────┴──────────────────────┘
```

Le panneau de droite occupe la place que laisse une photo 4:3. Il comprend trois parties.

**En haut, la photo affichée :**
- les commandes de marquage (étoiles, couleur, retenue, rejet) et l'état en toutes lettres ;
- le nom du fichier, puis l'exposition (vitesse, ouverture, ISO), le mode image et le filtre de bruit de l'appareil ;
- le RAW associé, la position dans le dossier, la rafale, la place dans la rafale et le rythme.

**La liste des rafales :** chaque ligne indique le numéro, l'heure, le nombre d'images, le rythme quand il est connu, et « ★ n » s'il y a des photos retenues. Elle montre 4 vignettes réparties sur la rafale. La rafale courante est surlignée, et un petit trait bleu sous ses vignettes montre où tu en es.

**En bas, la barre de filtre** (voir « Souris »), puis la ligne de copie (voir « Copier la sélection »), puis une ligne de boutons : **Rafales / À plat** à gauche, **Principes** et **Raccourcis** à droite.

**Vue au lancement :** À plat, sauf si Rafale détecte une rafale de plus de 5 photos ; dans ce cas, vue Rafales. Les boutons permettent toujours de changer.

**Vue À plat.** Pour un dossier de moins de 300 photos, le bouton « À plat » remplace la liste des rafales par une grille : une vignette par photo, avec son marquage (étoiles ou croix de rejet en haut à gauche, couleur en haut à droite). Un cadre bleu entoure la photo affichée. Un clic sur une vignette affiche la photo. Le filtre et le marquage fonctionnent comme en vue Rafales. ↑ et ↓ montent ou descendent d'une rangée.

**Principes et Raccourcis** ouvrent une fenêtre d'aide dont on peut sélectionner et copier le texte. Tant qu'elle est ouverte, la fenêtre de Rafale ne réagit plus ; ferme l'aide pour reprendre.

## Clavier

| Touche | Action |
|---|---|
| → ou Page Suiv. | Photo suivante. Maintenue : défilé continu. |
| ← ou Page Préc. | Photo précédente. Maintenue : défilé continu en arrière. |
| ↓ | Début de la rafale suivante (vue À plat : rangée suivante) |
| ↑ | Début de la rafale précédente (vue À plat : rangée précédente) |
| Début / Fin | Première / dernière photo |
| Espace | Retenue (1 ★) ↔ non retenue |
| 1 à 5 | Nombre d'étoiles (rangée du haut, sans Maj, ou pavé numérique) |
| 0 | Efface les étoiles |
| X ou R | Rejetée ↔ non rejetée |
| F1 à F5 | Étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève) |
| Ctrl+C | Copie la sélection dans le presse-papiers (voir « Copier la sélection ») |
| F ou F11 | Plein écran |
| Échap ou Q | Quitter |

**Le défilé.** Un appui bref avance d'une seule photo. Si tu maintiens la touche plus de 0,3 s, le défilé continu démarre. Il ne saute jamais d'image : si le disque ou le décodage ne suivent pas, il ralentit au lieu de sauter des photos. Pendant le défilé, les photos sont affichées en aperçu réduit. Au relâchement, la pleine résolution arrive en environ un quart de seconde.

**Les touches ↑ et ↓** font un saut par appui. Les maintenir ne fait pas défiler les rafales.

## Souris

- **Loupe 100 %.** À l'arrêt, une fois la pleine résolution chargée, place le pointeur sur l'image : un carré de 600 px montre les pixels réels autour du pointeur, par exemple le visage de la cavalière. Le pointeur est masqué tant que la loupe est affichée. La loupe disparaît dès que le pointeur sort de l'image ou que le défilé reprend.
- **Loupe figée.** Un **clic sur la photo** fige la loupe à cet endroit : elle y reste quand tu passes d'une vue à l'autre avec ← et →, ce qui permet de comparer la netteté du même visage sur toute une rafale sans bouger la souris. Elle est alors bordée de bleu, et le panneau indique « Loupe figée ». Un autre clic la déplace ; un **clic droit** la libère. Comme la loupe normale, elle s'affiche à l'arrêt, une fois la pleine résolution chargée.
- **Commandes en haut du panneau** (marquage de la photo affichée) : clic sur une étoile = cette note (re-clic sur la même = aucune étoile) ; clic sur une couleur = cette étiquette (re-clic = aucune) ; case ✓ = retenue / non retenue, comme Espace ; case ✕ = rejetée / non rejetée, comme X.
- **Barre de filtre en bas de la liste** (mêmes commandes) : seules les photos qui passent le filtre sont montrées, dans la liste comme au défilé (←, →, ↑, ↓, Début, Fin). Une étoile = au moins cette note ; plusieurs couleurs = l'une ou l'autre ; ✓ et ✕ = retenues, rejetées, ou les deux si les deux sont cochées. Les critères se cumulent. « effacer » supprime le filtre. Si la photo affichée ne passe plus le filtre après un marquage, elle reste à l'écran jusqu'au prochain déplacement.
- **Clic sur une ligne de la liste :** affiche la première photo de cette rafale (la première qui passe le filtre). En vue À plat, un clic sur une vignette affiche cette photo.
- **Molette au-dessus du panneau :** fait défiler la liste. Elle se recentre dès que tu changes de rafale.
- **Ascenseur à droite de la liste :** tu peux le faire glisser. Un clic sur la piste y amène directement le curseur.

## Marquer les photos

Le marquage est enregistré dans un petit fichier à côté de chaque photo marquée : `PA290123.JPG.xmp`. Les JPEG ne sont jamais modifiés. Si tu enlèves tout le marquage d'une photo (aucune étoile, aucune couleur, pas de rejet), son fichier `.xmp` est supprimé.

**Retenue** veut dire au moins une étoile et pas rejetée. Espace pose une étoile ; les touches 1 à 5 règlent la note précisément.

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

1. Choisis en bas **JPEG** (par défaut), **RAW** ou **Les deux**.
2. **Ctrl+C**, ou clic sur **« Copier … »**. La sélection, ce sont les photos qui passent le filtre si un filtre est actif, sinon les photos retenues. Le bouton indique combien il y en a.
3. Dans **Fichiers**, ouvre le dossier de destination et fais **Ctrl+V** : les fichiers y sont copiés.

Le compte rendu s'affiche à côté du bouton, par exemple « 24 fichiers copiés, 2 sans RAW ». Les `.xmp` ne sont pas copiés. Le presse-papiers reste rempli même après la fermeture de Rafale, jusqu'à la prochaine copie. Il faut le paquet `wl-clipboard` (installé d'office sur Fedora).

## Rafales et réglages de l'appareil

- **Rafales :** elles sont reconnues grâce au numéro de vue que les boîtiers OM System et Olympus inscrivent dans chaque fichier (1, 2, 3…). Une nouvelle rafale commence quand ce numéro repart à 1. Pour les autres appareils, Rafale se sert de l'heure de prise de vue : plus d'une seconde d'écart entre deux photos, et c'est une nouvelle rafale.
- **Rythme (images/s) :** l'heure de prise de vue n'est enregistrée qu'à la seconde près, donc le rythme n'est exact que pour une rafale qui s'étale sur au moins trois secondes distinctes, soit environ 30 images à 25 i/s. Pour les rafales plus courtes, rien n'est affiché.
- **Mode image et filtre de bruit :** ils sont lus dans les données du boîtier (OM System / Olympus uniquement). Les noms de modes sont ceux d'exiftool, en anglais (« Natural », « i-Enhance »). Ils ne correspondent pas aux numéros du menu de l'appareil : par exemple, i-Enhance est le n° 1 du menu et Natural le n° 3.

## Conseils

- **Disque :** le défilé à froid (photos jamais ouvertes) est limité par le disque. Sur le SSD SATA en NTFS, on mesure environ 32 photos/s pour des JPEG de 15 Mo. Sur le NVMe, il y a de la marge. Pour une grosse séance, copier d'abord le dossier sur le NVMe rend le tri plus confortable.
- **Démarrage :** la liste des rafales se remplit en quelques fractions de seconde à quelques secondes, selon le disque. Pendant ce temps, le haut de la liste affiche « Analyse des rafales… ».
