Je veux un visionneur de photos minimaliste pour Fedora 44 (GNOME/Wayland,
Ryzen AM5, 32 Go RAM, RX 9060 XT), écrit en Rust, pour dérusher des rafales
de JPEG (OM System OM-1 Mk II, 20 Mpx, ~10 Mo/fichier).

Comportement essentiel (copié d'ACDSee Pro) :
- Je maintiens flèche droite (ou Page Down) : les photos défilent en continu,
  je les vois TOUTES, sans attendre ni sauter d'image, même hors cache.
- Je relâche : le défilé s'arrête et la photo s'affiche en pleine résolution.
- Je peux revenir en arrière (flèche gauche / Page Up), marquer la photo
  (touche à définir), ou relancer le défilé. Tout doit être instantané.
- Problème de Geeqie à ne pas reproduire : il abandonne le décodage à
  chaque répétition de touche et n'affiche rien avant le relâchement.

Architecture souhaitée :
- Décodage JPEG avec zune-jpeg ou turbojpeg (libjpeg-turbo), décodage réduit
  (1/4 ou 1/2) pendant le défilé, pleine résolution après ~100 ms sans
  changement.
- Pool de threads, préchargement de ±10 images, cache LRU en RAM,
  annulation des tâches périmées.
- Détecter appui/relâchement de la touche (pas la répétition clavier).
- Affichage GPU via winit + wgpu. Orientation EXIF respectée.
- Marquage : copie ou déplacement dans un sous-dossier "retenus/" au début.
- Un dossier en argument ; pas de base de données, pas de vignettes.

Commence par un prototype minimal (fenêtre, défilé, réduit puis pleine
résolution), puis on itère. Je testerai sur de vraies rafales.