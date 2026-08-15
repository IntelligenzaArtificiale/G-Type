use crate::config::Snippet;

pub const MAX_SNIPPETS: usize = 100;
pub const MAX_TRIGGER_CHARS: usize = 100;
pub const MAX_VALUE_CHARS: usize = 4_000;
const MAX_PROMPT_CHARS: usize = 12_000;

pub fn validate(snippets: &[Snippet]) -> Result<(), String> {
    if snippets.len() > MAX_SNIPPETS {
        return Err(format!("Massimo {MAX_SNIPPETS} snippet"));
    }
    let mut seen = std::collections::HashSet::new();
    for snippet in snippets {
        let trigger = snippet.trigger.trim();
        if trigger.is_empty() {
            return Err("La chiave di uno snippet non può essere vuota".into());
        }
        if trigger.chars().count() > MAX_TRIGGER_CHARS {
            return Err(format!(
                "Chiave snippet troppo lunga (max {MAX_TRIGGER_CHARS} caratteri)"
            ));
        }
        if snippet.value.chars().count() > MAX_VALUE_CHARS {
            return Err(format!(
                "Valore snippet troppo lungo (max {MAX_VALUE_CHARS} caratteri)"
            ));
        }
        let normalized = trigger.to_lowercase();
        if !seen.insert(normalized) {
            return Err(format!("Snippet duplicato: {trigger}"));
        }
    }
    Ok(())
}

pub fn prompt_context(snippets: &[Snippet]) -> String {
    let mut out = String::new();
    for snippet in snippets.iter().filter(|snippet| snippet.enabled) {
        if out.chars().count() >= MAX_PROMPT_CHARS {
            break;
        }
        let trigger = sanitize_prompt_value(&snippet.trigger, MAX_TRIGGER_CHARS);
        let value = sanitize_prompt_value(&snippet.value, MAX_VALUE_CHARS);
        if trigger.is_empty() || value.is_empty() {
            continue;
        }
        out.push_str(&format!("- \"{trigger}\" => \"{value}\"\n"));
    }
    out
}

/// Deterministic post-processing for common snippet triggers. We intentionally
/// keep this simple: exact trigger replacement plus ASCII case-insensitive
/// matching. Gemini also receives the snippet list as context, so uncommon
/// Unicode/casing variants still benefit without growing a mini NLP engine.
pub fn apply(text: &str, snippets: &[Snippet]) -> String {
    let mut current = text.to_string();
    for snippet in snippets.iter().filter(|snippet| snippet.enabled) {
        let trigger = snippet.trigger.trim();
        if trigger.is_empty() || snippet.value.is_empty() {
            continue;
        }
        current = replace_ascii_case_insensitive(&current, trigger, &snippet.value);
    }
    current
}

fn replace_ascii_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    if !haystack.is_ascii() || !needle.is_ascii() {
        return haystack.replace(needle, replacement);
    }

    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut cursor = 0usize;
    let mut out = String::with_capacity(haystack.len());
    while let Some(relative) = lower_haystack[cursor..].find(&lower_needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        out.push_str(&haystack[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&haystack[cursor..]);
    out
}

fn sanitize_prompt_value(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control() || *ch == ' ')
        .map(|ch| {
            if ch == '\n' || ch == '\r' || ch == '\t' {
                ' '
            } else {
                ch
            }
        })
        .take(max_chars)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(trigger: &str, value: &str) -> Snippet {
        Snippet {
            trigger: trigger.into(),
            value: value.into(),
            enabled: true,
        }
    }

    #[test]
    fn replaces_ascii_trigger_case_insensitively() {
        let result = apply(
            "Ti mando Link Calendario domani",
            &[snippet("link calendario", "https://cal.test")],
        );
        assert_eq!(result, "Ti mando https://cal.test domani");
    }

    #[test]
    fn disabled_snippet_is_ignored() {
        let mut item = snippet("mia email", "a@example.com");
        item.enabled = false;
        assert_eq!(apply("mia email", &[item]), "mia email");
    }

    #[test]
    fn rejects_duplicates() {
        assert!(validate(&[snippet("Email", "a"), snippet("email", "b")]).is_err());
    }
}
