//! The predefined activities and the colour palette: static data, not a type (design section 2.1). Each
//! predefined id is a v5 UUID of its name under one namespace, so the nine ids are stable and derivable.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::exit
    )
)]

use std::sync::LazyLock;

use uuid::Uuid;

use crate::model::Activity;

/// The namespace the predefined ids are derived in. Fixed; changing it changes every stored `ActivityId`.
pub const NAMESPACE: Uuid = Uuid::from_bytes([0x7a, 0x1e, 0x6d, 0x2c, 0x5b, 0x40, 0x4f, 0x1a, 0x9c, 0x3e, 0x2d, 0x8f, 0x6b, 0x71, 0x05, 0xe4]);

/// The predefined names, in the order `BUILTIN` in `public/app.js` lists them, and the colour it gives each. A new
/// one is added in both places; its id follows from its name, so no stored document changes.
pub const PREDEFINED_NAMES: [(&str, &str); 9] = [
    ("sleep", "#5B3FA8"),
    ("nap", "#A48BE0"),
    ("eat", "#C8643B"),
    ("work", "#C9A227"),
    ("rest", "#6E8F5E"),
    ("game", "#8C3F6E"),
    ("travel", "#4A90C2"),
    ("social", "#E07A9B"),
    ("sport", "#22A699"),
];

/// The six colours a custom activity may take: `CUSTOM_COLORS` in `src/activities.js`.
pub const PALETTE: [&str; 6] = ["#3F7C85", "#B5835A", "#4F6BA8", "#9A6B2F", "#2F7A5C", "#A34A5A"];

/// The icons a custom activity may take: keys of `ICONS` in `public/app.js`, where the drawings live.
pub const ICONS: [&str; 12] = [
    "star", "book", "music", "heart", "code", "cart", "home", "pencil", "leaf", "chat", "camera", "film",
];

/// The icon of an activity created without one, and of every activity stored before icons could be chosen.
pub const DEFAULT_ICON: &str = "star";

/// The most live custom activities a user may hold.
pub const MAX_CUSTOM: usize = 50;

/// The nine predefined activities as `Activity` values.
pub static PREDEFINED: LazyLock<[Activity; 9]> = LazyLock::new(|| PREDEFINED_NAMES.map(|(name, color)| Activity::predefined(predefined_id(name), name, color)));

/// The id of the predefined activity named `name`, whether or not such an activity exists.
pub fn predefined_id(name: &str) -> Uuid {
    Uuid::new_v5(&NAMESPACE, name.as_bytes())
}

/// The predefined activity with this id, if any.
pub fn predefined_by_id(id: Uuid) -> Option<&'static Activity> {
    PREDEFINED.iter().find(|activity| activity.id == id)
}

/// The predefined activity with this name, if any.
pub fn predefined_by_name(name: &str) -> Option<&'static Activity> {
    PREDEFINED.iter().find(|activity| activity.name == name)
}

/// Whether `color` is one of the six palette colours.
pub fn in_palette(color: &str) -> bool {
    PALETTE.contains(&color)
}

/// Whether `icon` is one a custom activity may take.
pub fn is_icon(icon: &str) -> bool {
    ICONS.contains(&icon)
}

/// The palette entry a new activity takes when the body names none: `count mod 6`, where `count` is the
/// user's activities so far, deleted included.
pub fn default_color(count: u64) -> &'static str {
    let index = usize::try_from(count.checked_rem(PALETTE.len() as u64).unwrap_or(0)).unwrap_or(0);
    PALETTE.get(index).copied().unwrap_or(PALETTE[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nine_ids_are_distinct_and_derivable() {
        let ids: Vec<Uuid> = PREDEFINED.iter().map(|a| a.id).collect();
        for (i, id) in ids.iter().enumerate() {
            assert!(!ids.iter().skip(i + 1).any(|other| other == id));
        }
        assert_eq!(predefined_by_name("sleep").map(|a| a.id), Some(predefined_id("sleep")));
        assert_eq!(predefined_by_id(predefined_id("work")).map(|a| a.name.as_str()), Some("work"));
        assert_eq!(predefined_by_name("sport").map(|a| a.color.as_str()), Some("#22A699"));
        assert_eq!(predefined_by_name("sport").map(|a| a.icon.as_str()), Some("sport"));
    }

    #[test]
    fn the_default_icon_is_one_of_the_icons() {
        assert!(is_icon(DEFAULT_ICON));
        assert!(!is_icon("sleep"), "a predefined name is not a custom icon");
        assert!(predefined_by_name("hidden").is_none());
    }

    #[test]
    fn default_color_cycles_the_palette() {
        assert_eq!(default_color(0), PALETTE[0]);
        assert_eq!(default_color(5), PALETTE[5]);
        assert_eq!(default_color(6), PALETTE[0]);
        assert_eq!(default_color(13), PALETTE[1]);
    }
}
