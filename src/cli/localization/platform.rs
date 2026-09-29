//! Presentation-only captions for the open-ended Designer XML vocabulary.

use crate::project::metadata_model::{MetadataKind, PropertyKey};

use super::Localizer;

impl Localizer {
    /// Resolve contextual captions first, retaining `QName` boundaries for unknown extensions.
    pub(crate) fn property_caption(
        &self,
        owner: Option<MetadataKind>,
        key: &PropertyKey,
    ) -> Option<String> {
        // Catalog names are ASCII identifiers; a hyphenated XML name must not impersonate
        // the owner segment of a contextual Fluent key.
        if !key.name.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return None;
        }
        let namespace = match key.namespace.as_deref()? {
            "http://v8.1c.ru/8.3/MDClasses" => "md",
            "http://v8.1c.ru/8.3/xcf/readable" => "readable",
            "http://v8.1c.ru/8.1/data/core" => "core",
            "http://v8.1c.ru/8.2/managed-application/core" => "app",
            "http://v8.1c.ru/8.3/xcf/predef" => "predef",
            _ => return None,
        };
        if let Some(owner) = owner {
            let contextual = format!(
                "platform-property-{namespace}-{}-{}",
                owner.as_str(),
                key.name
            );
            if let Some(caption) = self.optional_text(&contextual) {
                return Some(caption);
            }
        }
        self.optional_text(&format!("platform-property-{namespace}-{}", key.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::localization::Locale;

    /// Every audited `QName` and owner resolves in both locales, including newer additions.
    #[test]
    fn catalog_covers_the_audited_vocabulary() {
        let catalog = include_str!("../../../locales/platform-catalog.tsv");
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).unwrap();
            for line in catalog.lines().skip(1) {
                let columns: Vec<_> = line.split('\t').collect();
                assert_eq!(columns.len(), 5);
                let owner = MetadataKind::ALL
                    .iter()
                    .copied()
                    .find(|kind| kind.as_str() == columns[3]);
                assert!(columns[3].is_empty() || owner.is_some());
                let caption = localizer.property_caption(
                    owner,
                    &PropertyKey {
                        namespace: Some(columns[1].into()),
                        name: columns[2].into(),
                    },
                );
                assert_eq!(caption, localizer.optional_text(columns[0]), "{line}");
                assert!(caption.is_some_and(|text| !text.trim().is_empty()));
            }
            for line in include_str!("../../../locales/platform-coverage.tsv")
                .lines()
                .skip(1)
            {
                let columns: Vec<_> = line.split('\t').collect();
                assert_eq!(columns.len(), 6);
                assert!(localizer.optional_text(columns[5]).is_some(), "{line}");
            }
        }
    }

    /// Unknown namespaces and fields cannot accidentally borrow a familiar-looking caption.
    #[test]
    fn captions_respect_namespaces_and_owner_context() {
        let localizer = Localizer::try_new(Locale::RuRu).unwrap();
        let mut key = PropertyKey {
            namespace: Some("http://v8.1c.ru/8.3/MDClasses".into()),
            name: "Type".into(),
        };
        assert_eq!(
            localizer.property_caption(None, &key).as_deref(),
            Some("Тип")
        );
        assert_eq!(
            localizer
                .property_caption(Some(MetadataKind::StyleItem), &key)
                .as_deref(),
            Some("Вид")
        );
        key.namespace = Some("urn:vendor".into());
        assert_eq!(
            localizer.property_caption(Some(MetadataKind::StyleItem), &key),
            None
        );
        key.namespace = None;
        assert_eq!(localizer.property_caption(None, &key), None);
        key.namespace = Some("http://v8.1c.ru/8.3/MDClasses".into());
        key.name = "FutureProperty".into();
        assert_eq!(localizer.property_caption(None, &key), None);
        key.name = "sequence-RegisterRecords".into();
        assert_eq!(localizer.property_caption(None, &key), None);
    }
}
