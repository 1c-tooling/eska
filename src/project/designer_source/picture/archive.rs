use std::io::Cursor;

use super::{MAX_IMAGE, MAX_XML, Picture, PicturePreview, bounded, image, safe_name};

/// Prefer a current-interface variant, then vector graphics or the largest declared size.
pub(super) fn picture(bytes: &[u8]) -> Result<Picture, PicturePreview> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| PicturePreview::Invalid)?;
    if archive.len() > 512 {
        return Err(PicturePreview::TooLarge);
    }
    let name = match archive.by_name("manifest.xml") {
        Ok(file) => {
            if file.size() > MAX_XML {
                return Err(PicturePreview::TooLarge);
            }
            let bytes = bounded(file, MAX_XML)?;
            variant(&bytes)?
        }
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(_) => return Err(PicturePreview::Invalid),
    };
    let name = name
        .or_else(|| {
            archive
                .file_names()
                .filter(|name| safe_name(name) && supported_name(name))
                .max_by_key(|name| {
                    (
                        std::path::Path::new(name)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg")),
                        name.trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<u32>()
                            .unwrap_or(0),
                        *name,
                    )
                })
                .map(str::to_owned)
        })
        .ok_or(PicturePreview::Unsupported)?;
    let file = archive
        .by_name(&name)
        .map_err(|_| PicturePreview::Invalid)?;
    if file.size() > MAX_IMAGE {
        return Err(PicturePreview::TooLarge);
    }
    image(bounded(file, MAX_IMAGE)?, name)
}

/// A manifest determines the preferred image; invalid references never reach the filesystem.
fn variant(bytes: &[u8]) -> Result<Option<String>, PicturePreview> {
    let input = std::str::from_utf8(bytes).map_err(|_| PicturePreview::Invalid)?;
    let document = roxmltree::Document::parse(input).map_err(|_| PicturePreview::Invalid)?;
    if document.root_element().tag_name().name() != "Picture" {
        return Err(PicturePreview::Invalid);
    }
    Ok(document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("PictureVariant"))
        .filter_map(|node| {
            let name = node.attribute("name")?;
            if !safe_name(name) {
                return None;
            }
            let current = node.attribute("interfaceVariant").is_none_or(str::is_empty);
            let width = node
                .attribute("glyphWidth")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            let height = node
                .attribute("glyphHeight")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            Some((
                (
                    current,
                    std::path::Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg")),
                    u64::from(width) * u64::from(height),
                    name,
                ),
                name,
            ))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, name)| name.to_owned()))
}

/// Restrict fallback discovery to formats that the webview can display as images.
fn supported_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        ["png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "webp"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}
