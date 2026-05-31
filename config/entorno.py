import subprocess

base = [
    "xorg",
    "xorg-server",
    "xorg-xinit",
    "mesa",
    "networkmanager",
    "bluez",
    "bluez-utils",
    "pipewire",
    "pipewire-pulse",
    "wireplumber",
    "git",
    "base-devel"
]

kde = [
    "plasma",
    "kde-applications",
    "sddm"
]

gnome = [
    "gnome",
    "gdm"
]

hyprland = [
    "hyprland",
    "waybar",
    "wofi",
    "xdg-desktop-portal-hyprland"
]

qtile = [
    "qtile",
    "python",
    "python-pywlroots"
]

display_managers = {
    "kde": ["sddm"],
    "gnome": ["gdm"],
    "hyprland": ["sddm"], 
    "qtile": ["lightdm"]
}

wayland = [
    "wayland",
    "wlroots"
]

x11 = [
    "xorg",
    "xorg-xinit"
]



def instalar_entorno(entorno):
    paquetes = []
    
    paquetes.extend(base)
    paquetes.extend(entorno)