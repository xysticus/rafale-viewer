# Rafale — pistes pour les versions suivantes (v4, v5)

Idées retenues pour plus tard, pas encore étudiées en détail. Chacune passera par une étude (comme `ETUDE-V3.md`) avant d'être réalisée.

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
