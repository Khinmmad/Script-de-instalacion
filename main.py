import subprocess
import os
import sys
import shutil

# Importamos las configuraciones, usando try-except para evitar errores si están vacíos o no existen
try:
    from config.paquetes import PAQUETES_OFICIALES
except ImportError:
    PAQUETES_OFICIALES = []
    print("Advertencia: No se encontró PAQUETES_OFICIALES en config/paquetes.py")

try:
    from config.paquetes_aur import PAQUETES_AUR
except ImportError:
    PAQUETES_AUR = []
    print("Advertencia: No se encontró PAQUETES_AUR en config/paquetes_aur.py")

try:
    from config.entorno import ENTORNOS
except ImportError:
    ENTORNOS = {}
    print("Advertencia: No se encontró ENTORNOS en config/entorno.py o config/entornos.py")

def ejecutar_comando(comando, cwd=None, exit_on_error=True):
    """Función auxiliar para ejecutar comandos y manejar errores."""
    try:
        print(f"\n> Ejecutando: {comando}")
        subprocess.run(comando, shell=True, check=True, cwd=cwd)
    except subprocess.CalledProcessError as e:
        print(f"[!] Error al ejecutar el comando: {comando}")
        if exit_on_error:
            print("Abortando instalación por error crítico.")
            sys.exit(e.returncode)
        else:
            print("Continuando a pesar del error...")

def instalar_aur():
    print("\n=== Verificando e instalando YAY ===")
    if shutil.which("yay"):
        print("yay ya está instalado.")
    else:
        print("yay no ha sido encontrado. Procediendo con la instalación...")
        pasos = [
            "sudo pacman -S --needed base-devel git --noconfirm",
            "git clone https://aur.archlinux.org/yay.git /tmp/yay",
        ]
        for paso in pasos:
            ejecutar_comando(paso)
        
        ejecutar_comando("makepkg -si --noconfirm", cwd="/tmp/yay")
        ejecutar_comando("rm -rf /tmp/yay", exit_on_error=False)

def instalar_paquetes_oficiales():
    if not PAQUETES_OFICIALES:
        print("\n=== No hay paquetes oficiales definidos para instalar. ===")
        return
        
    print("\n=== Instalando paquetes oficiales ===")
    paquetes_str = " ".join(PAQUETES_OFICIALES)
    ejecutar_comando(f"sudo pacman -S --noconfirm {paquetes_str}", exit_on_error=False)

def instalar_paquetes_aur():
    if not PAQUETES_AUR:
        print("\n=== No hay paquetes de AUR definidos para instalar. ===")
        return
        
    print("\n=== Instalando paquetes de AUR ===")
    paquetes_str = " ".join(PAQUETES_AUR)
    ejecutar_comando(f"yay -S --noconfirm {paquetes_str}", exit_on_error=False)

def elegir_entorno():
    if not ENTORNOS:
        print("\n=== No hay entornos de escritorio configurados. ===")
        return None, []

    # Detectamos si es un diccionario (nueva versión) o una lista (versión anterior)
    entornos_dict = ENTORNOS if isinstance(ENTORNOS, dict) else {f"Opción {i+1}": [e] for i, e in enumerate(ENTORNOS)}

    print("\n=== Elige tu entorno de escritorio ===")
    nombres_entornos = list(entornos_dict.keys())
    for i, nombre in enumerate(nombres_entornos):
        paquetes = len(entornos_dict[nombre])
        print(f"[{i+1}] {nombre} ({paquetes} paquetes)")
    print(f"[{len(nombres_entornos)+1}] Saltar instalación de entorno")
    
    while True:
        try:
            opcion = int(input("\nOpción: ")) - 1
            if 0 <= opcion < len(nombres_entornos):
                nombre_elegido = nombres_entornos[opcion]
                return nombre_elegido, entornos_dict[nombre_elegido]
            elif opcion == len(nombres_entornos):
                return None, []
            else:
                print("Por favor, selecciona un número válido.")
        except ValueError:
            print("Entrada no válida. Introduce un número.")

def instalar_entornos():
    nombre_entorno, paquetes_entorno = elegir_entorno()
    if nombre_entorno and paquetes_entorno:
        print(f"\n=== Instalando entorno: {nombre_entorno} ===")
        paquetes_str = " ".join(paquetes_entorno)
        ejecutar_comando(f"sudo pacman -S --noconfirm {paquetes_str}", exit_on_error=False)
    else:
        print("\nOmitiendo instalación de entorno de escritorio.")

if __name__ == "__main__":
    # Evita romper en otros sistemas e impide correr como root directo
    if hasattr(os, 'geteuid') and os.geteuid() == 0:
        print("Error: No ejecutes este script como root (no uses sudo script.py).")
        print("El propio script pedirá la contraseña para comandos específicos.")
        sys.exit(1)

    print("Iniciando instalador Arch Linux post-instalación de Isra...")
    
    instalar_aur()
    instalar_paquetes_oficiales()
    instalar_paquetes_aur()
    instalar_entornos()
    
    print("\n¡Instalación completada! Es recomendable reiniciar el sistema.")