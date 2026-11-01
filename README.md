# Rafale

Visionneur de photos pour Linux (GNOME / Wayland), écrit en Rust, fait pour **trier vite de longues rafales de JPEG** : 25 images/s ou plus, des milliers de photos par séance.

- **Défilé sans saut.** Touche maintenue, les photos défilent en continu, sans en sauter aucune ; si le disque ne suit pas, le défilé ralentit. Au relâchement, la pleine résolution arrive en un quart de seconde.
- **Loupe 100 %** sous le pointeur, pour vérifier la netteté.
- **Rafales détectées** automatiquement (numéro de vue OM System / Olympus, sinon heure de prise de vue), avec vignettes et rythme ; **vue à plat** pour les petits dossiers.
- **Marquage interopérable** : notes, couleurs, retenue / rejet dans un `.xmp` à côté du JPEG et de son RAW, lu par darktable, digiKam, RawTherapee et Lightroom. Les photos ne sont jamais modifiées.
- **Filtre** par note, couleur, retenue ou rejet, et **copie de la sélection** (JPEG, RAW ou les deux) vers le presse-papiers, à coller dans Fichiers.

Décodage turbojpeg (réduit 1/4 pendant le défilé), 11 décodeurs en parallèle, préchargement ±10 images, affichage wgpu.

## Installation

```
sudo dnf install cargo cmake nasm wl-clipboard zenity
cargo build --release
./target/release/rafale-viewer <dossier>
```

## Documentation

- [Guide d'utilisation](docs/UTILISATEUR.md)
- [Document de conception](docs/CONCEPTION.md)

## Licence

MIT, voir [LICENSE](LICENSE).
