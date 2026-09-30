use std::io::{Cursor, Write};

use eska::project::designer_source::{Picture, PicturePreview};

use super::{DesignerSource, MetadataKind, ObjectId, TestDir, fixture, id, write, xml};

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="red"/></svg>"#;

/// Build only the files owned by one common picture.
fn picture_fixture(name: &str, payload: &[u8]) -> (TestDir, DesignerSource, ObjectId) {
    let (directory, source) = fixture("configuration", "Configuration.xml", "Configuration", "");
    write(
        source.project().source(),
        "CommonPictures/Icon.xml",
        xml("CommonPicture", "Icon", ""),
    );
    reference(&source, name);
    write(
        source.project().source(),
        &format!("CommonPictures/Icon/Ext/Picture/{name}"),
        payload,
    );
    (
        directory,
        source,
        id(MetadataKind::CommonPicture, "Icon", None),
    )
}

/// Change only the descriptor's payload reference for boundary cases.
fn reference(source: &DesignerSource, name: &str) {
    write(
        source.project().source(),
        "CommonPictures/Icon/Ext/Picture.xml",
        format!(
            "<ExtPicture xmlns=\"http://v8.1c.ru/8.3/xcf/extrnprops\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\"><Picture><xr:Abs>{name}</xr:Abs></Picture></ExtPicture>"
        ),
    );
}

/// Archives exercise real DEFLATE decoding without committing generated binary fixtures.
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Return decoded bytes while preserving useful assertion diagnostics.
fn ready(source: &DesignerSource, id: &ObjectId) -> Picture {
    let preview = source.picture_preview(id);
    let PicturePreview::Ready(picture) = preview else {
        panic!("{preview:?}")
    };
    picture
}

/// MIME comes from contents; rereading a picture does not retain stale binary data.
#[test]
fn reads_current_picture_contents_and_distinguishes_missing_from_unsupported() {
    let (_directory, source, id) = picture_fixture("wrong.png", SVG);
    let picture = ready(&source, &id);
    assert_eq!(picture.mime_type, "image/svg+xml");
    assert_eq!(picture.bytes, SVG);
    write(
        source.project().source(),
        "CommonPictures/Icon/Ext/Picture/wrong.png",
        b"not an image",
    );
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Unsupported
    ));
    reference(&source, "missing.png");
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Missing
    ));
    reference(&source, "../Configuration.xml");
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Invalid
    ));
    reference(&source, "C:\\picture.png");
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Invalid
    ));
}

/// A larger legacy-interface variant must not replace the modern-interface picture.
#[test]
fn zip_selects_largest_current_variant_and_never_extracts_files() {
    let manifest = br#"<Picture><PictureVariant name="old.png" interfaceVariant="version8_2" glyphWidth="256" glyphHeight="256"/><PictureVariant name="100.png" glyphWidth="16" glyphHeight="16"/><PictureVariant name="400.png" glyphWidth="64" glyphHeight="64"/></Picture>"#;
    let bytes = archive(&[
        ("old.png", b"old"),
        ("100.png", b"small"),
        ("400.png", SVG),
        ("manifest.xml", manifest),
    ]);
    let (_directory, source, id) = picture_fixture("Picture.zip", &bytes);
    let picture = ready(&source, &id);
    assert_eq!(picture.file_name, "400.png");
    assert_eq!(picture.bytes, SVG);
    assert_eq!(
        std::fs::read_dir(
            source
                .project()
                .source()
                .join("CommonPictures/Icon/Ext/Picture")
        )
        .unwrap()
        .count(),
        1
    );
}

/// Uncompressed entry limits apply even when the archive itself is tiny.
#[test]
fn rejects_oversized_compressed_and_corrupt_pictures() {
    let bytes = archive(&[("Picture.svg", &vec![b' '; 8 * 1024 * 1024 + 1])]);
    let (_directory, source, id) = picture_fixture("Picture.zip", &bytes);
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::TooLarge
    ));
    write(
        source.project().source(),
        "CommonPictures/Icon/Ext/Picture/Picture.zip",
        b"PK\x03\x04broken",
    );
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Invalid
    ));
}

/// A payload symlink cannot expose files outside the configured source directory.
#[cfg(unix)]
#[test]
fn rejects_payload_symlink_escape() {
    let (directory, source, id) = picture_fixture("Picture.svg", SVG);
    write(&directory.0, "outside.svg", SVG);
    let payload = source
        .project()
        .source()
        .join("CommonPictures/Icon/Ext/Picture/Picture.svg");
    std::fs::remove_file(&payload).unwrap();
    std::os::unix::fs::symlink(directory.0.join("outside.svg"), payload).unwrap();
    assert!(matches!(
        source.picture_preview(&id),
        PicturePreview::Unavailable
    ));
}

/// Designer manifests may refer to extensionless files; their bytes define the MIME type.
#[test]
fn zip_manifest_supports_extensionless_picture_names() {
    let bytes = archive(&[
        ("l", SVG),
        (
            "manifest.xml",
            br#"<Picture><PictureVariant name="l" interfaceVariant=""/></Picture>"#,
        ),
    ]);
    let (_directory, source, id) = picture_fixture("Picture.zip", &bytes);
    let picture = ready(&source, &id);
    assert_eq!(picture.mime_type, "image/svg+xml");
    assert_eq!(picture.file_name, "l");
}
