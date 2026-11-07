"""Entrée « Ouvrir dans Rafale » du clic droit dans Fichiers (nautilus-python)."""

import os
import subprocess

from gi.repository import GObject, Nautilus

PROGRAM = os.path.expanduser("~/.local/bin/rafale-viewer")


def launch(_item, path):
    subprocess.Popen(
        [PROGRAM, path],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


def item(name, path):
    entry = Nautilus.MenuItem(name=name, label="Ouvrir dans Rafale")
    entry.connect("activate", launch, path)
    return entry


class RafaleMenu(GObject.GObject, Nautilus.MenuProvider):
    def get_file_items(self, files):
        """Un dossier ou un JPEG local, sélectionné seul."""
        if len(files) != 1:
            return []
        file = files[0]
        path = file.get_location().get_path()
        if path is None or not (file.is_directory() or file.get_mime_type() == "image/jpeg"):
            return []
        return [item("Rafale::ouvrir", path)]

    def get_background_items(self, folder):
        """Le fond de la fenêtre : le dossier affiché."""
        path = folder.get_location().get_path()
        return [item("Rafale::ouvrir-dossier", path)] if path else []
