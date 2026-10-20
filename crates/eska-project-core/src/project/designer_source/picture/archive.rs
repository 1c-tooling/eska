use std::{io::Cursor, path::Path};

use super::{MAX_IMAGE, MAX_XML, Picture, PictureError, bounded, image, safe_name};

const MAX_ENTRIES: usize = 512;

/// Prefer a current-interface variant, then vector graphics or the largest declared size.
pub(super) fn picture(bytes: &[u8]) -> Result<Picture, PictureError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| PictureError::Invalid)?;
    if archive.len() > MAX_ENTRIES {
        return Err(PictureError::TooLarge);
    }
    let declared = match archive.by_name("manifest.xml") {
        Ok(file) => {
            if file.size() > MAX_XML {
                return Err(PictureError::TooLarge);
            }
            Some(variant(&bounded(file, MAX_XML)?)?)
        }
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(_) => return Err(PictureError::Invalid),
    };
    // Filename discovery is only for archives without a manifest, never for broken references.
    let name = declared
        .or_else(|| fallback_name(&archive))
        .ok_or(PictureError::Unsupported)?;
    let file = archive.by_name(&name).map_err(|_| PictureError::Invalid)?;
    if file.size() > MAX_IMAGE {
        return Err(PictureError::TooLarge);
    }
    image(bounded(file, MAX_IMAGE)?, name)
}

/// An authoritative manifest must name at least one safe variant, including extensionless files.
fn variant(bytes: &[u8]) -> Result<String, PictureError> {
    let input = std::str::from_utf8(bytes).map_err(|_| PictureError::Invalid)?;
    let document = roxmltree::Document::parse(input).map_err(|_| PictureError::Invalid)?;
    if !document.root_element().has_tag_name("Picture") {
        return Err(PictureError::Invalid);
    }
    let mut preferred = None;
    for node in document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("PictureVariant"))
    {
        let name = node
            .attribute("name")
            .filter(|name| safe_name(name))
            .ok_or(PictureError::Invalid)?;
        let current = node.attribute("interfaceVariant").is_none_or(str::is_empty);
        let width = dimension(node, "glyphWidth");
        let height = dimension(node, "glyphHeight");
        let rank = (
            current,
            is_svg(name),
            u64::from(width) * u64::from(height),
            name,
        );
        if preferred.is_none_or(|previous| rank > previous) {
            preferred = Some(rank);
        }
    }
    preferred
        .map(|(_, _, _, name)| name.to_owned())
        .ok_or(PictureError::Unsupported)
}

/// Unknown dimensions rank as zero, retaining valid exports with omitted size attributes.
fn dimension(node: roxmltree::Node<'_, '_>, name: &str) -> u32 {
    node.attribute(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Without a manifest, prefer SVG or the largest numeric filename; ties are deterministic.
fn fallback_name(archive: &zip::ZipArchive<Cursor<&[u8]>>) -> Option<String> {
    archive
        .file_names()
        .filter(|name| safe_name(name) && supported_name(name))
        .max_by_key(|name| {
            let density = Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u32>().ok())
                .unwrap_or(0);
            (is_svg(name), density, *name)
        })
        .map(str::to_owned)
}

/// Share case-insensitive vector preference between declared and discovered variants.
fn is_svg(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

/// Restrict fallback discovery to formats that the webview can display as images.
fn supported_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        ["png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "webp"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}
