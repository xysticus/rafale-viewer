#!/bin/sh
# Installe Rafale pour l'utilisateur courant (sans sudo) : programme dans
# ~/.local/bin, entrée de menu et « Ouvrir avec » dans ~/.local/share.
# À relancer après chaque `cargo build --release` pour mettre à jour le programme.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release/rafale-viewer"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

if [ ! -x "$bin" ]; then
    echo "Programme absent : lancer d'abord « cargo build --release »." >&2
    exit 1
fi

install -Dm755 "$bin" "$HOME/.local/bin/rafale-viewer"
install -Dm644 "$root/packaging/rafale-viewer.svg" "$data/icons/hicolor/scalable/apps/rafale-viewer.svg"
# Chemin absolu : le menu de GNOME ne voit pas toujours ~/.local/bin dans son PATH.
sed "s|^Exec=rafale-viewer|Exec=$HOME/.local/bin/rafale-viewer|" \
    "$root/packaging/rafale-viewer.desktop" > "$data/applications/rafale-viewer.desktop"
chmod 644 "$data/applications/rafale-viewer.desktop"
# Entrée directe du clic droit dans Fichiers ; sans le paquet nautilus-python, le fichier reste inutilisé.
install -Dm644 "$root/packaging/rafale-nautilus.py" "$data/nautilus-python/extensions/rafale-nautilus.py"

update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true

echo "Rafale $("$HOME/.local/bin/rafale-viewer" --version | cut -d' ' -f2) installé."
echo "Menu des applications : « Rafale »."
if rpm -q nautilus-python >/dev/null 2>&1; then
    echo "Fichiers : clic droit sur un dossier ou une photo › Ouvrir dans Rafale (après « nautilus -q »)."
else
    echo "Fichiers : clic droit sur un dossier ou une photo › Ouvrir avec… › Rafale."
    echo "Pour un accès direct au clic droit : sudo dnf install nautilus-python, puis nautilus -q."
fi
