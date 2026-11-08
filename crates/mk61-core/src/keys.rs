//! Key matrix layout of the MK61 / SK61L.
//!
//! The matrix has 70 positions indexed as `column * 5 + row` (5 rows, 14
//! columns). `NULL` slots in the reference layout are gaps left by wide keys
//! and are reported as `None`.

/// Number of matrix positions.
pub const KEY_POSITIONS: usize = 70;

/// Number of rows in the matrix.
pub const KEY_ROWS: usize = 5;

/// Key name for each matrix position, or `None` for a gap.
pub const KEY_NAMES: [Option<&'static str>; KEY_POSITIONS] = [
    Some("esc"), Some("tab"), Some("caps"), Some("lshift"), Some("lctrl"),
    Some("1"), Some("q"), Some("a"), None, Some("lwin"),
    Some("2"), Some("w"), Some("s"), Some("z"), Some("lalt"),
    Some("3"), Some("e"), Some("d"), Some("x"), None,
    Some("4"), Some("r"), Some("f"), Some("c"), None,
    Some("5"), Some("t"), Some("g"), Some("v"), Some("space"),
    Some("6"), Some("y"), Some("h"), Some("b"), None,
    Some("7"), Some("u"), Some("j"), Some("n"), None,
    Some("8"), Some("i"), Some("k"), Some("m"), None,
    Some("9"), Some("o"), Some("l"), Some("comma"), None,
    Some("0"), Some("p"), Some("semicolon"), Some("period"), Some("ralt"),
    Some("minus"), Some("lbracket"), Some("quote"), Some("slash"), Some("menu"),
    Some("equals"), Some("rbracket"), None, None, Some("rctrl"),
    Some("backspace"), Some("backslash"), Some("enter"), Some("rshift"), Some("fn"),
];

/// Returns the key name for `position`, if any.
#[must_use]
pub fn key_name(position: usize) -> Option<&'static str> {
    KEY_NAMES.get(position).copied().flatten()
}

/// Returns the row of `position` (0..5).
#[must_use]
pub const fn key_row(position: usize) -> usize {
    position % KEY_ROWS
}

/// Returns the column of `position`.
#[must_use]
pub const fn key_column(position: usize) -> usize {
    position / KEY_ROWS
}

/// Looks up a matrix position by key name (case-insensitive).
#[must_use]
pub fn key_position(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    for (index, key) in KEY_NAMES.iter().enumerate() {
        if let Some(k) = key {
            if k.eq_ignore_ascii_case(&lower) {
                return Some(index);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_has_70_positions() {
        assert_eq!(KEY_NAMES.len(), KEY_POSITIONS);
    }

    #[test]
    fn position_round_trips_through_row_and_column() {
        for position in 0..KEY_POSITIONS {
            let row = key_row(position);
            let column = key_column(position);
            assert_eq!(column * KEY_ROWS + row, position);
            assert!(row < KEY_ROWS);
        }
    }

    #[test]
    fn known_keys_resolve() {
        assert_eq!(key_name(0), Some("esc"));
        assert_eq!(key_name(29), Some("space"));
        assert_eq!(key_position("Space"), Some(29));
        assert_eq!(key_position("does-not-exist"), None);
        assert_eq!(key_name(8), None);
    }
}
