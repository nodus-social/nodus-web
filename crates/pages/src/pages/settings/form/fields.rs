//! One profile metadata row, and the rules the instance applies to the list of them.
//!
//! The rules are duplicated here on purpose: the instance is the authority and answers a
//! `422` with the row it objected to, but a writer should learn that a name is empty
//! before a round trip. The messages carry the row number for the same reason the server's
//! do — a form with six rows needs to say which one it means.

/// How many metadata rows an instance accepts. Six is the fediverse-wide cap, and the
/// editor stops adding at it rather than letting the server reject the seventh.
pub const MAX_FIELDS: usize = 6;

/// How long a row's name may be, matching the instance's own limit.
pub const MAX_NAME_CHARS: usize = 255;

/// One metadata row as the editor and the profile both see it.
///
/// `verified_at` is the instance's answer, not the writer's: a link is verified when the
/// instance has checked it, and the editor only ever displays that verdict.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileField {
    /// The label, for example "Blog".
    pub name: String,
    /// What the label points at: usually a URL, sometimes a handle.
    pub value: String,
    /// When the instance verified the value, in RFC 3339; `None` until it has.
    pub verified_at: Option<String>,
}

/// One row's complaint, by position counted from zero so it indexes the row list.
///
/// The server counts rows from one in its messages; this is the editor's numbering, and
/// the conversion happens where the message is parsed.
pub type FieldError = Option<String>;

/// Every row's complaint, in row order.
///
/// Rules that name no row — too many rows, which the editor prevents — are left to the
/// form-level message, so this only ever answers per-row questions.
#[must_use]
pub fn validate_fields(fields: &[ProfileField]) -> Vec<FieldError> {
    let mut errors: Vec<FieldError> = vec![None; fields.len()];
    let mut seen: Vec<(usize, String)> = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let row = index + 1;
        let name = field.name.trim();
        let value = field.value.trim();
        // A row blank in both members is a row the writer has not filled in yet, and the
        // instance drops it too: complaining would block a save over a row that means
        // nothing.
        if name.is_empty() && value.is_empty() {
            continue;
        }
        if name.is_empty() {
            errors[index] = Some(format!("Field {row} needs a name."));
            continue;
        }
        if name.chars().count() > MAX_NAME_CHARS {
            errors[index] = Some(format!(
                "Field {row}'s name must be at most {MAX_NAME_CHARS} characters."
            ));
            continue;
        }
        if has_markup(name) || has_markup(value) {
            errors[index] = Some(format!("Field {row} must be plain text, without HTML."));
            continue;
        }
        let key = name.to_lowercase();
        match seen.iter().find(|(_, held)| *held == key) {
            Some((first, _)) => {
                errors[index] = Some(format!(
                    "Fields {} and {row} have the same name.",
                    first + 1
                ));
            }
            None => seen.push((index, key)),
        }
    }
    errors
}

/// Whether a row's value looks like a link.
///
/// Only a link can be verified by an instance, so the mark is never drawn beside a
/// sentence or a handle: a verified-looking tick on something the instance cannot check
/// would read as a claim the writer made.
#[must_use]
pub fn is_link(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("https://") || value.starts_with("http://")
}

/// Whether the text carries a tag, which the instance refuses to store as metadata.
///
/// A bare comparison rather than a parser: the instance sanitises with its own HTML
/// pipeline, and all this has to do is catch the case early and say so.
fn has_markup(text: &str) -> bool {
    text.contains('<') || text.contains('>')
}

/// The row a `422` blames, and the sentence to show beside it.
///
/// The number comes back counted from zero so it can index the row list. A sentence that
/// names no row — `a profile has at most 6 fields; 7 were sent` — answers `None` and the
/// caller shows it at form level, where it belongs.
#[must_use]
pub fn parse_field_error(text: &str) -> Option<(usize, String)> {
    let lowered = text.to_lowercase();
    let after = &lowered[lowered.find("profile field")? + "profile field".len()..];
    let mut rows: Vec<usize> = Vec::new();
    for token in after.split_whitespace() {
        // `fields` pluralises the noun, and the duplicate message joins two rows with
        // `and`; every other word ends the leading clause.
        if token == "s" || token == "and" {
            continue;
        }
        let digits: String = token.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            break;
        }
        rows.push(digits.parse().ok()?);
    }
    // Two rows are named when two share a name, and the later one is where a writer can
    // still act on it.
    let row = *rows.last()?;
    (row > 0).then(|| (row - 1, text.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{MAX_NAME_CHARS, ProfileField, parse_field_error, validate_fields};

    fn field(name: &str, value: &str) -> ProfileField {
        ProfileField {
            name: name.to_owned(),
            value: value.to_owned(),
            verified_at: None,
        }
    }

    #[test]
    fn a_server_message_naming_a_row_becomes_that_row() {
        assert_eq!(
            parse_field_error("profile field 2 needs a name"),
            Some((1, "profile field 2 needs a name".to_owned()))
        );
        assert_eq!(
            parse_field_error("profile field 2's name must be at most 255 characters")
                .map(|(row, _)| row),
            Some(1)
        );
        assert_eq!(
            parse_field_error("profile field 2 must be plain text, without HTML")
                .map(|(row, _)| row),
            Some(1)
        );
    }

    #[test]
    fn a_message_naming_two_rows_blames_the_later_one() {
        assert_eq!(
            parse_field_error("profile fields 1 and 2 are both named “Blog”").map(|(row, _)| row),
            Some(1)
        );
    }

    #[test]
    fn a_message_naming_no_row_stays_at_form_level() {
        assert_eq!(
            parse_field_error("a profile has at most 6 fields; 7 were sent"),
            None
        );
        assert_eq!(parse_field_error("this instance is read-only"), None);
    }

    #[test]
    fn a_blank_row_is_not_a_complaint() {
        let fields = [field("", ""), field("Blog", "https://example.test")];
        assert_eq!(validate_fields(&fields), vec![None, None]);
    }

    #[test]
    fn a_row_with_a_value_needs_a_name() {
        let fields = [field("", "https://example.test")];
        assert_eq!(
            validate_fields(&fields),
            vec![Some("Field 1 needs a name.".to_owned())]
        );
    }

    #[test]
    fn a_long_name_is_refused_with_its_row() {
        let fields = [field(&"a".repeat(MAX_NAME_CHARS + 1), "value")];
        assert_eq!(
            validate_fields(&fields),
            vec![Some(
                "Field 1's name must be at most 255 characters.".to_owned()
            )]
        );
    }

    #[test]
    fn markup_is_refused_on_either_member() {
        let fields = [field("Blog <b>", "x"), field("Site", "<script>")];
        assert_eq!(
            validate_fields(&fields),
            vec![
                Some("Field 1 must be plain text, without HTML.".to_owned()),
                Some("Field 2 must be plain text, without HTML.".to_owned()),
            ]
        );
    }

    #[test]
    fn duplicate_names_compare_trimmed_and_case_insensitively() {
        let fields = [
            field(" Blog ", "https://a.test"),
            field("blog", "https://b.test"),
        ];
        assert_eq!(
            validate_fields(&fields),
            vec![None, Some("Fields 1 and 2 have the same name.".to_owned())]
        );
    }
}
