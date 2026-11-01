# Rafale — étude des évolutions de la v2

Liste des pistes proposées pour la version 2, avec leur état. Elle sert de feuille de route : on choisit la suivante ici, et on met l'état à jour à chaque étape.

Principe commun à toutes les pistes : **ne jamais ralentir le défilé.** Tout calcul se fait à l'arrêt ou en tâche de fond, et s'interrompt dès que le défilé reprend.

## Fait

| # | Piste | Ce qui a été fait |
|---|---|---|
| 0 | Cadrage public | Le README, l'aide (« À propos ») et la conception précisent que le projet est « vibe codé », qu'il cible OM System et GNOME, et ce qu'il faut adapter ailleurs (conception, section 11). |
| 1 | Loupe figée | Un clic sur la photo fige la loupe à cet endroit pour toute la rafale ; clic droit, ou changement de rafale, pour la libérer. |
| 2 | Zone AF | Cadre vert sur la zone de mise au point enregistrée par le boîtier (MakerNote OM, balise 0x030a). Elle suit le sujet vue par vue, le plus souvent le visage détecté. Touche C : afficher ou masquer le cadre. Touche A : loupe AF, qui suit la zone vue après vue. |
| 3 | Netteté AF | Indice de netteté (variance du laplacien) sur la zone AF, mesuré à l'arrêt pour toute la rafale (~1 s pour 15 vues). Le panneau affiche le pourcentage de la plus nette et le rang. Touche N : aller à la plus nette. |
| 4 | Corbeille des rejetées | Bouton rouge « Corbeille : N rejetées » : après confirmation (nombre de fichiers, place libérée), `gio trash` sur le JPEG, le RAW et tous leurs fichiers d'accompagnement (XMP de Rafale ou d'autres logiciels, PP3) des photos marquées de la croix rouge, et seulement celles-là. Rafale relit ensuite le dossier sans se fermer. |
| 6 | Intégration à GNOME | `packaging/install.sh` installe dans `~/.local` le programme, une icône et un `.desktop` : entrée « Rafale » dans le menu, *Ouvrir avec… › Rafale* sur un dossier ou une photo. Sans argument, Rafale demande le dossier ; sur une photo, il ouvre son dossier en commençant par elle ; hors terminal, les erreurs s'affichent dans une fenêtre. |

## À faire

### 5. « Ouvrir les retenues dans… »

Décidé : option B. Un bouton « Retoucher… » copie la sélection (retenues, ou filtrées), selon le choix JPEG / RAW / Les deux, avec leurs `.xmp`, dans un dossier voisin de la séance (`~/Images/2026-10-29-retouche/`, pas un sous-dossier : Rafale y chercherait des RAW), puis lance le logiciel de retouche sur ce dossier. Le logiciel ne voit que les photos choisies et écrit ses fichiers dans sa copie ; la séance triée et les originaux restent intacts. Choix du logiciel au premier usage, parmi ceux installés qui ouvrent un dossier ou un RAW, puis retenu.

### 7. Reprise à la dernière photo vue

En rouvrant un dossier, revenir à la photo où on s'était arrêté, pour un tri fait en plusieurs fois. À trancher : où garder cette position sans créer de fichier dans le dossier des photos (par exemple `~/.local/state/rafale/`).

### 8. Import de la carte SD

Copier la carte vers `~/Images/AAAA-MM-JJ`, en rangeant les RAW dans `raw/`, puis ouvrir le dossier. Utile seulement si ce rangement se fait aujourd'hui à la main.

## Essai : interface GTK4 / libadwaita

Question : une interface en vrais widgets GNOME ralentirait-elle le défilé ? Essai sur la branche `essai-gtk4` : programme `rafale-gtk` avec le même chargeur, le même décodage et la même logique de pas, mais la photo dans un `GtkPicture` et le panneau en widgets Adwaita (sans loupe, cadres, orientation ni liste de vignettes). Mesures avec `--bench-scroll 5`, séance du 29/10, écran 144 Hz :

| Cadence visée | winit + wgpu (actuel) | GTK4 + libadwaita |
|---|---|---|
| 25 i/s | 25,0 ; intervalles 40,0 ms, 0 à-coup | 25,0 ; intervalles 40,8 ms (p95 42, max 45), 0 à-coup |
| 50 i/s | 50,0 ; intervalles 20,0 ms, 0 à-coup | 50,0 ; intervalles 24 ms (p95 27, max 36), 3 à 5 à-coups sur 220 |
| 60 i/s | 60,0 ; intervalles 16,7 ms, 0 à-coup | 60,0 en moyenne, mais 86 à 102 à-coups sur 264 (p95 29 ms) |

Pleine résolution, processeur et mémoire : équivalents. Conclusion provisoire : à 25 i/s, GTK ne ralentit pas ; il a moins de marge à cadence élevée. L'aspect GNOME plaît. À trancher après un essai sur le M7 Ultra (processeur bridé), avec les mêmes commandes sur la branche `essai-gtk4`.

Effet de bord de l'essai : les « flashs » au début de la séance venaient d'exports (digiKam `_v1`, darktable `-1`) intercalés dans le défilé et d'une photo écrasée par un recadrage de gThumb (PA290088, restaurée depuis le SSD) ; rangés dans `exports/`.

## Abandonné

- **Sélection multiple de vignettes** (Ctrl+clic, Maj+clic, Ctrl+A), pour la copie puis pour marquer plusieurs photos d'un coup. Elle a été réalisée, essayée, puis retirée : pas assez pratique pour la complexité qu'elle ajoute. La copie passe par le filtre ou les retenues.

## Pistes techniques en réserve

- **Décodage partiel de la zone AF** (`tj3SetCroppingRegion`) : il rendrait la mesure de netteté plus rapide, si elle se révèle lente sur de longues rafales ou un disque lent.
- **Décodage parallèle d'une même image** grâce aux marqueurs de redémarrage des JPEG OM : il réduirait la latence d'une photo jamais lue (aujourd'hui 100 à 140 ms).
- **Rafales des autres marques** : un horodatage aux millisecondes (`SubSecTimeOriginal`) améliorerait la détection par l'heure.
