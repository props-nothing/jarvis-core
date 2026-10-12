//! Phone numbers, as far as the platform cares: one canonical written form, and a form safe to show.
//!
//! The number decides who is rung, so it is read strictly. Everything that stores or compares a number goes through
//! [`normalize_phone_number`] first, which is what makes "the number the owner approved" and "the number being called" the same string.

/// Reads a phone number into international format (`+` and 8 to 15 digits, no leading zero), tolerating the spaces, dashes, dots and
/// brackets people type. `00` in place of `+` is accepted. Anything else is refused.
///
/// # Errors
///
/// Returns a sentence saying what is wrong with the number.
pub fn normalize_phone_number(text: &str) -> Result<String, &'static str> {
    let trimmed = text.trim();
    let digits_part = trimmed
        .strip_prefix('+')
        .or_else(|| trimmed.strip_prefix("00"))
        .ok_or("a phone number must start with + and the country code, such as +31612345678")?;
    let mut digits = String::with_capacity(digits_part.len());
    for character in digits_part.chars() {
        match character {
            '0'..='9' => digits.push(character),
            ' ' | '-' | '.' | '(' | ')' => {}
            _ => return Err("a phone number has only digits after the +"),
        }
    }
    if !(8..=15).contains(&digits.len()) || digits.starts_with('0') {
        return Err("a phone number has 8 to 15 digits after the + and country code");
    }
    Ok(format!("+{digits}"))
}

/// Whether a number is in a range that charges the caller per minute or per call (premium rate, mass-traffic and similar), for the
/// countries below. **Best effort, and not a complete list**: it exists so that a model steered by hostile text cannot cheaply run up the
/// owner's phone bill with a number it was handed, not to classify every number in the world.
#[must_use]
pub fn is_premium_rate(number: &str) -> bool {
    const PREFIXES: [&str; 13] = [
        "+31900", "+31906", "+31909", // Netherlands 0900, 0906, 0909
        "+32900", "+32902", "+32904", // Belgium 0900, 0902, 0904
        "+449",   // United Kingdom 09
        "+1900",  // United States and Canada 900
        "+49900", "+49137", // Germany 0900, 0137
        "+33899", "+33891", // France 0899, 0891
        "+882",   // international networks
    ];
    PREFIXES.iter().any(|prefix| number.starts_with(prefix))
}

/// A number with the middle hidden, for a log line, a card or a record shown on screen: `+31••••••678`.
#[must_use]
pub fn mask_phone_number(number: &str) -> String {
    let chars: Vec<char> = number.chars().collect();
    if chars.len() <= 7 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 3..].iter().collect();
    format!("{head}{}{tail}", "•".repeat(chars.len() - 6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_into_international_format_or_refused() {
        assert_eq!(
            normalize_phone_number(" +31 (6) 1234-5678 ").as_deref(),
            Ok("+31612345678")
        );
        assert_eq!(
            normalize_phone_number("0031612345678").as_deref(),
            Ok("+31612345678")
        );
        for bad in [
            "0612345678",
            "+0612345678",
            "+31",
            "+3161234567890123",
            "+31abc12345678",
            "",
            "tel:+31612345678",
        ] {
            assert!(
                normalize_phone_number(bad).is_err(),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn premium_rate_ranges_are_recognised_and_ordinary_numbers_are_not() {
        for premium in [
            "+31900123456",
            "+449091234567",
            "+19005551234",
            "+4990012345",
            "+3389912345",
        ] {
            assert!(is_premium_rate(premium), "{premium}");
        }
        for ordinary in [
            "+31612345678",
            "+442071234567",
            "+12025550101",
            "+4915112345678",
        ] {
            assert!(!is_premium_rate(ordinary), "{ordinary}");
        }
    }

    #[test]
    fn a_masked_number_hides_the_middle() {
        assert_eq!(mask_phone_number("+31612345678"), "+31••••••678");
        assert!(!mask_phone_number("+31612345678").contains("1234"));
        assert_eq!(mask_phone_number("+1"), "••");
    }
}
