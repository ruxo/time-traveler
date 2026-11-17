//! An activity on the wire is a string key: a predefined name, or `c:` + the UUID of a stored one.

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

use uuid::Uuid;

use crate::activities;

/// Node's `isUuid`: exactly the 8-4-4-4-12 hexadecimal form, either case.
pub fn parse_uuid(text: &str) -> Option<Uuid> {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    let well_formed = bytes.iter().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => *b == b'-',
        _ => b.is_ascii_hexdigit(),
    });
    if well_formed { Uuid::parse_str(text).ok() } else { None }
}

/// The activity a key names: a predefined name, or `c:` + a UUID. `None` for anything else.
pub fn activity_from_key(key: &str) -> Option<Uuid> {
    if let Some(predefined) = activities::predefined_by_name(key) {
        return Some(predefined.id);
    }
    key.strip_prefix("c:").and_then(parse_uuid)
}

/// The UUID of a stored activity named by `c:` + UUID; `None` for a predefined name or anything else.
pub fn custom_from_key(key: &str) -> Option<Uuid> {
    key.strip_prefix("c:").and_then(parse_uuid)
}

/// The key of an activity id: the predefined name, or `c:` + the lowercase hyphenated UUID.
pub fn key_of(activity: Uuid) -> String {
    match activities::predefined_by_id(activity) {
        Some(predefined) => predefined.name.clone(),
        None => format!("c:{activity}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_convert_both_ways() {
        let sleep = activities::predefined_id("sleep");
        assert_eq!(activity_from_key("sleep"), Some(sleep));
        assert_eq!(key_of(sleep), "sleep");
        let id = Uuid::new_v4();
        assert_eq!(activity_from_key(&format!("c:{id}")), Some(id));
        assert_eq!(activity_from_key(&format!("c:{}", id.as_hyphenated().to_string().to_uppercase())), Some(id));
        assert_eq!(key_of(id), format!("c:{id}"));
        assert_eq!(custom_from_key("sleep"), None);
        assert_eq!(custom_from_key(&format!("c:{id}")), Some(id));
    }

    #[test]
    fn other_forms_are_refused() {
        let id = Uuid::new_v4();
        for bad in [
            "",
            "c:",
            "hidden",
            &id.simple().to_string(),
            &format!("c:{}", id.simple()),
            &format!("c:{{{id}}}"),
            &format!("{id}"),
        ] {
            assert_eq!(activity_from_key(bad), None, "{bad}");
        }
        assert_eq!(parse_uuid(&id.simple().to_string()), None);
        assert_eq!(parse_uuid(&id.to_string()), Some(id));
    }
}
