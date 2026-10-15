use super::OAuthError;

/// Commas delimit both challenges and parameters; quoted commas delimit neither.
pub(super) fn resource_metadata(text: &str) -> Result<Option<String>, OAuthError> {
    let mut bearer = false;
    let mut location = None;
    for part in parts(text)? {
        let (first, remainder) = token(part).ok_or(OAuthError::Discovery)?;
        let remainder = remainder.trim_start_matches([' ', '\t']);
        let parameter = if remainder.starts_with('=') {
            part
        } else {
            bearer = first.eq_ignore_ascii_case("Bearer");
            remainder
        };
        if bearer && !parameter.is_empty() {
            let (name, value) = parameter_value(parameter)?;
            if name.eq_ignore_ascii_case("resource_metadata") && location.replace(value).is_some() {
                return Err(OAuthError::Discovery);
            }
        }
    }
    Ok(location)
}

fn parts(text: &str) -> Result<Vec<&str>, OAuthError> {
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, byte) in text.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if byte == b',' && !quoted {
            parts.push(text[start..index].trim());
            start = index + 1;
        }
    }
    if quoted || escaped {
        return Err(OAuthError::Discovery);
    }
    parts.push(text[start..].trim());
    // HTTP list syntax permits empty list members between challenges.
    parts.retain(|part| !part.is_empty());
    Ok(parts)
}

fn token(text: &str) -> Option<(&str, &str)> {
    let length = text.bytes().take_while(|byte| token_byte(*byte)).count();
    (length != 0).then(|| text.split_at(length))
}

fn token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn parameter_value(text: &str) -> Result<(&str, String), OAuthError> {
    let (name, tail) = token(text).ok_or(OAuthError::Discovery)?;
    let tail = tail
        .trim_start_matches([' ', '\t'])
        .strip_prefix('=')
        .ok_or(OAuthError::Discovery)?;
    let value = tail.trim_start_matches([' ', '\t']);
    if let Some(quoted) = value.strip_prefix('"') {
        return quoted_value(quoted).map(|value| (name, value));
    }
    let (value, rest) = token(value).ok_or(OAuthError::Discovery)?;
    if !rest.trim().is_empty() {
        return Err(OAuthError::Discovery);
    }
    Ok((name, value.to_owned()))
}

fn quoted_value(text: &str) -> Result<String, OAuthError> {
    let mut value = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => value.push(characters.next().ok_or(OAuthError::Discovery)?),
            '"' => {
                return if characters.as_str().trim().is_empty() {
                    Ok(value)
                } else {
                    Err(OAuthError::Discovery)
                };
            }
            _ => value.push(character),
        }
    }
    Err(OAuthError::Discovery)
}
