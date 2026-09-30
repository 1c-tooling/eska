//! Bounded common-picture reads. Archives are inspected in memory, never extracted.

use std::{fs::File, io::Read, path::Path};

use super::DesignerSource;

mod archive;

const MAX_IMAGE: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 32 * 1024 * 1024;
const MAX_XML: u64 = 64 * 1024;
const EXTERNAL: &str = "http://v8.1c.ru/8.3/xcf/extrnprops";
const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

/// A browser-displayable image, with its original bytes and a diagnostic filename.
#[derive(Debug)]
pub struct Picture {
    pub mime_type: &'static str,
    pub bytes: Vec<u8>,
    pub file_name: String,
}

/// Preview failures are independent of the metadata properties themselves.
#[derive(Debug)]
pub enum PicturePreview {
    Ready(Picture),
    Missing,
    Unsupported,
    Invalid,
    TooLarge,
    Unavailable,
}

/// Read only the payload explicitly referenced by the external-property descriptor.
pub(super) fn read(source: &DesignerSource, descriptor: &Path) -> PicturePreview {
    resolve(source, descriptor).unwrap_or_else(|status| status)
}

/// Resolve all workspace paths through the existing source-containment boundary.
fn resolve(source: &DesignerSource, descriptor: &Path) -> Result<PicturePreview, PicturePreview> {
    let ext = descriptor.with_extension("").join("Ext");
    let xml = read_file(source, &ext.join("Picture.xml"), MAX_XML)?;
    let input = std::str::from_utf8(&xml).map_err(|_| PicturePreview::Invalid)?;
    let document = roxmltree::Document::parse(input).map_err(|_| PicturePreview::Invalid)?;
    let root = document.root_element();
    if !root.has_tag_name((EXTERNAL, "ExtPicture")) {
        return Err(PicturePreview::Invalid);
    }
    let name = root
        .children()
        .find(|node| node.has_tag_name((EXTERNAL, "Picture")))
        .and_then(|node| {
            node.children()
                .find(|node| node.has_tag_name((READABLE, "Abs")))
        })
        .and_then(|node| node.text())
        .ok_or(PicturePreview::Missing)?
        .trim();
    if !safe_name(name) {
        return Err(PicturePreview::Invalid);
    }
    let bytes = read_file(source, &ext.join("Picture").join(name), MAX_ARCHIVE)?;
    let picture = if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        archive::picture(&bytes)?
    } else {
        image(bytes, name.to_owned())?
    };
    Ok(PicturePreview::Ready(picture))
}

/// External picture references and manifest variants are local filenames, never paths.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

/// Limit both the declared file length and the actual read to tolerate concurrent changes.
fn read_file(
    source: &DesignerSource,
    relative: &Path,
    limit: u64,
) -> Result<Vec<u8>, PicturePreview> {
    let path = source
        .checked_file(relative)
        .map_err(|_| PicturePreview::Unavailable)?
        .ok_or(PicturePreview::Missing)?;
    let file = File::open(path).map_err(|_| PicturePreview::Unavailable)?;
    if file
        .metadata()
        .map_err(|_| PicturePreview::Unavailable)?
        .len()
        > limit
    {
        return Err(PicturePreview::TooLarge);
    }
    bounded(file, limit)
}

/// Bound decompression as well as ordinary reads; never trust a ZIP size alone.
fn bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, PicturePreview> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PicturePreview::Invalid)?;
    if bytes.len() as u64 > limit {
        return Err(PicturePreview::TooLarge);
    }
    Ok(bytes)
}

/// Detect display formats from contents, not a user-controlled extension.
fn image(bytes: Vec<u8>, file_name: String) -> Result<Picture, PicturePreview> {
    if bytes.len() as u64 > MAX_IMAGE {
        return Err(PicturePreview::TooLarge);
    }
    let mime_type = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.starts_with(b"\0\0\x01\0") {
        "image/x-icon"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        let input = std::str::from_utf8(&bytes).map_err(|_| PicturePreview::Unsupported)?;
        let document =
            roxmltree::Document::parse(input).map_err(|_| PicturePreview::Unsupported)?;
        if !document
            .root_element()
            .has_tag_name(("http://www.w3.org/2000/svg", "svg"))
        {
            return Err(PicturePreview::Unsupported);
        }
        "image/svg+xml"
    };
    Ok(Picture {
        mime_type,
        bytes,
        file_name,
    })
}
