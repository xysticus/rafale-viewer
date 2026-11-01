# Rafale — document de conception

Ce document s'adresse à qui reprend le code. Il décrit le but de chaque élément, les choix faits et les mesures qui les justifient. Le besoin d'origine est dans [BRIEF.md](../BRIEF.md), et la façon de se servir du programme dans [UTILISATEUR.md](UTILISATEUR.md).

## 1. Objectif et contraintes

Le programme sert à trier des rafales de JPEG OM System OM-1 Mk II : 20 Mpx (5184 × 3888), 4:2:2, environ 15 Mo par fichier, 25 i/s, des milliers d'images par séance. Trois exigences :

1. **Défilé continu sans saut.** Touche maintenue : on voit *toutes* les images, même celles jamais lues depuis le disque.
2. **Pleine résolution instantanée au relâchement,** avec une loupe 100 % pour juger la netteté.
3. **Rien à l'écart des photos :** pas de base de données, pas de cache de vignettes sur disque. Le marquage est interopérable (XMP).

Environnement cible : Fedora 44, GNOME/Wayland, Ryzen AM5 12 threads, RX 9060 XT (Vulkan/RADV), clavier AZERTY.

## 2. Vue d'ensemble

```
             ┌───────────── thread principal (winit) ─────────────┐
 clavier ──▶ │ App : état du défilé, sélection, marquage, panneau │──▶ Renderer (wgpu)
 souris      └──┬───────────────────────▲──────────────▲──────────┘
                │ set_wanted(jobs)      │ Done(image)  │ (index, Meta)
                ▼                       │              │
        ┌──────────────┐  octets  ┌───────────┐   ┌──────────┐
        │ reader (×1)  │ ───────▶ │ decode-N  │   │ scan (×1)│
        │ lecture disque│  cache   │ (×11)     │   │ en-têtes │
        └──────────────┘  brut    └───────────┘   └──────────┘
```

Les threads réveillent la boucle winit par `EventLoopProxy::send_event(())`. Les résultats passent par des canaux crossbeam, vidés dans `about_to_wait`.

| Module | Rôle |
|---|---|
| `main.rs` | `App` : boucle d'événements, défilé, préchargement, rafales, marquage, panneau, loupe |
| `loader.rs` | thread de lecture + pool de décodage turbojpeg, file de priorité remplaçable |
| `render.rs` | wgpu : quads texturés (image, loupe, panneau, liste, repères) |
| `shader.wgsl` | un seul shader : quad, rectangle source, orientation EXIF |
| `meta.rs` | analyse des en-têtes EXIF et de la MakerNote en arrière-plan |
| `panel.rs` | rendu CPU du panneau (texte fontdue, vignettes) |
| `marks.rs` | lecture et écriture des XMP |
| `raw.rs` | association de chaque JPEG à son RAW |
| `help.rs` | textes des fenêtres d'aide (principes, raccourcis) |
| `examples/bench.rs` | banc de mesure lecture disque / décodage |

## 3. Défilé

### 3.1 Appui et relâchement, pas la répétition clavier

`KeyEvent::repeat` est ignoré. Seuls l'appui et le relâchement comptent, et c'est le programme qui cadence le défilé. C'est précisément le défaut de Geeqie : il relance un chargement à chaque répétition de touche, donc il abandonne le décodage en cours et n'affiche rien avant le relâchement.

État dans `App` :

- `pending` : pas demandés par des appuis brefs mais pas encore faits. Un appui fait toujours avancer d'une image, même si elle n'est pas encore décodée.
- `scrolling` (−1, 0, +1) et `next_step_at` : défilé continu, qui démarre `HOLD_DELAY` (300 ms) après l'appui et avance ensuite toutes les `1/fps` s.
- Perte de focus : on arrête tout, puisque le relâchement ne nous parviendra pas.

### 3.2 Jamais de saut

`App::step` n'avance que si l'image suivante est **prête**, en réduit ou en plein. Sinon, il attend le réveil du décodeur. Après une attente, la cadence repart de *maintenant*, sans rattrapage en rafale (`base = now` si le retard dépasse un intervalle).

### 3.3 Réduit pendant le défilé, plein à l'arrêt

- Pendant le défilé, on décode au **1/4** (1296 × 972), avec la réduction DCT de turbojpeg.
- On considère l'image comme à l'arrêt (`stationary`) quand il n'y a ni défilé ni pas en attente, et qu'elle n'a pas changé depuis `FULL_DELAY` (100 ms). On demande alors la pleine résolution de l'image courante, puis celle de ses deux voisines.
- L'affichage prend toujours la meilleure version disponible (plein, sinon réduit), sans jamais revenir en arrière.

