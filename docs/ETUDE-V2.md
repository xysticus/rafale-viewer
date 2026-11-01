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
| 6 | Intégration à GNOME | `packaging/install.sh` installe dans `~/.local` le programme, une icône et un `.desktop` : entrée « Rafale » dans le menu, *Ouvrir avec… › Rafale* sur un dossier ou une photo. Sans argument, Rafale demande le dossier ; sur une photo, il ouvre son dossier en commençant par elle ; hors terminal, les erreurs s'affichent dans une fenêtre. |

## À faire

### 4. Mettre les rejetées à la corbeille

Envoyer à la corbeille de GNOME (`gio trash`, donc récupérable) les photos marquées rejetées, avec leur RAW et leurs `.xmp`, après une confirmation qui annonce le nombre de fichiers et la place libérée. Intérêt : une séance comme celle du 29/10 pèse 32 Go. Points à trancher : rejetées seulement, ou aussi les non retenues d'un dossier entièrement trié ; et le comportement si le RAW a un `.xmp` d'un autre logiciel (darktable).

### 5. « Ouvrir les retenues dans… »

Un bouton qui lance le logiciel de retouche (RawTherapee, darktable…) directement sur les RAW retenus, ou sur la sélection filtrée. C'est la suite de la copie vers Fichiers. À trancher : quel logiciel par défaut, et comment le choisir (fichier de réglages, ou les applications enregistrées dans GNOME pour le type ORF).

### 7. Reprise à la dernière photo vue

En rouvrant un dossier, revenir à la photo où on s'était arrêté, pour un tri fait en plusieurs fois. À trancher : où garder cette position sans créer de fichier dans le dossier des photos (par exemple `~/.local/state/rafale/`).

### 8. Import de la carte SD

Copier la carte vers `~/Images/AAAA-MM-JJ`, en rangeant les RAW dans `raw/`, puis ouvrir le dossier. Utile seulement si ce rangement se fait aujourd'hui à la main.

## Abandonné

- **Sélection multiple de vignettes** (Ctrl+clic, Maj+clic, Ctrl+A), pour la copie puis pour marquer plusieurs photos d'un coup. Elle a été réalisée, essayée, puis retirée : pas assez pratique pour la complexité qu'elle ajoute. La copie passe par le filtre ou les retenues.

## Pistes techniques en réserve

- **Décodage partiel de la zone AF** (`tj3SetCroppingRegion`) : il rendrait la mesure de netteté plus rapide, si elle se révèle lente sur de longues rafales ou un disque lent.
- **Décodage parallèle d'une même image** grâce aux marqueurs de redémarrage des JPEG OM : il réduirait la latence d'une photo jamais lue (aujourd'hui 100 à 140 ms).
- **Rafales des autres marques** : un horodatage aux millisecondes (`SubSecTimeOriginal`) améliorerait la détection par l'heure.
