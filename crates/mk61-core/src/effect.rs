//! Built-in LED effects and their colour-slot metadata.

use crate::Error;

/// Effect id used by the keyboard to store per-key custom layouts.
pub const CUSTOM_EFFECT_ID: u8 = 15;

/// Number of built-in effects supported by the keyboard (indices `0..=12`).
pub const EFFECT_COUNT: usize = 13;

/// Number of colour slots used by the `wave` effect.
pub const WAVE_COLOR_COUNT: usize = 14;

/// A single colour slot block of a built-in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSlots {
    /// First planar offset of the block (R plane start).
    pub offset: usize,
    /// Number of colours stored (R, G and B planes each).
    pub count: usize,
}

impl ColorSlots {
    /// Returns `true` when the effect has colour slots.
    #[must_use]
    pub const fn is_some(self) -> bool {
        self.count > 0
    }
}

/// A built-in hardware LED effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// Effect identifier used in profile byte `3`.
    pub index: u8,
    /// Lowercase effect name used by the CLI and documentation.
    pub name: &'static str,
    /// Human readable description.
    pub description: &'static str,
    /// Colour slots of the effect, if it has any.
    pub colors: Option<ColorSlots>,
}

/// The built-in effects supported by the keyboard firmware.
pub const EFFECTS: [Effect; EFFECT_COUNT] = [
    Effect {
        index: 0,
        name: "wave",
        description: "wave across the matrix",
        colors: Some(ColorSlots {
            offset: 84,
            count: WAVE_COLOR_COUNT,
        }),
    },
    Effect {
        index: 1,
        name: "breath",
        description: "smooth colour pulse",
        colors: Some(ColorSlots {
            offset: 33,
            count: 2,
        }),
    },
    Effect {
        index: 2,
        name: "cycle",
        description: "hue rotation over the matrix",
        colors: None,
    },
    Effect {
        index: 3,
        name: "starlight",
        description: "single colour starlight",
        colors: Some(ColorSlots {
            offset: 39,
            count: 1,
        }),
    },
    Effect {
        index: 4,
        name: "encounter",
        description: "encounter colour pair",
        colors: Some(ColorSlots {
            offset: 42,
            count: 2,
        }),
    },
    Effect {
        index: 5,
        name: "rain",
        description: "rain drop animation",
        colors: Some(ColorSlots {
            offset: 48,
            count: 1,
        }),
    },
    Effect {
        index: 6,
        name: "pulse",
        description: "pulse animation",
        colors: Some(ColorSlots {
            offset: 51,
            count: 2,
        }),
    },
    Effect {
        index: 7,
        name: "cross",
        description: "cross fade",
        colors: Some(ColorSlots {
            offset: 57,
            count: 2,
        }),
    },
    Effect {
        index: 8,
        name: "trigger",
        description: "trigger flash",
        colors: Some(ColorSlots {
            offset: 63,
            count: 2,
        }),
    },
    Effect {
        index: 9,
        name: "ripple",
        description: "ripple propagation",
        colors: Some(ColorSlots {
            offset: 69,
            count: 2,
        }),
    },
    Effect {
        index: 10,
        name: "blossom",
        description: "blossom animation",
        colors: Some(ColorSlots {
            offset: 75,
            count: 2,
        }),
    },
    Effect {
        index: 11,
        name: "game_mode",
        description: "WASD only lighting",
        colors: None,
    },
    Effect {
        index: 12,
        name: "lighting",
        description: "constant single colour",
        colors: Some(ColorSlots {
            offset: 81,
            count: 1,
        }),
    },
];

/// Returns the built-in effect registered at `index`.
#[must_use]
pub fn effect_by_index(index: u8) -> Option<&'static Effect> {
    if index >= EFFECT_COUNT as u8 {
        return None;
    }
    EFFECTS.get(index as usize)
}

/// Normalises an effect name for lookup: lower case, separators removed.
fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

/// Aliases accepted for built-in effect names.
const EFFECT_ALIASES: [(&str, u8); 5] = [
    ("rainbow", 2),
    ("game", 11),
    ("gamemode", 11),
    ("light", 12),
    ("solid", 12),
];

