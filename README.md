# Rafale

Visionneur de photos pour Linux (GNOME / Wayland), écrit en Rust, fait pour **trier vite de longues rafales de JPEG** : 25 images/s ou plus, des milliers de photos par séance.

- **Défilé sans saut.** Touche maintenue, les photos défilent en continu, sans en sauter aucune ; si le disque ne suit pas, le défilé ralentit. Au relâchement, la pleine résolution arrive en un quart de seconde.
- **Loupe 100 %** sous le pointeur, pour vérifier la netteté.
- **Rafales détectées** automatiquement (numéro de vue OM System / Olympus, sinon heure de prise de vue), avec vignettes et rythme ; **vue à plat** pour les petits dossiers.
- **Marquage interopérable** : notes, couleurs, retenue / rejet dans un `.xmp` à côté du JPEG et de son RAW, lu par darktable, digiKam, RawTherapee et Lightroom. Les photos ne sont jamais modifiées.
- **Filtre** par note, couleur, retenue ou rejet, et **copie de la sélection** (JPEG, RAW ou les deux) vers le presse-papiers, à coller dans Fichiers.

Décodage turbojpeg (réduit 1/4 pendant le défilé), 11 décodeurs en parallèle, préchargement ±10 images, affichage wgpu.

## À qui ça sert, et à qui non

Rafale est un **projet personnel**, écrit pour un besoin précis : trier des rafales d'**OM System OM-1 Mk II** sous **Fedora, GNOME / Wayland**. Il est publié tel quel, au cas où il serve à d'autres, sans engagement de support.

- **Boîtiers OM System / Olympus : visés.** La détection des rafales s'appuie sur le numéro de vue de la MakerNote OM ; le mode image et le filtre de bruit en viennent aussi, et les fonctions à venir (point AF…) en dépendront.
- **Autres marques (Sony, Canon, Nikon, Fujifilm…) : à adapter de ton côté.** Le défilé, la loupe, le marquage XMP et le filtre fonctionnent avec n'importe quel JPEG. Mais les rafales ne sont détectées que d'après l'heure de prise de vue (à la seconde, donc approximativement), et les réglages propres au boîtier ne sont pas affichés.
- **GNOME : visé. KDE et autres bureaux : à adapter.** La copie vers le gestionnaire de fichiers utilise le format de presse-papiers de Fichiers (Nautilus) via `wl-copy` ; Dolphin attend un autre format. L'aide s'ouvre avec `zenity`, la police est trouvée par `fc-match`.

## Un projet « vibe codé »

Rafale a été écrit entièrement par une IA (Claude Code), sous la direction de son auteur, photographe et non relecteur de chaque ligne : il est testé à l'usage, sur de vraies séances, pas par une revue de code. Les choix et les mesures qui les justifient sont consignés dans le [document de conception](docs/CONCEPTION.md).

**La façon conseillée de le modifier est la même** : forke le dépôt et demande les changements à ton agent de code, en lui donnant [docs/CONCEPTION.md](docs/CONCEPTION.md) comme point de départ. Sa section « Adapter Rafale » liste les endroits qui dépendent du boîtier et du bureau.

## Installation

```
sudo dnf install cargo cmake nasm wl-clipboard zenity
cargo build --release
./packaging/install.sh
```

Le script installe Rafale pour l'utilisateur courant (`~/.local`) : entrée « Rafale » dans le menu des applications, et *Ouvrir avec… › Rafale* au clic droit sur un dossier ou une photo dans Fichiers. En ligne de commande : `rafale-viewer <dossier>`.

## Documentation

- [Guide d'utilisation](docs/UTILISATEUR.md)
- [Document de conception](docs/CONCEPTION.md)
- [Étude des évolutions de la v2](docs/ETUDE-V2.md)

## Licence

MIT, voir [LICENSE](LICENSE).
