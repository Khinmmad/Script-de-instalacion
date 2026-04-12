# Aquí definimos los entornos de escritorio como un diccionario.
# La clave (nombre del entorno) es lo que verá el usuario.
# El valor es la lista de paquetes que se instalarán para ese entorno.

ENTORNOS = {
    "KDE Plasma": [
        "plasma",
        "sddm",     # Gestor de inicio de sesión
        "konsole",
        "dolphin"
    ],
    "GNOME": [
        "gnome",
        "gdm",      # Gestor de inicio de sesión de GNOME
        "gnome-tweaks",
        "gnome-terminal"
    ],
    "Hyprland": [
        "hyprland",
        "sddm",     # Gestor de inicio de sesión (recomendado y funciona bien con wayland)
        "kitty",    # Terminal muy usada en Hyprland
        "wofi",     # Lanzador de aplicaciones por defecto en wayland
        "waybar",   # Barra de estado
        "dunst",    # Demonio de notificaciones
        "hyprpaper", # Para fondos de pantalla en Hyprland
        "polkit-kde-agent",  # necesario para permisos gráficos en wayland
        "xdg-desktop-portal-hyprland"  # para screensharing y más
    ]
}
