# Rafale — étude de la v4 : la planche-contact

Étude de la piste notée le 8 novembre 2026 dans [PISTES.md](PISTES.md) (« Vue à plat pour toutes les séances : une vignette par séquence »), tranchée point par point le 15 novembre 2026.

## Le besoin

La vue à plat montre une vignette par photo. Elle n'est proposée qu'en dessous de 300 photos : au-delà, la grille est trop longue à parcourir. Or les séances réelles sont bien plus grosses, et souvent mixtes :

| Séance | Photos | Séquences | Rafales | Vues isolées |
|---|---|---|---|---|
| 29 octobre 2026 | 2 355 | 115 | 112 | 3 |
| 15 novembre 2026 | 2 792 | 315 | 71 (de 6 à 152 vues, médiane 25) | 244 |

En ne montrant **qu'une vignette par séquence** (une rafale ou une vue isolée), la vue à plat devient une **planche-contact** de la séance, quelle que soit sa taille : 315 vignettes au lieu de 2 792 pour la séance du 15 novembre. Elle complète la vue Rafales : la liste montre la structure (4 vignettes par rafale, heure, rythme), la planche le meilleur de chaque séquence d'un coup d'œil.

## 1. La planche

- Une vignette par **séquence** : une vue isolée garde sa vignette ; une rafale est représentée par une seule de ses vues (section 4).
- **Plus de limite de 300 photos** : la planche est disponible pour toutes les séances (`FLAT_MAX` disparaît).
- La vue Rafales et la rafale bloquée (grille complète des vues d'une rafale) ne changent pas.

## 2. Le signe d'une séquence

- **Effet de pile** : deux bords décalés derrière la vignette, comme des tirages empilés. Il se repère instantanément, même en faisant défiler 80 rangées, ce qui compte avec une majorité de vues isolées.
- **Pastille de nombre** en bas à droite : « ×25 », dans un petit cartouche sombre. Elle dit combien de vues sont derrière la vignette (de 6 à 152 le 15 novembre), utile pour décider d'entrer ou non dans la rafale.
- Une vue isolée n'a ni pile ni pastille.
- Le bleu reste réservé au cadre de la vue affichée.

## 3. Le marquage affiché

- **En haut de la vignette, le marquage de la photo** qui représente la séquence, comme pour une vue isolée : étoiles ou croix de rejet en haut à gauche, coche ✓ dessous, couleur en haut à droite.
- **En bas à gauche, le marquage de la rafale** dans un cartouche distinct, avec un contour (ses étoiles), pour ne pas le confondre avec celui de la photo (même leçon que le nombre de photos retenues dans la liste des rafales).
- **Rafale retenue** : les bords de la pile en vert.
- **Rafale rejetée**, ou rafale dont toutes les vues sont rejetées : vignette **grisée, pas masquée** (le rejet n'est qu'une marque).

## 4. La photo qui représente une séquence

1. la photo **retenue** (✓) la mieux notée ;
2. à défaut, la photo la mieux notée ;
3. à défaut, la vue du **milieu** de la séquence.

- Les photos **rejetées** sont exclues du choix. Si toutes les vues d'une rafale sont rejetées : la vue du milieu, grisée.
- **Égalités** entre photos de même note : **la première dans l'ordre de la rafale**. La netteté AF a été écartée : mesurée à l'arrêt et en tâche de fond, elle ferait changer la vignette d'elle-même. Pour trouver la plus nette, N dans la rafale.
- Le choix n'est **recalculé qu'après un marquage** (ou un changement de filtre), jamais pendant le défilé.

## 5. Le filtre

- La photo qui représente une séquence est choisie **parmi celles qui passent le filtre** : avec ★3, une rafale est représentée par sa meilleure photo à 3 étoiles ou plus.
- Une séquence dont aucune vue ne passe le filtre est **masquée**, comme dans la liste des rafales.
- Le **filtre des rafales** (rangée « Rafales » : note, rafale retenue) s'applique aussi.
- Au second passage, le filtre ✓ réduit la planche aux séquences où une photo est retenue, chacune représentée par elle.

## 6. Navigation

| Geste (planche) | Rôle |
|---|---|
| **← → ↑ ↓** | se déplacer dans la planche, de vignette en vignette (gauche, droite, rangée du dessus, du dessous) ; on arrive sur la photo qui représente la séquence |
| **Page préc. / Page suiv.** | vignette précédente / suivante, **sans jamais entrer dans une rafale** |
| **Clic** | affiche la photo qui représente la séquence |
| **Entrée** | entrer dans la séquence : **rafale bloquée**, grille complète de ses vues, comme en v3 ; Entrée ou Échap pour revenir à la planche |

- C'est le seul moyen de voir toutes les vues d'une rafale depuis la planche ; une fois la rafale bloquée, le défilé continu (flèches maintenues) fonctionne comme en v3.
- Écarté : Page préc. / Page suiv. qui feraient défiler les vues de la rafale, avec arrêt au bout ; la sortie de la rafale aurait été confuse.
- En vue Rafales, rien ne change.

## 7. La vignette qui change

- Marquer une photo peut changer celle qui représente sa séquence (par exemple en retenant une vue mieux notée) : la vignette se met à jour au marquage, **sans animation**.
- **Le cadre bleu suit la séquence, pas la photo** : il reste sur la même case, et la photo affichée reste celle qu'on regardait.

## 8. La vue au lancement

1. **La dernière vue utilisée pour ce dossier** (Rafales ou planche), retenue avec la dernière photo vue dans `~/.local/state/rafale/positions` (rien dans le dossier des photos).
2. Pour un dossier jamais ouvert, compter les **rafales de plus de 5 vues** (seuil actuel, `FLAT_BURST_MAX`) :
   - aucune ou une seule : **planche** ;
   - au moins deux : **vue Rafales**.

## 9. Performances

Exigence : ne pas ralentir l'affichage.

- La pile, la pastille et les cartouches sont dessinés **avec la vignette**, sur le canevas du panneau, **une seule fois**, quand la grille est recomposée (changement de rangée, de marquage ou de filtre), jamais à chaque photo du défilé : quelques rectangles et un petit texte par vignette, négligeables à côté de la vignette.
- Le **cadre bleu** reste un quad de la carte graphique, comme aujourd'hui (`Track` avec `span` : la vignette tient lieu de toutes les vues de la séquence).
- Le choix des photos représentantes : un passage sur les séquences, après un marquage seulement.
- Vérifier avec `--bench-scroll` que le défilé en rafale bloquée garde ses 25 images/s, et que la planche de la séance du 15 novembre (315 vignettes ; cache de vignettes de 512 entrées) reste fluide.

## 10. Étapes de réalisation

| Étape | Contenu |
|---|---|
| 1 | Planche : une vignette par séquence, choix de la photo représentante (section 4), filtre (section 5), plus de limite de 300 photos |
| 2 | Signe des séquences (pile, pastille) et marquage (photo en haut, rafale en bas à gauche, pile verte, vignette grisée) |
| 3 | Navigation : flèches dans la planche, Page préc. / suiv. de vignette en vignette, Entrée pour la rafale bloquée et retour |
| 4 | Vue au lancement : mémoire par dossier, règle des rafales de plus de 5 vues |
| 5 | Mesures, aide, guide, conception et README à jour ; version 4.0.0 |

Séance d'essai : `~/Images/2026-11-15` (mixte rafales et vues isolées, déjà triée).
