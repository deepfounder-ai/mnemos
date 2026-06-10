//! Slug utilities — kebab-case ASCII, max 80 chars.

use crate::error::{AppError, Result};

const MAX_SLUG_LEN: usize = 80;

/// Slugify arbitrary text into a stable, file-system-safe identifier.
///
/// Rules:
/// 1. Transliterate common Latin/Cyrillic/Greek diacritics to ASCII.
/// 2. Lowercase.
/// 3. Map any non-`[a-z0-9]` run to a single `-`.
/// 4. Trim leading/trailing `-`.
/// 5. Truncate to 80 chars on a `-` boundary when possible.
pub fn slugify(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let transliterated = transliterate(input);

    let mut out = String::with_capacity(transliterated.len());
    let mut prev_dash = true; // collapse leading dashes
    for ch in transliterated.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    // Strip trailing dash.
    if out.ends_with('-') {
        out.pop();
    }

    if out.len() > MAX_SLUG_LEN {
        let cut = out[..MAX_SLUG_LEN].rfind('-').unwrap_or(MAX_SLUG_LEN);
        out.truncate(cut);
        if out.ends_with('-') {
            out.pop();
        }
    }
    out
}

/// Validate an externally supplied slug. Reject anything that doesn't match
/// `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$` and isn't pure ASCII, with a length
/// cap of 80.
pub fn validate(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(AppError::Validation("slug must not be empty".into()));
    }
    if slug.len() > MAX_SLUG_LEN {
        return Err(AppError::Validation(format!(
            "slug longer than {MAX_SLUG_LEN} chars"
        )));
    }
    let mut chars = slug.chars();
    let first = chars.next().expect("non-empty");
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(AppError::Validation(
            "slug must start with [a-z0-9]".into(),
        ));
    }
    if slug.ends_with('-') {
        return Err(AppError::Validation("slug must not end with '-'".into()));
    }
    for c in slug.chars() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if !ok {
            return Err(AppError::Validation(format!(
                "slug contains invalid character '{c}'"
            )));
        }
    }
    if slug.contains("--") {
        return Err(AppError::Validation("slug must not contain '--'".into()));
    }
    Ok(())
}

/// Quick compatibility check used by callers that want to know whether a
/// slug can serve as a filename on common filesystems.
pub fn is_filesystem_safe(slug: &str) -> bool {
    !slug.is_empty() && validate(slug).is_ok()
}

