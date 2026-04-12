# Script de Post-Instalación - Arch Linux

Script en Python para automatizar la instalación de paquetes y entorno de escritorio después de instalar Arch Linux.

## Características

- Instala yay (AUR helper) automáticamente si no está presente
- Instala paquetes oficiales de pacman
- Instala paquetes del AUR
- Permite elegir tu entorno de escritorio (KDE Plasma, GNOME, Hyprland)
- Manejo de errores para no abortar toda la instalación si un paquete falla

## Requisitos

- Arch Linux recién instalado
- Conexión a internet
- Python 3

## Uso

```bash
git clone https://github.com/Khinmmad/Script-de-instalacion
cd Script-de-instalacion
python main.py
```

> No corras el script como root, el propio script pedirá contraseña cuando sea necesario.

## Estructura del proyecto
Script-de-instalacion/ ├── main.py # Script principal ├── config/ │ ├── paquetes.py # Paquetes oficiales │ ├── paquetes_aur.py # Paquetes del AUR │ └── entorno.py # Entornos de escritorio └── .gitignore

## Paquetes incluidos

**Oficiales:** firefox, vlc, vim, git, htop, fastfetch...

**AUR:** spotify, visual-studio-code-bin, ags...

**Entornos disponibles:**
- KDE Plasma
- GNOME
- Hyprland

## Personalización

Edita los archivos en la carpeta `config/` para agregar o quitar paquetes según tus necesidades.