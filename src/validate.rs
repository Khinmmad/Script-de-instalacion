//! Validadores de los datos que el usuario ingresa en la TUI (hostname,
//! locale, keymap, zona horaria). Sirven como primera linea de defensa: si el
//! valor no es valido, no llega al instalador y se omite del plan.
//!
//! El instalador tambien escribe los valores de forma segura (archivo
//! temporal + sudo), asi que un valor raro no podria causar inyeccion de
//! comandos aunque se colara por otra via (perfil editado a mano, etc).

/// True si `s` es un hostname valido para `/etc/hostname` (RFC 1123
/// simplificado: letras ASCII, digitos y guiones; no empieza/termina con guion;
/// max 63 caracteres).
pub fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// True si `s` parece un locale razonable (ej. "es_MX.UTF-8"). No comprueba
/// que exista en `/usr/share/i18n/locales`; eso es responsabilidad de
/// `locale-gen` y se ve en el log.
pub fn is_valid_locale(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 60
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
}

/// True si `s` parece un keymap de consola valido (ej. "la-latin1", "es").
pub fn is_valid_keymap(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 50
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// True si `s` parece una zona horaria IANA valida (ej. "America/Mexico_City").
/// No comprueba que el archivo exista bajo `/usr/share/zoneinfo/`.
pub fn is_valid_timezone(s: &str) -> bool {
    if s.is_empty() || s.len() > 60 {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[0] == b'/' || bytes[bytes.len() - 1] == b'/' {
        return false;
    }
    if s.contains("//") {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_accepts_valid() {
        assert!(is_valid_hostname("mi-arch"));
        assert!(is_valid_hostname("a"));
        assert!(is_valid_hostname("archlinux"));
        assert!(is_valid_hostname("a1b2c3"));
        assert!(is_valid_hostname("ABC-xyz-123"));
    }

    #[test]
    fn hostname_rejects_invalid() {
        assert!(!is_valid_hostname(""));
        assert!(!is_valid_hostname("-foo"));
        assert!(!is_valid_hostname("foo-"));
        assert!(!is_valid_hostname("foo bar"));
        assert!(!is_valid_hostname("foo'; rm -rf /"));
        assert!(!is_valid_hostname("foo.bar"));
        assert!(!is_valid_hostname("ñ"));
        assert!(!is_valid_hostname(&"a".repeat(64)));
    }

    #[test]
    fn locale_accepts_valid() {
        assert!(is_valid_locale("es_MX.UTF-8"));
        assert!(is_valid_locale("en_US"));
        assert!(is_valid_locale("C"));
        assert!(is_valid_locale("POSIX"));
    }

    #[test]
    fn locale_rejects_invalid() {
        assert!(!is_valid_locale(""));
        assert!(!is_valid_locale("es MX"));
        assert!(!is_valid_locale("es'; rm -rf /"));
        assert!(!is_valid_locale("es|MX"));
    }

    #[test]
    fn keymap_accepts_valid() {
        assert!(is_valid_keymap("la-latin1"));
        assert!(is_valid_keymap("es"));
        assert!(is_valid_keymap("us"));
    }

    #[test]
    fn keymap_rejects_invalid() {
        assert!(!is_valid_keymap(""));
        assert!(!is_valid_keymap("es lat"));
        assert!(!is_valid_keymap("es'; rm"));
    }

    #[test]
    fn timezone_accepts_valid() {
        assert!(is_valid_timezone("America/Mexico_City"));
        assert!(is_valid_timezone("Europe/Madrid"));
        assert!(is_valid_timezone("UTC"));
    }

    #[test]
    fn timezone_rejects_invalid() {
        assert!(!is_valid_timezone(""));
        assert!(!is_valid_timezone("/America/Mexico_City"));
        assert!(!is_valid_timezone("America/Mexico_City/"));
        assert!(!is_valid_timezone("America//Mexico_City"));
        assert!(!is_valid_timezone("../../etc/passwd"));
        assert!(!is_valid_timezone("America Mexico City"));
        assert!(!is_valid_timezone("America; rm -rf /"));
    }
}
