# Rafale — étude de la v3 : juger les rafales

## Le besoin

Aujourd'hui Rafale juge des **photos** : étoiles pour la qualité, coche ✓ pour « je la retouche ». Mais le vrai travail de tri se fait à deux niveaux :

1. **Quelles rafales valent le coup ?** Une séquence accroche l'œil ou non, indépendamment de la meilleure vue qu'elle contient.
2. **Dans chaque rafale intéressante, quelle est la meilleure photo ?** Et s'assurer qu'on l'a bien trouvée et retenue.

La v3 ajoute donc un **marquage des rafales** (note, retenue, rejet), un **mode « rafale bloquée »** pour l'examiner seule, et des **règles de cohérence** entre la rafale et ses photos, qui signalent ce qu'il reste à faire.

## 1. Bloquer une rafale

- **Entrée** bloque la rafale de la photo affichée ; Entrée à nouveau la débloque.
- Rafale bloquée :
  - ← → ne défilent qu'à l'intérieur de la rafale et s'arrêtent à ses extrémités ; le défilé continu aussi ;
  - **↑ ↓ passent à la rafale précédente / suivante en restant bloqué** (décidé : au second passage, on enchaîne les rafales retenues sans débloquer et rebloquer) ;
  - Début / Fin vont à la première / dernière vue de la rafale ;
  - la liste des rafales est remplacée par une **grille de vignettes de la rafale**, régulièrement espacées (autant de rangées de 4 que le panneau en contient : 16 vues pour une rafale de 64, toutes si elle est courte), avec le marquage de chaque vue (étoiles, ✓, ✕) et le cadre bleu sur la vue affichée ; un clic va à la vue ;
  - le panneau et le titre indiquent « Rafale 12 bloquée » ;
  - le filtre s'applique toujours : seules les vues qui le passent sont parcourues et montrées.
- **Vue À plat** : les photos y sont isolées ; le blocage et le marquage des rafales y sont désactivés.

## 2. Marquer une rafale

Une rafale a son propre marquage, indépendant de celui de ses photos :

| Marque | Clavier | Panneau |
|---|---|---|
| Note de 1 à 5 étoiles | Maj+1 à Maj+5, Maj+0 pour l'effacer | seconde rangée de commandes, « Rafale : ★★★★★ ✓ ✕ », sous celle de la photo |
| Retenue ✓ | Maj+Espace | case ✓ de la rangée « Rafale » |
| Rejetée ✕ | Maj+X ou Maj+R | case ✕ de la rangée « Rafale » |

- Le marquage de la rafale s'applique à la rafale de la photo affichée, bloquée ou non.
- **Le rejet d'une rafale est une marque, comme pour une photo** : il ne masque rien et ne touche pas aux photos ; Maj+X à nouveau la réintègre.
- La liste des rafales affiche le marquage de chaque rafale (étoiles, ✓, ✕) en plus du compte « ✓ n » des photos retenues, et ⚠ si une règle n'est pas respectée.

### Rangement : un fichier par rafale

- Un petit fichier texte par rafale **marquée**, dans le dossier de la séance, nommé d'après sa première et sa dernière vue : `PA290128-PA290142.rafale`. Une rafale sans marque n'a pas de fichier ; effacer tout son marquage supprime le fichier (comme pour les `.xmp`).
- Contenu (format clé = valeur, lisible et modifiable à la main) :

  ```
  rafale-viewer 3
  premiere = PA290128.JPG
  derniere = PA290142.JPG
  vues = 15
  note = 3
  retenue = oui
  rejetee = non
  ```

- **Identité de la rafale** : au chargement, le fichier s'applique à la rafale détectée qui contient sa première vue, ou à défaut sa dernière (si la première a été supprimée) ; il est renommé à la prochaine écriture si la rafale a changé de bornes.
- Rafale ne lit pas ces fichiers comme des photos ni comme des RAW (extension inconnue des deux).

## 3. Règles de cohérence

1. ~~**Retenir une photo retient sa rafale**~~ : abandonné à l'usage (section 8). Le marquage d'une rafale et celui de ses photos sont indépendants ; la règle 2 rappelle de faire les deux.
2. **Une rafale retenue doit contenir au moins une photo retenue.**
3. **Une rafale notée X doit contenir au moins une photo notée X ou plus** (photos rejetées exclues).
4. **Une rafale rejetée ne doit contenir aucune photo retenue.**

Une rafale qui ne respecte pas une règle est **signalée** :