/// Hand-rolled transliteration table. Covers the diacritics & non-Latin
/// scripts most often encountered in user content. We intentionally keep
/// this dependency-free: a comprehensive table is large and rarely useful.
fn transliterate(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            // Latin diacritics
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ă' | 'Ą' => out.push('A'),
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'É' | 'È' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => out.push('E'),
            'í' | 'ì' | 'î' | 'ï' | 'ī' | 'ĩ' | 'ĭ' | 'į' => out.push('i'),
            'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' | 'Ĩ' | 'Ĭ' | 'Į' => out.push('I'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => out.push('o'),
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => out.push('O'),
            'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ũ' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
            'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' | 'Ũ' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => out.push('U'),
            'ý' | 'ÿ' | 'ŷ' => out.push('y'),
            'Ý' | 'Ÿ' | 'Ŷ' => out.push('Y'),
            'ñ' | 'ń' => out.push('n'),
            'Ñ' | 'Ń' => out.push('N'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'Ç' | 'Ć' | 'Č' => out.push('C'),
            'ś' | 'š' => out.push('s'),
            'Ś' | 'Š' => out.push('S'),
            // ß (German sharp s) → "ss"
            'ß' => out.push_str("ss"),
            'ẞ' => out.push_str("SS"),
            'ž' | 'ź' | 'ż' => out.push('z'),
            'Ž' | 'Ź' | 'Ż' => out.push('Z'),
            'ř' | 'ŕ' => out.push('r'),
            'Ř' | 'Ŕ' => out.push('R'),
            'ď' | 'đ' => out.push('d'),
            'Ď' | 'Đ' => out.push('D'),
            'ť' | 'ţ' | 'ț' => out.push('t'),
            'Ť' | 'Ţ' | 'Ț' => out.push('T'),
            'ł' => out.push('l'),
            'Ł' => out.push('L'),
            'ğ' => out.push('g'),
            'Ğ' => out.push('G'),
            'ĥ' => out.push('h'),
            'Ĥ' => out.push('H'),
            'ĵ' => out.push('j'),
            'Ĵ' => out.push('J'),
            'ķ' => out.push('k'),
            'Ķ' => out.push('K'),
            // Cyrillic
            'а' => out.push('a'), 'б' => out.push('b'), 'в' => out.push('v'),
            'г' => out.push('g'), 'д' => out.push('d'), 'е' => out.push('e'),
            'ё' => out.push('e'), 'ж' => out.push('z'), 'з' => out.push('z'),
            'и' => out.push('i'), 'й' => out.push('i'), 'к' => out.push('k'),
            'л' => out.push('l'), 'м' => out.push('m'), 'н' => out.push('n'),
            'о' => out.push('o'), 'п' => out.push('p'), 'р' => out.push('r'),
            'с' => out.push('s'), 'т' => out.push('t'), 'у' => out.push('u'),
            'ф' => out.push('f'), 'х' => out.push('h'), 'ц' => out.push('c'),
            'ч' => out.push('c'), 'ш' => out.push('s'), 'щ' => out.push('s'),
            'ъ' => out.push(' '), 'ы' => out.push('y'), 'ь' => out.push(' '),
            'э' => out.push('e'), 'ю' => out.push('u'), 'я' => out.push('y'),
            'А' => out.push('A'), 'Б' => out.push('B'), 'В' => out.push('V'),
            'Г' => out.push('G'), 'Д' => out.push('D'), 'Е' => out.push('E'),
            'Ё' => out.push('E'), 'Ж' => out.push('Z'), 'З' => out.push('Z'),
            'И' => out.push('I'), 'Й' => out.push('I'), 'К' => out.push('K'),
            'Л' => out.push('L'), 'М' => out.push('M'), 'Н' => out.push('N'),
            'О' => out.push('O'), 'П' => out.push('P'), 'Р' => out.push('R'),
            'С' => out.push('S'), 'Т' => out.push('T'), 'У' => out.push('U'),
            'Ф' => out.push('F'), 'Х' => out.push('H'), 'Ц' => out.push('C'),
            'Ч' => out.push('C'), 'Ш' => out.push('S'), 'Щ' => out.push('S'),
            'Ъ' => out.push(' '), 'Ы' => out.push('Y'), 'Ь' => out.push(' '),
            'Э' => out.push('E'), 'Ю' => out.push('U'), 'Я' => out.push('Y'),
            // Greek
            'α' => out.push('a'), 'β' => out.push('b'), 'γ' => out.push('g'),
            'δ' => out.push('d'), 'ε' => out.push('e'), 'ζ' => out.push('z'),
            'η' => out.push('i'), 'θ' => out.push('t'), 'ι' => out.push('i'),
            'κ' => out.push('k'), 'λ' => out.push('l'), 'μ' => out.push('m'),
            'ν' => out.push('n'), 'ξ' => out.push('x'), 'ο' => out.push('o'),
            'π' => out.push('p'), 'ρ' => out.push('r'), 'σ' => out.push('s'),
            'τ' => out.push('t'), 'υ' => out.push('u'), 'φ' => out.push('f'),
            'χ' => out.push('h'), 'ψ' => out.push('p'), 'ω' => out.push('o'),
            // Whitespace & punctuation → handled below
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_slug() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  spaces   collapse  "), "spaces-collapse");
        assert_eq!(slugify("kebab-case-stays"), "kebab-case-stays");
    }

    #[test]
    fn unicode_transliteration() {
        // Cyrillic "Привет" → "privet"
        assert_eq!(slugify("Привет"), "privet");
        // German "Straße" → "strasse"
        assert_eq!(slugify("Straße"), "strasse");
        // "naïve" → "naive"
        assert_eq!(slugify("naïve"), "naive");
    }

    #[test]
    fn strips_symbols() {
        assert_eq!(slugify("What?! (really)"), "what-really");
        assert_eq!(slugify("___"), "");
    }

    #[test]
    fn max_length() {
        let long = "a".repeat(200);
        let s = slugify(&long);
        assert!(s.len() <= MAX_SLUG_LEN);
        assert_eq!(s, "a".repeat(MAX_SLUG_LEN));

        let words = "word ".repeat(50); // 250 chars
        let s = slugify(&words);
        assert!(s.len() <= MAX_SLUG_LEN);
    }

    #[test]
    fn validate_ok() {
        assert!(validate("hello").is_ok());
        assert!(validate("hello-world").is_ok());
        assert!(validate("abc-123").is_ok());
    }

    #[test]
    fn validate_rejects() {
        assert!(validate("").is_err());
        assert!(validate("-leading").is_err());
        assert!(validate("trailing-").is_err());
        assert!(validate("UPPER").is_err());
        assert!(validate("with space").is_err());
        assert!(validate("double--dash").is_err());
    }
}
