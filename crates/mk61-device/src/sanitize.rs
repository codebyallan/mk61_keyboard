//! Sanitisation of untrusted USB and sysfs strings.
//!
//! USB strings are supplied by the device and are therefore untrusted. They are
//! decoded lossily as UTF-8, stripped of terminal escape sequences and control
//! characters, and truncated before any consumer can print them. This prevents a
//! malformed or malicious device from injecting ANSI sequences into a terminal
//! or a graphical log view.

use std::iter::Peekable;
use std::str::Chars;

/// Maximum number of printable characters kept from a USB string.
pub const MAX_USB_STRING_CHARS: usize = 64;

/// Converts untrusted bytes into a safe, printable, truncated string.
///
/// Invalid UTF-8 is replaced with `U+FFFD`. ANSI/ECMA-48 escape sequences,
/// control characters and bidirectional formatting controls are removed. The
/// result is trimmed and contains at most [`MAX_USB_STRING_CHARS`] characters.
pub fn sanitize_usb_string(raw: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(raw);
    let stripped = strip_terminal_sequences(&decoded);
    let mut output = String::with_capacity(stripped.len().min(MAX_USB_STRING_CHARS * 4));
    let mut kept = 0;

    for character in stripped.chars() {
        if character.is_control() || is_bidi_control(character) {
            continue;
        }
        output.push(character);
        kept += 1;
        if kept == MAX_USB_STRING_CHARS {
            break;
        }
    }

    output.trim().to_owned()
}

fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn strip_terminal_sequences(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\u{001B}' => skip_escape_sequence(&mut characters),
            '\u{009B}' => skip_csi(&mut characters),
            '\u{009D}' => skip_string_sequence(&mut characters),
            _ => output.push(character),
        }
    }

    output
}

fn skip_escape_sequence(characters: &mut Peekable<Chars<'_>>) {
    match characters.peek().copied() {
        Some('[') => {
            characters.next();
            skip_csi(characters);
        }
        Some(']') => {
            characters.next();
            skip_string_sequence(characters);
        }
        Some('P' | 'X' | '^' | '_') => {
            characters.next();
            skip_string_sequence(characters);
        }
        Some(_) => {
            // Generic escape: optional intermediate bytes, then one final byte.
            while let Some(&next) = characters.peek() {
                let code = u32::from(next);
                if (0x20..=0x2F).contains(&code) {
                    characters.next();
                } else {
                    break;
                }
            }
            if let Some(&next) = characters.peek() {
                let code = u32::from(next);
                if (0x30..=0x7E).contains(&code) {
                    characters.next();
                }
            }
        }
        None => {}
    }
}

fn skip_csi(characters: &mut Peekable<Chars<'_>>) {
    for character in characters.by_ref() {
        let code = u32::from(character);
        if (0x40..=0x7E).contains(&code) {
            break;
        }
    }
}

fn skip_string_sequence(characters: &mut Peekable<Chars<'_>>) {
    while let Some(character) = characters.next() {
        if character == '\u{0007}' || character == '\u{009C}' {
            break;
        }
        if character == '\u{001B}' && characters.peek() == Some(&'\\') {
            characters.next();
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_product_names() {
        assert_eq!(sanitize_usb_string(b"SK61L SK61L"), "SK61L SK61L");
    }

    #[test]
    fn strips_ansi_csi_sequences() {
        assert_eq!(
            sanitize_usb_string(b"SK61L\x1b[31m EVIL\x1b[0m"),
            "SK61L EVIL"
        );
    }

    #[test]
    fn strips_osc_and_generic_escape_sequences() {
        assert_eq!(
            sanitize_usb_string(b"before\x1b]0;title\x07after"),
            "beforeafter"
        );
        assert_eq!(sanitize_usb_string(b"a\x1b(Bb"), "ab");
        assert_eq!(sanitize_usb_string("a\u{009b}31mb".as_bytes()), "ab");
    }

    #[test]
    fn strips_control_and_bidi_characters() {
        assert_eq!(sanitize_usb_string(b"a\x00b\x07c\x7fd"), "abcd");
        assert_eq!(sanitize_usb_string("\u{202E}evil".as_bytes()), "evil");
    }

    #[test]
    fn replaces_invalid_utf8_lossily() {
        assert_eq!(sanitize_usb_string(b"SK61\xFFL"), "SK61\u{FFFD}L");
    }

    #[test]
    fn truncates_to_the_documented_limit() {
        let input = vec![b'a'; MAX_USB_STRING_CHARS + 20];
        assert_eq!(sanitize_usb_string(&input).chars().count(), 64);
    }

    #[test]
    fn trims_surrounding_whitespace_and_returns_empty_for_controls() {
        assert_eq!(sanitize_usb_string(b"  SK61L  "), "SK61L");
        assert_eq!(sanitize_usb_string(b"\x1b[31m\x07"), "");
    }
}