/// Returns the built-in effect whose name matches `name`, case-insensitively.
///
/// Accepted aliases: `wave`, `breath`, `cycle`, `rainbow`, `starlight`,
/// `encounter`, `rain`, `pulse`, `cross`, `trigger`, `ripple`, `blossom`,
/// `game`, `gamemode`, `lighting`, `light`, `solid`, `custom`.
#[must_use]
pub fn effect_by_name(name: &str) -> Option<&'static Effect> {
    let key = normalize(name);
    if key == "custom" || key == "customfx" {
        return Some(&CUSTOM);
    }
    for (alias, index) in EFFECT_ALIASES {
        if normalize(alias) == key {
            if let Some(effect) = effect_by_index(index) {
                return Some(effect);
            }
        }
    }
    EFFECTS.iter().find(|effect| normalize(effect.name) == key)
}

/// The custom per-key LED effect.
#[must_use]
pub fn custom_effect() -> &'static Effect {
    &CUSTOM
}

/// The custom effect descriptor, kept outside the array because it is not part
/// of the built-in set.
pub const CUSTOM: Effect = Effect {
    index: CUSTOM_EFFECT_ID,
    name: "custom",
    description: "user defined per key layout",
    colors: None,
};

/// Resolves an effect from its numeric index or its name.
///
/// # Errors
///
/// Returns [`Error::UnknownEffect`] when neither a built-in effect nor the
/// custom effect matches.
pub fn resolve_effect(spec: &str) -> std::result::Result<&'static Effect, Error> {
    if let Ok(index) = spec.parse::<u8>() {
        if let Some(effect) = effect_by_index(index) {
            return Ok(effect);
        }
        if index == CUSTOM_EFFECT_ID {
            return Ok(&CUSTOM);
        }
        return Err(Error::UnknownEffect);
    }
    effect_by_name(spec).ok_or(Error::UnknownEffect)
}

/// Returns the colour slot block of an effect.
///
/// # Errors
///
/// Returns [`Error::UnknownEffect`] for unknown effects and
/// [`Error::EffectWithoutColors`] for effects without slots.
pub fn effect_color_slots(effect: &Effect) -> std::result::Result<ColorSlots, Error> {
    effect.colors.ok_or(Error::EffectWithoutColors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_effects_cover_the_firmware_range() {
        assert_eq!(EFFECTS.len(), EFFECT_COUNT);
        assert_eq!(EFFECTS[0].index, 0);
        assert_eq!(EFFECTS[12].index, 12);
        assert_eq!(EFFECTS[11].name, "game_mode");
        assert!(EFFECTS[11].colors.is_none());
    }

    #[test]
    fn resolves_built_in_effects_by_index_and_name() {
        assert_eq!(effect_by_index(0).unwrap().name, "wave");
        assert_eq!(effect_by_index(12).unwrap().name, "lighting");
        assert_eq!(effect_by_index(13), None);
        assert_eq!(effect_by_index(255), None);
        assert_eq!(effect_by_index(15), None);

        assert_eq!(effect_by_name("wave").unwrap().index, 0);
        assert_eq!(effect_by_name("BREATH").unwrap().index, 1);
        assert_eq!(effect_by_name("Game-Mode").unwrap().index, 11);
        assert_eq!(effect_by_name("Rainbow").unwrap().index, 2);
        assert_eq!(effect_by_name("Solid").unwrap().index, 12);
    }

    #[test]
    fn resolves_the_custom_effect() {
        assert_eq!(custom_effect().index, CUSTOM_EFFECT_ID);
        assert_eq!(custom_effect().name, "custom");
        assert_eq!(effect_by_name("custom").unwrap().index, CUSTOM_EFFECT_ID);
    }

    #[test]
    fn resolve_rejects_unknown_effects() {
        assert_eq!(resolve_effect("does-not-exist"), Err(Error::UnknownEffect));
        assert_eq!(resolve_effect("15"), Ok(&CUSTOM));
        assert_eq!(resolve_effect("16"), Err(Error::UnknownEffect));
    }

    #[test]
    fn colour_slots_are_exposed_per_effect() {
        assert_eq!(
            effect_color_slots(&EFFECTS[0]).unwrap(),
            ColorSlots {
                offset: 84,
                count: WAVE_COLOR_COUNT
            }
        );
        assert_eq!(
            effect_color_slots(&EFFECTS[2]),
            Err(Error::EffectWithoutColors)
        );
        assert_eq!(
            effect_color_slots(&EFFECTS[11]),
            Err(Error::EffectWithoutColors)
        );
    }
}