### 3.4 File de priorité remplaçable (annulation)

À chaque tour, `App::wanted` construit la liste **complète** des tâches voulues, par priorité :

1. réduit de l'image courante ;
2. plein de l'image courante (si à l'arrêt) ;
3. réduits de +1 à +10 dans le sens du déplacement, puis de −1 à −10 ;
4. pleins des deux voisines (si à l'arrêt).

`Loader::set_wanted` **remplace** la file : ce qui n'y figure plus est abandonné. Une tâche déjà commencée va jusqu'au bout. Le résultat est envoyé *avant* le retrait de `in_flight`, si bien que le thread principal ne redemande jamais une image décodée mais pas encore reçue.

Caches LRU côté `App` : 64 réduits (~5 Mo chacun) et 6 pleins (~80 Mo chacun), soit environ 800 Mo au pire.

## 4. Lecture disque et décodage : mesures

Mesures faites avec `examples/bench.rs` sur la séance de test (fichiers de 15 Mo, SSD SATA 860 QVO en NTFS via ntfs-3g).

| Mesure | Résultat |
|---|---|
| Décodage 1/8, un thread | 77-82 ms |
| Décodage 1/4 | 92-98 ms |
| Décodage 1/1 | 132-139 ms |
| Lecture séquentielle, un thread | 27-30 ms/fichier, ~485 Mo/s |
| Lecture + décodage 1/4, 11 threads qui lisent chacun | **17-18 images/s** |
| 1 thread de lecture + 11 décodeurs | **32 images/s** (limite du disque) |

Ce qu'on en tire :

- **Le décodage Huffman domine.** Les fichiers sont peu compressés (qualité maximale), et la réduction DCT n'allège que la transformée inverse : 1/8 coûte presque autant que 1/4. En parallèle, le CPU reste largement suffisant (11 threads, environ 85 réduits/s).
- **Les lectures concurrentes divisent le débit par deux** sur ntfs-3g. D'où **un seul thread de lecture** (`reader`), qui lit dans l'ordre de priorité et alimente les décodeurs. Les octets bruts restent dans un LRU de 40 fichiers (~600 Mo), pour que la pleine résolution ne relise pas le disque.
- **À froid sur ce disque, 25 i/s tiennent, avec peu de marge.** Sur le NVMe, la marge est confortable.

Choix de turbojpeg plutôt que zune-jpeg : la réduction DCT native (1/2, 1/4, 1/8). Fedora sépare l'API TurboJPEG (`turbojpeg-devel`) de `libjpeg-turbo-devel`. Faute de pkg-config, le crate `turbojpeg-sys` compile libjpeg-turbo en statique, ce qui demande cmake et nasm.

## 5. Rendu

### 5.1 Un seul pipeline, des quads

Chaque élément affiché est un `Quad` : une texture, un sampler et un tampon uniforme de 48 octets.

```
dst : vec4  rectangle écran en coordonnées normalisées (g, h, d, b)
src : vec4  rectangle source dans l'image *orientée*, en [0, 1]
orientation : u32  EXIF 1 à 8
```

- Le vertex shader produit un triangle strip de 4 sommets et applique la correspondance d'orientation EXIF sur les coordonnées de texture. **Les pixels ne sont jamais tournés sur le CPU.**
- Le blending alpha est activé pour le texte. Les photos, opaques, n'en sont pas affectées.
- Ordre de dessin : image, panneau d'informations, liste, repère de position, ascenseur, puis bordure et loupe par-dessus.

### 5.2 Disposition

- **Image :** ajustée à la zone libre `[IMAGE_MARGIN, largeur − largeur du panneau]` sans dépasser 100 %, puis centrée dans cette zone. Un 4:3 paysage la remplit, un portrait est au milieu.
- **Panneau :** largeur = fenêtre − marge − hauteur × 4/3 (minimum 280 px), soit 616 px sur un écran 2560 × 1440. Sa position ne dépend pas de l'orientation de la photo, pour que l'œil retrouve toujours les informations au même endroit.

### 5.3 Loupe 100 %

`Renderer::loupe_rects` : carré de 600 px centré sur le pointeur, avec un rectangle source de la même taille en pixels pleine résolution. Les deux origines sont **arrondies au pixel** et le sampler est en `Nearest` : chaque pixel de l'écran tombe exactement sur un pixel source, sans aucun flou d'interpolation. La source est ramenée à l'intérieur de l'image près des bords. Conditions d'affichage (`loupe_allowed`) : arrêt, pleine résolution affichée, pointeur sur l'image.

### 5.4 Pièges Wayland rencontrés

- **`set_title` à chaque tour de boucle :** les décorations dessinées côté client par winit sont redessinées à chaque changement de titre, ce qui génère des événements, donc un nouveau tour de boucle. Mesure : 51 000 `set_title` en 1,7 s, puis saturation de la connexion Wayland en plein écran et `ExitFailure(1)`. Règle : **ne jamais appeler `set_title`, `request_redraw`, etc. sans vérifier que quelque chose a changé.** Au repos, on mesure environ 1,5 commit Wayland par seconde.
- **wgpu 30 :** le handle d'affichage est requis dans `InstanceDescriptor` (`owned_display_handle`) et la présentation passe par `queue.present(frame)`.

## 6. Panneau (CPU)

Le panneau est rastérisé sur le CPU (`fontdue`, police trouvée par `fc-match sans-serif`), puis envoyé comme texture. Pour qu'il ne coûte rien pendant le défilé :

- **Informations :** recomposées seulement si leur texte change, une fois par image et en moins d'une milliseconde. La clé de comparaison est le texte plus la largeur.
- **Liste des rafales :** recomposée seulement si la rafale courante, la fenêtre de lignes, les comptes de photos retenues ou la largeur changent. Cela prend quelques millisecondes et environ 2,7 Mo de texture.
- **Vue à plat** (dossiers de moins de 300 photos, `FLAT_MAX`) : la liste devient une grille de rangées de 4 vignettes (`Entry::Photos`), une par photo visible, avec son marquage dessiné dessus (étoiles ou rejet, pastille de couleur). Elle est la vue de départ ; l'analyse des en-têtes bascule en vue rafales dès qu'elle trouve une rafale de plus de 5 photos (`FLAT_BURST_MAX`), sauf si l'utilisateur a déjà choisi sa vue au bouton. Elle n'est recomposée qu'au changement de rangée courante ou de marquage : la photo courante est soulignée par le même quad GPU que le repère des rafales. Le rendu renvoie pour cela une liste de `Track` (photos `start..start+len` sous lesquelles placer le repère) : une par rafale, une par vignette.
- **Repère de position et ascenseur :** ce sont des quads GPU séparés (textures 1 × 1), qui bougent à chaque image sans recomposer la liste.
- **Vignettes :** ce sont les miniatures EXIF intégrées (160 × 120, ~9 Ko). Elles sont gardées compressées dans `Meta`, puis décodées, orientées et redimensionnées (bilinéaire) à la demande, dans un LRU de 512 entrées.

## 7. Analyse des en-têtes et rafales

Le thread `scan` lit les **64 premiers Ko** de chaque fichier (512 Ko en cas d'échec), ce qui suffit pour le segment APP1 des OM System. Il envoie au fil de l'eau un `(index, Meta)` dans l'ordre des fichiers, et réveille la boucle tous les 64 fichiers. Il faut 48 ms pour 2355 fichiers en cache, plus à froid.

### 7.1 Découpage en rafales

L'horodatage EXIF (`DateTimeOriginal`) n'a **pas de sous-secondes** sur ce boîtier, donc un écart d'1 s est ambigu. Avec un seuil à 2 s, on obtenait 77 rafales, contre 115 réelles : des rafales rapprochées étaient fusionnées.

Source exacte retenue : **MakerNote OM System, CameraSettings (0x2020), DriveMode (0x0600)** = `[mode, numéro de vue, bits de mode, …]`. Mode 0 = vue unique. Règle (`meta::same_burst`) : deux images consécutives sont dans la même rafale si leurs deux modes sont non nuls et que le numéro de la seconde vaut celui de la première + 1. Sans MakerNote, on se rabat sur un écart d'horodatage de 0 à 1 s.

La MakerNote est un IFD little-endian dont les décalages sont relatifs à son propre début. L'en-tête fait 16 octets pour `OM SYSTEM\0` et 12 pour `OLYMPUS\0II`. Le parseur (`olympus_settings`) vérifie les bornes à chaque lecture.

Les rafales sont construites de façon incrémentale (`App::add_meta`) : `bursts: Vec<Burst>` et `burst_of: Vec<usize>` pour la partie déjà analysée.

### 7.2 Rythme

Aucune balise documentée n'enregistre la cadence (vérifié dans la table exiftool Olympus). Le rythme est donc mesuré : on compte les images entre le **premier et le dernier changement de seconde**, puis on divise par l'écart en secondes. Le résultat est exact, mais exige au moins deux changements de seconde. Il est recalculé pour la dernière rafale à chaque en-tête reçu. Sur la séance de test : 14 rafales mesurables, toutes à 25,0 i/s.

### 7.3 Réglages affichés

- Exposition : `ExposureTime`, `FNumber` et `PhotographicSensitivity` (EXIF standard).
- `PictureMode` (0x0520) et `NoiseFilter` (0x0527) dans CameraSettings, décodés d'après exiftool. NoiseFilter vaut `[valeur, min, max]` : `-2,-2,1` = coupé, `-1` = faible, `0` = standard, `1` = fort. Sur la séance de test, ces valeurs correspondent aux souvenirs du photographe.

## 8. Marquage XMP

Le fichier `PHOTO.JPG.xmp` est placé à côté de la photo. C'est le nommage de darktable, et digiKam sait le lire. Les originaux ne sont jamais modifiés.

| Donnée | Champs écrits |
|---|---|
| Note | `xmp:Rating` 0-5, **−1 = rejetée** |
| Couleur | `xmp:Label` = `Red`/`Yellow`/`Green`/`Blue`/`Purple` ; `digiKam:ColorLabel` = 1/3/4/5/6 |
| Retenue / rejet | `digiKam:PickLabel` : 3 = acceptée, 1 = rejetée, 0 = rien |
| Signature | `xmp:CreatorTool="rafale-viewer"` |

Règles :

- **Retenue ⇔ au moins une étoile et pas rejetée.** Espace bascule entre 0 et 1 étoile.
- Un marquage vide **supprime** notre XMP. L'écriture est atomique (fichier `.tmp`, puis renommage).
- **Un XMP sans notre signature est en lecture seule** (`Mark::foreign`) s'il contient quelque chose. C'est le cas, par exemple, du sidecar de darktable, qui contient l'historique des retouches.
- **Exception : un XMP étranger vierge** (`blank`) est remplacé dès qu'on marque la photo, jamais supprimé. Vierge veut dire : pas de note, de couleur, d'élément ou de rejet ; un historique darktable vide (`history_end="0"`, pas de `rdf:li`) ; uniquement les mots-clés automatiques `darktable|…` ; ni `crs:` (Lightroom), ni `digiKam:ImageHistory`, ni titre, description ou droits. Tout ce qui n'est pas reconnu compte comme du contenu. Raison : darktable pose un tel fichier pour chaque photo à l'import, ce qui bloquait le tri de toute la séance.
- Lecture tolérante : attributs (`nom="v"`) ou éléments (`<nom>v</nom>`), `xmp:Label` prioritaire sur `digiKam:ColorLabel`.
- darktable donne priorité à `xmp:Label` sur `darktable:colorlabels` (vérifié dans `exif.cc`), qu'il n'est donc pas nécessaire d'écrire.

Les fichiers produits ont été validés avec `exiv2 -px`, qui lit les 5 champs. Les tests unitaires de `marks.rs` couvrent l'aller-retour, la suppression, la protection d'un XMP étranger et le remplacement d'un XMP étranger vierge. Non testé : l'import réel dans darktable et digiKam.

Abandonné : la copie des photos retenues dans `retenus/`, remplacée par le XMP. Les anciens dossiers `retenus/` ne sont plus lus.

### 8.1 Report sur les RAW

`raw::find` cherche, pour chaque JPEG, un fichier de **même nom** avec une extension RAW. Il regarde d'abord dans le dossier des JPEG, puis dans ses sous-dossiers directs, ce qui fait une lecture de dossier chacun. Quand un même nom existe aux deux endroits, le dossier des JPEG l'emporte.

- Chaque marquage est écrit dans `PHOTO.JPG.xmp` **et** dans `RAW.EXT.xmp`, avec un contenu identique. Le JPEG fait foi.
- Au démarrage, `sync_raws` recopie les marquages des JPEG vers les RAW (rattrapage), et supprime un XMP de RAW à nous dont le JPEG n'est plus marqué. Un JPEG marqué par un autre logiciel n'est pas recopié.
- La protection est vérifiée **sur le disque au moment d'écrire** (`save_to` → `is_foreign`), pas d'après un état mémorisé au chargement. Un XMP créé entre-temps par darktable n'est donc pas écrasé.
- **Nommage :** on garde le nom complet, `RAW.ORF.xmp`, partout. Le nom de base (`RAW.xmp`, convention Lightroom, ACDSee et RawTherapee par défaut) est ambigu quand JPEG et RAW partagent un dossier. Il entrerait aussi en conflit avec les `.xmp` d'ACDSee des anciennes séances.
- **RawTherapee** (lu dans son code source, branche dev) : il lit `xmp:Rating` (borné à 0-5, donc sans rejet) et `xmp:Label` (`Red`… `Purple`). Il faut activer `ThumbnailRankColorMode=xmp`, et `XMPSidecarStyle=ext` pour le nom complet. Son rejet (« corbeille ») n'existe que dans le `.pp3`.

Sur la séance de test : 2353 RAW trouvés dans `raw/`, pour 2355 JPEG. Les 2 vues isolées ont été prises en JPEG seul.

## 9. Interactions et clavier

- **Commandes cliquables** (`panel.rs`) : étoiles, pastilles de couleur, cases ✓ et ✕, dessinées en vectoriel sur le canevas CPU (anticrénelage 4 × 4, composition alpha) plutôt qu'avec des glyphes : Noto Sans n'a pas « ★ ». Le rendu renvoie leurs zones (`Hit`), testées au clic : marquage relativement au panneau, filtre relativement à la liste.
- **Filtre** (`marks::Filter`) : note minimale, couleurs (ou), retenue / rejetée (ou), critères cumulés (et). `App::visible` (indices triés) est recalculé à chaque changement de filtre ou de marquage (O(n)). `App::neighbor(from, dir, k)` (recherche dichotomique) sert au défilé, au préchargement, à ↑/↓, Début/Fin ; la liste ne montre que les rafales ayant des images visibles, avec leurs vignettes choisies parmi celles-ci.

- **Lettres** (`f`, `q`, `x`, `r`) : lues par touche **logique**, pour fonctionner en AZERTY.
- **Flèches, chiffres, F1-F5, F11, Espace** : lus par position **physique**. La rangée des chiffres marche donc sans Maj en AZERTY.
- **Clic dans la liste :** test sur les zones `list_hits` (`Widget::Photo`) renvoyées par le rendu de la liste, relatives à `list_origin()` : ligne entière en vue rafales, vignette en vue à plat. Les commandes du bas (`bar_hits`) regroupent filtre, vue et aide.
- **Copie de la sélection** (Ctrl+C, bouton) : sélection = `visible` si un filtre est actif, sinon les retenues ; JPEG, RAW ou les deux (`CopyKind`). Les chemins sont passés à `wl-copy --type x-special/gnome-copied-files` (« copy » puis une URI `file://` encodée par ligne), le format que Nautilus lit au collage. wl-copy reste en arrière-plan pour servir le contenu, donc le presse-papiers survit à Rafale. Abandon de l'ancienne copie dans `retenus/` : l'utilisateur choisit la destination dans Fichiers.
- **↑ / ↓** : rafale précédente / suivante, ou rangée de 4 photos visibles en vue à plat.
- **Aide :** « Principes » et « Raccourcis » ouvrent `zenity --text-info` (texte sélectionnable, envoyé sur l'entrée standard). Tant que le processus vit (`try_wait`), la fenêtre principale ignore clavier, clics et molette : modalité simulée, Wayland ne permettant pas de rattacher la fenêtre zenity à la nôtre.
- **Ascenseur :** l'appui (avec une tolérance de 8 px de chaque côté) démarre un glissement. La position du curseur est convertie en décalage de lignes (`list_offset`), et ce décalage est remis à zéro quand la ligne courante (rafale ou rangée) change.

## 10. Limites connues et pistes

- **Pas de mipmaps :** une image pleine résolution réduite à l'écran (≈ 0,37×) est échantillonnée en bilinéaire, ce qui peut produire un léger crénelage. La loupe n'est pas concernée.
- **Image affichée en 8 bits sRGB**, sans gestion des profils ICC ni du HDR.
- **Le rythme d'une rafale courte** (moins de 3 secondes distinctes) reste inconnu.
- **Pas de décodage parallèle d'une même image.** Les JPEG OM ont des marqueurs de redémarrage tous les 4 MCU, ce qui permettrait de répartir le décodage Huffman d'une image sur plusieurs cœurs. C'est la piste pour réduire la latence d'une image jamais lue (aujourd'hui ~100-140 ms de décodage).
- **Pistes ouvertes :** loupe redimensionnable à la molette, défilé limité aux photos retenues, intégration au menu GNOME.
