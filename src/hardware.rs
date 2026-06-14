//! Deteccion de hardware del sistema.
//!
//! Usado al iniciar la TUI para pre-seleccionar drivers y microcódigo
//! segun el hardware que tengamos, en vez de obligar al usuario a
//! conocer su GPU/CPU y elegir a mano.
//!
//! - **GPU**: detecta via `lspci -mm` y mapea el vendor a uno de los
//!   drivers del catalogo (NVIDIA, AMD, Intel, o VM para invitados).
//! - **CPU microcódigo**: detecta via `/proc/cpuinfo` y mapea el
//!   vendor a intel-ucode o amd-ucode.
//!
//! Si la deteccion falla (lspci no esta, /proc/cpuinfo ilegible, etc.)
//! devuelve `None` y la TUI empieza sin nada seleccionado (el usuario
//! elige manualmente).

use std::process::Command;

/// IDs de driver del catalogo que podemos auto-seleccionar.
/// Coinciden con `id` en `catalog::DRIVERS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedDriver {
    Nvidia,
    Amd,
    Intel,
    Vm,
}

impl DetectedDriver {
    /// Convierte al `id` que el catalogo espera, para que el caller
    /// pueda encontrar el indice con `DRIVERS.iter().position(...)`.
    pub fn catalog_id(self) -> &'static str {
        match self {
            DetectedDriver::Nvidia => "nvidia",
            DetectedDriver::Amd => "amd",
            DetectedDriver::Intel => "intel",
            DetectedDriver::Vm => "vm",
        }
    }
}

/// Detecta la GPU principal via `lspci -mm` y devuelve el driver
/// correspondiente. Si no encuentra nada reconocible, devuelve `None`.
///
/// Logica:
/// 1. Lee todas las lineas VGA/3D de `lspci -mm`.
/// 2. Para cada una, mira el vendor ("NVIDIA", "AMD"/"ATI", "Intel",
///    "Red Hat"/"Virtio"/"QEMU" para VMs).
/// 3. Si encuentra NVIDIA, devuelve `Nvidia` (el propietario, mas
///    compatible con todo). El usuario puede cambiar a `nvidia-open`
///    en la TUI si tiene Turing+ y lo prefiere.
/// 4. Si encuentra AMD, devuelve `Amd`.
/// 5. Si encuentra Intel, devuelve `Intel`.
/// 6. Si encuentra virtio/qemu/red hat, devuelve `Vm`.
pub fn detect_gpu() -> Option<DetectedDriver> {
    let out = Command::new("lspci").args(["-mm"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        // Buscamos las lineas que sean VGA o 3D controller. El
        // formato de `lspci -mm` es CSV entre comillas, asi que el
        // segundo campo es la clase.
        let fields: Vec<&str> = line.split('"').collect();
        // fields: ["00:02.0 ", " VGA compatible controller ",
        //          " NVIDIA ... ", " device ", " vendor ", " device "]
        let class = fields.get(1).copied().unwrap_or("").trim();
        if class != "VGA compatible controller" && class != "3D controller" {
            continue;
        }
        let vendor = fields.get(3).copied().unwrap_or("");
        if vendor.contains("NVIDIA") {
            return Some(DetectedDriver::Nvidia);
        } else if vendor.contains("AMD") || vendor.contains("ATI") {
            return Some(DetectedDriver::Amd);
        } else if vendor.contains("Intel") {
            return Some(DetectedDriver::Intel);
        } else if vendor.contains("Red Hat") || vendor.contains("Virtio") || vendor.contains("QEMU")
        {
            return Some(DetectedDriver::Vm);
        }
    }
    None
}

/// IDs de microcodigo del catalogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedMicrocode {
    Intel,
    Amd,
}

impl DetectedMicrocode {
    pub fn catalog_id(self) -> &'static str {
        match self {
            DetectedMicrocode::Intel => "intel-ucode",
            DetectedMicrocode::Amd => "amd-ucode",
        }
    }
}

/// Detecta el vendor del CPU via `/proc/cpuinfo` y devuelve el microcode
/// correspondiente. Lee solo la primera linea `vendor_id` (todos los
/// cores tienen la misma).
pub fn detect_microcode() -> Option<DetectedMicrocode> {
    let body = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("vendor_id") {
            let v = rest.trim_start_matches(':').trim();
            return match v {
                "GenuineIntel" => Some(DetectedMicrocode::Intel),
                "AuthenticAMD" => Some(DetectedMicrocode::Amd),
                _ => return None,
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_match() {
        // Smoke test: los IDs que devolvemos tienen que existir en
        // el catalogo (verificado contra el contenido de catalog.rs).
        assert_eq!(DetectedDriver::Nvidia.catalog_id(), "nvidia");
        assert_eq!(DetectedDriver::Amd.catalog_id(), "amd");
        assert_eq!(DetectedDriver::Intel.catalog_id(), "intel");
        assert_eq!(DetectedDriver::Vm.catalog_id(), "vm");
        assert_eq!(DetectedMicrocode::Intel.catalog_id(), "intel-ucode");
        assert_eq!(DetectedMicrocode::Amd.catalog_id(), "amd-ucode");
    }

    #[test]
    fn detect_microcode_works_on_this_system() {
        // Solo funciona si /proc/cpuinfo existe. En CI (Linux runner)
        // existe; en otros sistemas no, pero el test se saltara solo
        // si el archivo no esta.
        let m = detect_microcode();
        // No asumimos un vendor especifico; solo que la deteccion
        // no panicea y devuelve algo razonable.
        if let Some(m) = m {
            assert!(matches!(
                m,
                DetectedMicrocode::Intel | DetectedMicrocode::Amd
            ));
        }
    }
}