- ⚠ sur sa ligne dans la liste ;
- dans le panneau, une ligne qui dit quoi faire : « Rafale notée 4 : aucune photo à 4 étoiles ou plus. Trouve-la, ou baisse la note de la rafale. » ;
- à **Ctrl+C** et à **« Retoucher… »**, une confirmation récapitule les rafales signalées (« 2 rafales retenues sans photo retenue : 12, 37 ») avec « Copier quand même » ou « Annuler ». Sans signalement, pas de fenêtre : la copie part directement.

Pour lever un signalement, on trouve la photo qui mérite la note (ou la coche), ou on baisse la note de la rafale (ou on la décoche).

## 4. Corbeille d'une rafale

- Quand la rafale affichée est **bloquée et rejetée**, le bouton rouge devient **« Corbeille : rafale 12 (15 photos) »** : il envoie à la corbeille toutes les photos de la rafale (JPEG, RAW, `.xmp`, `.pp3`) et son fichier `.rafale`, après la même confirmation que pour les photos rejetées (nombre de fichiers, place libérée).
- Les rafales partent donc **une par une**, en connaissance de cause.
- Hors de ce cas, le bouton garde son rôle actuel : les photos rejetées.
- Si la rafale contient une photo retenue (règle 4), la confirmation le signale.

## 5. Le flux visé

1. **Premier passage, la séance entière** : défilé continu ; sur une rafale qui accroche, Entrée pour la bloquer, la parcourir et regarder la grille ; la noter (Maj+1 à 5), la retenir (Maj+Espace) ou la rejeter (Maj+X) ; Entrée, et on continue.
2. **Second passage, les rafales retenues** : bloqué, ↓ de rafale en rafale ; dans chacune, trouver la meilleure vue (N pour la plus nette, loupe AF, grille), lui donner sa note et la cocher (Espace).
3. **Les ⚠ disent ce qu'il reste à faire** ; quand il n'y en a plus, « Retoucher… » part sans avertissement.
4. **Ménage** : bloquer chaque rafale rejetée et la mettre à la corbeille, une par une.

## 6. Étapes de réalisation (toutes faites, version 3.0.0)

| Étape | Contenu |
|---|---|
| 1 | Marquage des rafales : fichiers `.rafale`, touches Maj, rangée « Rafale » du panneau, affichage dans la liste |
| 2 | Rafale bloquée : défilé limité, ↑ ↓ en restant bloqué, grille de vignettes espacées |
| 3 | Règles de cohérence : retenue automatique de la rafale, ⚠ dans la liste et le panneau, confirmation à la copie et à « Retoucher… » |
| 4 | Corbeille d'une rafale bloquée et rejetée |
| 5 | Finitions (section 8) ; aide, guide et conception à jour ; version 3.0.0 |

## 7. Points tranchés

- Filtres sur les rafales (n'afficher que les rafales retenues, ou notées X ou plus) : d'abord écartés tant que le filtre des photos suffisait. Il ne suffit pas au second passage : les photos des rafales retenues n'y sont encore ni cochées ni notées. Ajoutés à la fin : rangée « Rafales » de la barre de filtre, cumulée avec celle des photos (décidé).
- La note de la rafale n'est pas exportée vers les autres logiciels (décidé) : elle sert au tri dans Rafale.
- Échap débloque la rafale bloquée au lieu de fermer Rafale (décidé à l'étape 2).
- Rejeter une rafale efface sa note et sa coche, comme pour une photo ; la noter ou la cocher lève le rejet (décidé à l'étape 1).
- Le marquage et le blocage des rafales attendent la fin de l'analyse des en-têtes, qui fixe les bornes des rafales (décidé à l'étape 1).

## 8. À peaufiner à la fin (fait dans la 3.0.0)

- **Cases ✓ et ✕ lisibles décochées** : pour la photo comme pour la rafale, une case non cochée montre sa coche ou sa croix en gris, pour qu'on les distingue sans les cocher.
- **Mode en cours bien visible dans le panneau de droite** : rafale bloquée ou non, avec un symbole distinct et assez voyant pour s'en rendre compte d'un coup d'œil.
- **Retenir une photo ne retient plus sa rafale** (abandon de la règle 1) : Espace retient la photo, Maj+Espace la rafale, symétriques et indépendants du contexte, même en rafale bloquée. L'utilisateur fait les deux ; la règle 2 signale une rafale retenue sans photo retenue.
- **Cadre vert de la zone AF absent au démarrage** : il s'active avec C si on le veut.
