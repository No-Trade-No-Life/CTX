pub fn normalize_language_tag(value: &str) -> Option<String> {
    let mut parts = value.trim().split('-');
    let primary = parts.next()?;
    if primary.eq_ignore_ascii_case("und") {
        return parts.next().is_none().then(|| "und".to_owned());
    }
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return None;
    }

    let mut normalized = primary.to_ascii_lowercase();
    for part in parts {
        let component = if part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            part.to_ascii_uppercase()
        } else if part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit()) {
            part.to_owned()
        } else if part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            let lower = part.to_ascii_lowercase();
            format!("{}{}", lower[..1].to_ascii_uppercase(), &lower[1..])
        } else {
            return None;
        };
        normalized.push('-');
        normalized.push_str(&component);
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_language_tag;

    #[test]
    fn normalizes_language_tags_used_by_publication_and_reading() {
        assert_eq!(normalize_language_tag("zh-cn").as_deref(), Some("zh-CN"));
        assert_eq!(normalize_language_tag("en-us").as_deref(), Some("en-US"));
        assert_eq!(normalize_language_tag("ja-JP").as_deref(), Some("ja-JP"));
        assert_eq!(
            normalize_language_tag("zh-hans-cn").as_deref(),
            Some("zh-Hans-CN")
        );
        assert_eq!(normalize_language_tag("und").as_deref(), Some("und"));
    }

    #[test]
    fn rejects_non_bcp_47_language_tags() {
        assert_eq!(normalize_language_tag("English"), None);
        assert_eq!(normalize_language_tag("und-US"), None);
        assert_eq!(normalize_language_tag("en_US"), None);
    }
}
