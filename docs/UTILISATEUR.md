# Rafale — guide d'utilisation

Rafale est un visionneur fait pour trier des rafales de JPEG. Tu maintiens une flèche et les photos défilent en continu, sans en sauter aucune. Tu relâches, et la photo s'affiche en pleine résolution, avec une loupe 100 % pour vérifier la netteté. Les photos que tu retiens sont marquées dans des fichiers XMP, que les logiciels de retouche savent lire.

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

Exemple :

```
rafale-viewer ~/Images/2026-10-29
```

## L'écran

```
┌──────────────────────────────────────┬──────────────────────┐
│                                      │ ★★★  Retenue         │
│                                      │ Étiquette rouge      │
│                                      │ PA290123.JPG         │
│              photo                   │ 123 / 2355           │
│        (calée à gauche ;             │ Rafale 12  vue 5/54  │
│     portrait : centrée dans          │ 1/500 s  f/4  ISO …  │
│        la zone libre)                │ Natural · filtre …   │
│                                      │ 100 % : loupe …      │
│                                      ├──────────────────────┤
│                                      │ Rafale 11  20:45:07 ▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ Rafale 12  …  25 i/s▐│
│                                      │ [▫][▫][▫][▫]        ▐│
│                                      │ ━━━ (position)       │
└──────────────────────────────────────┴──────────────────────┘
```

Le panneau de droite occupe la place que laisse une photo 4:3. Il comprend deux parties.

**Les informations sur la photo affichée :**
- le marquage (retenue, rejetée ou non retenue) et l'étiquette de couleur ;
- le nom du fichier et sa position dans le dossier ;
- la rafale à laquelle elle appartient, sa place dans cette rafale et le rythme de la rafale ;
- l'exposition (vitesse, ouverture, ISO) ;
- le mode image et le filtre de bruit de l'appareil ;
- l'état de l'affichage : « aperçu réduit », puis « 100 % : loupe sous le pointeur » dès que la pleine résolution est chargée.

**La liste des rafales :** chaque ligne indique le numéro, l'heure, le nombre d'images, le rythme quand il est connu, et « ★ n » s'il y a des photos retenues. Elle montre 4 vignettes réparties sur la rafale. La rafale courante est surlignée, et un petit trait blanc sous ses vignettes montre où tu en es.

## Clavier

