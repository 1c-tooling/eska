//! Generated platform prototypes share native method identities even when the pinned help contains placeholders.

/// Read actual callable spellings from native documentation without replacing signatures or execution contexts.
pub fn methods(prototype: &str) -> Vec<bsl_platform::PlatformMethod> {
    let platform = bsl_platform::PlatformData::instance();
    platform
        .get_type_methods(prototype)
        .into_iter()
        .filter_map(|method| {
            let english = method.english_name.rsplit('.').next()?;
            let docs = platform.get_method_docs(method.id)?;
            let russian = docs.syntax.split_once('(')?.0.trim();
            if russian.is_empty() || russian.contains(['.', '<', '>', '&', ';', ' ']) {
                return None;
            }
            let mut value = method.clone();
            value.name = russian.into();
            value.english_name = english.into();
            Some(value)
        })
        .collect()
}
