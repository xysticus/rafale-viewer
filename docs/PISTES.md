# Rafale — pistes pour les versions suivantes (v4, v5)

Idées retenues pour plus tard, pas encore étudiées en détail. Chacune passera par une étude (comme `ETUDE-V3.md`) avant d'être réalisée.

## Vue à plat pour toutes les séances : une vignette par séquence

*Noté le 8 novembre 2026, piste pour la v4.*

**Le besoin.** La vue à plat n'est proposée qu'en dessous de 300 photos : au-delà, une vignette par photo donne une grille trop longue. En ne montrant qu'une vignette par séquence (rafale), elle devient une planche-contact de la séance, possible quelle que soit sa taille : 115 vignettes au lieu de 2 355 pour la séance du 29/10 (112 rafales, 3 vues isolées). Elle complète la vue par rafales : la liste montre la structure (4 vignettes par rafale, heure, rythme), la planche le meilleur de chaque séquence d'un coup d'œil.

**La règle.** Une vue isolée garde sa vignette. Une rafale est représentée par une seule vue, signalée comme une séquence ; le choix de cette vue :

1. la photo retenue la mieux notée ;
2. à défaut, la photo la mieux notée ;
3. à défaut, une vue du milieu de la séquence.

**À trancher dans l'étude.**

- **Le signe d'une séquence.** Le bleu est déjà le cadre de la vue affichée : plutôt une pastille avec le nombre de vues (« ×19 ») ou un effet de pile.
- **Clic et Entrée sur une séquence.** Proposition : un clic va à la vue choisie, Entrée bloque la rafale (grille complète) ; ← → parcourent toutes les vues, le cadre bleu restant sur la vignette tant qu'on est dans la séquence.
- **Le marquage affiché.** Celui de la vue choisie, celui de la rafale, ou les deux.
- **Les rejetées.** Exclues du choix de la vue ; rafale rejetée, ou toutes ses vues rejetées : vignette grisée, pas masquée (le rejet n'est qu'une marque).
- **Le filtre.** Vue choisie parmi celles qui le passent ; séquence sans vue visible masquée, comme dans la liste.
- **Les égalités.** Entre photos de même note : la première, la plus nette (si la netteté AF est mesurée) ou celle du milieu.
- **La vignette qui change.** Noter ou retenir une vue peut changer celle qui représente la séquence : logique, mais à rendre lisible.
- **La vue au lancement.** Aujourd'hui à plat, sauf rafale de plus de 5 vues ; à revoir si la vue à plat est toujours disponible.

## Signaler les vues qui sortent du lot dans une rafale

*Noté le 8 novembre 2026.*

**Le besoin.** Dans une rafale, une vue peut ne pas ressembler aux autres : une cavalière qui passe au premier plan et masque le sujet (PA290834, séance du 29/10), ou une reprise après une pause du boîtier, mémoire tampon pleine, l'appareil ayant bougé entre-temps (PA291586). Le boîtier les rattache à juste titre à la rafale ; c'est à l'œil de décider s'il faut les rejeter. Rafale peut attirer le regard sur elles, sans juger à la place du photographe.

**La méthode, essayée sur la séance du 29/10.** Calcul local, sans IA : chaque vue réduite à 48 × 36 pixels en niveaux de gris, écart moyen avec la vue précédente, comparé à l'écart médian de la rafale. Avec un écart supérieur à 4 fois la médiane et à 12 niveaux, une seule vue sort sur 104 rafales (PA290834) ; PA291586 n'est pas attrapée, le seuil est donc à régler. L'écart dit qu'une vue a changé, jamais pourquoi.

**À trancher dans l'étude.**

- Où calculer : à l'analyse des en-têtes, avec la vignette EXIF (160 × 120, déjà lue), pour ne rien coûter au défilé.
- Comment le montrer : une marque sur la vignette dans la grille d'une rafale bloquée, un repère dans la liste, ou une touche pour aller à la prochaine vue signalée.
- Le seuil : fixe, ou relatif à la rafale ; essayer sur plusieurs séances.
- Pas d'action automatique : ni rejet ni découpage de la rafale, la vue reste où le boîtier l'a mise.

**Écarté.** Comprendre la scène (« quelqu'un passe devant le sujet ») demanderait un modèle de vision : trop lourd à embarquer sur le PC, ou des photos envoyées à un service en ligne, contraire au principe de Rafale. Découper automatiquement la rafale à ces vues : écarté aussi (le boîtier a raison de les y rattacher).