| Touche | Action |
|---|---|
| → ou Page Suiv. | Photo suivante. Maintenue : défilé continu. |
| ← ou Page Préc. | Photo précédente. Maintenue : défilé continu en arrière. |
| ↓ | Début de la rafale suivante |
| ↑ | Début de la rafale précédente |
| Début / Fin | Première / dernière photo |
| Espace | Retenue (1 ★) ↔ non retenue |
| 1 à 5 | Nombre d'étoiles (rangée du haut, sans Maj, ou pavé numérique) |
| 0 | Efface les étoiles |
| X ou R | Rejetée ↔ non rejetée |
| F1 à F5 | Étiquette rouge, jaune, verte, bleue, violette (un deuxième appui l'enlève) |
| F ou F11 | Plein écran |
| Échap ou Q | Quitter |

**Le défilé.** Un appui bref avance d'une seule photo. Si tu maintiens la touche plus de 0,3 s, le défilé continu démarre. Il ne saute jamais d'image : si le disque ou le décodage ne suivent pas, il ralentit au lieu de sauter des photos. Pendant le défilé, les photos sont affichées en aperçu réduit. Au relâchement, la pleine résolution arrive en environ un quart de seconde.

**Les touches ↑ et ↓** font un saut par appui. Les maintenir ne fait pas défiler les rafales.

## Souris

- **Loupe 100 %.** À l'arrêt, une fois la pleine résolution chargée, place le pointeur sur l'image : un carré de 600 px montre les pixels réels autour du pointeur, par exemple le visage de la cavalière. Le pointeur est masqué tant que la loupe est affichée. La loupe disparaît dès que le pointeur sort de l'image ou que le défilé reprend.
- **Clic sur une ligne de la liste :** affiche la première photo de cette rafale.
- **Molette au-dessus du panneau :** fait défiler la liste. Elle se recentre dès que tu changes de rafale.
- **Ascenseur à droite de la liste :** tu peux le faire glisser. Un clic sur la piste y amène directement le curseur.

## Marquer les photos

Le marquage est enregistré dans un petit fichier à côté de chaque photo marquée : `PA290123.JPG.xmp`. Les JPEG ne sont jamais modifiés. Si tu enlèves tout le marquage d'une photo (aucune étoile, aucune couleur, pas de rejet), son fichier `.xmp` est supprimé.

**Retenue** veut dire au moins une étoile et pas rejetée. Espace pose une étoile ; les touches 1 à 5 règlent la note précisément.

### Les RAW

Si la photo a un RAW de même nom (`PA290123.ORF`, `.DNG`, `.CR3`, `.NEF`…), dans le même dossier ou dans un sous-dossier direct comme `raw/`, le même marquage est écrit à côté du RAW : `raw/PA290123.ORF.xmp`. Le panneau affiche « RAW : raw/PA290123.ORF », ou « pas de RAW ».

- **Rattrapage :** au lancement, les photos déjà marquées dont le RAW n'a pas encore reçu le marquage sont mises à jour.
- **Ordre conseillé :** fais le tri **avant** d'importer les RAW dans darktable. Ensuite, darktable crée son propre `.xmp` pour chaque RAW, Rafale ne le modifie plus, et le panneau indique « XMP externe, non synchronisé ».

### Dans les logiciels de retouche

| Logiciel | Ce qu'il lit | À savoir |
|---|---|---|
| darktable | étoiles, rejet, couleur | Importer avec **« ajouter à la bibliothèque »**. « Copier et importer » ignore les `.xmp`. Après l'import, c'est la base de données de darktable qui fait foi. |
| digiKam | étoiles, couleur, retenue / rejetée | Activer la lecture des sidecars : *Configuration › Métadonnées › Sidecars*. |
| RawTherapee (5.11 et plus) | étoiles, couleur (pas le rejet) | Activer la synchronisation des notes et couleurs avec les XMP (Préférences › Navigateur de fichiers). Mettre `XMPSidecarStyle=ext` dans la section `[Metadata]` du fichier `~/.config/RawTherapee/options`, pour qu'il lise `PA290123.ORF.xmp` au lieu de `PA290123.xmp`. Une photo rejetée y apparaît sans étoile. |
| Lightroom | étoiles, rejet, couleur | Conventions Lightroom respectées (`xmp:Rating`, `xmp:Label`). |

**Le fichier XMP d'un autre logiciel n'est jamais modifié.** C'est le cas par exemple après un import dans darktable, qui y range l'historique de tes retouches. Rafale le lit mais n'y touche pas, et le panneau indique « XMP externe, lecture seule ». Les touches de marquage sont alors sans effet sur cette photo.

## Rafales et réglages de l'appareil

- **Rafales :** elles sont reconnues grâce au numéro de vue que les boîtiers OM System et Olympus inscrivent dans chaque fichier (1, 2, 3…). Une nouvelle rafale commence quand ce numéro repart à 1. Pour les autres appareils, Rafale se sert de l'heure de prise de vue : plus d'une seconde d'écart entre deux photos, et c'est une nouvelle rafale.
- **Rythme (images/s) :** l'heure de prise de vue n'est enregistrée qu'à la seconde près, donc le rythme n'est exact que pour une rafale qui s'étale sur au moins trois secondes distinctes, soit environ 30 images à 25 i/s. Pour les rafales plus courtes, rien n'est affiché.
- **Mode image et filtre de bruit :** ils sont lus dans les données du boîtier (OM System / Olympus uniquement). Les noms de modes sont ceux d'exiftool, en anglais (« Natural », « i-Enhance »). Ils ne correspondent pas aux numéros du menu de l'appareil : par exemple, i-Enhance est le n° 1 du menu et Natural le n° 3.

## Conseils

- **Disque :** le défilé à froid (photos jamais ouvertes) est limité par le disque. Sur le SSD SATA en NTFS, on mesure environ 32 photos/s pour des JPEG de 15 Mo. Sur le NVMe, il y a de la marge. Pour une grosse séance, copier d'abord le dossier sur le NVMe rend le tri plus confortable.
- **Démarrage :** la liste des rafales se remplit en quelques fractions de seconde à quelques secondes, selon le disque. Pendant ce temps, le haut de la liste affiche « Analyse des rafales… ».
