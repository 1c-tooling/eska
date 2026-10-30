use std::{borrow::Cow, path::Path};

/// The platform's argument parser rejects Win32 verbatim paths returned by canonicalize.
#[cfg(windows)]
pub fn native_path(path: &Path) -> Cow<'_, Path> {
    #[cfg(windows)]
    {
        use std::{
            ffi::OsString,
            path::{Component, PathBuf, Prefix},
        };
        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            let mut plain = match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:", char::from(drive))),
                Prefix::VerbatimUNC(server, share) => {
                    let mut value = OsString::from(r"\\");
                    value.push(server);
                    value.push(r"\");
                    value.push(share);
                    PathBuf::from(value)
                }
                _ => return Cow::Borrowed(path),
            };
            for component in components {
                plain.push(component.as_os_str());
            }
            return Cow::Owned(plain);
        }
    }
    Cow::Borrowed(path)
}

/// Preserve paths on systems without Win32 verbatim prefixes.
#[cfg(not(windows))]
pub const fn native_path(path: &Path) -> Cow<'_, Path> {
    Cow::Borrowed(path)
}

#[cfg(all(test, windows))]
mod tests {
    use super::native_path;
    use std::{ffi::OsString, path::Path};

    #[test]
    fn platform_arguments_preserve_drive_unc_unicode_and_spaces() {
        for (input, expected) in [
            (r"\\?\C:\Проект 😀\src", r"C:\Проект 😀\src"),
            (r"\\?\UNC\server\share\Проект", r"\\server\share\Проект"),
            (r"C:\Проект\src", r"C:\Проект\src"),
        ] {
            assert_eq!(native_path(Path::new(input)), Path::new(expected));
            let mut option = OsString::from("--data=");
            option.push(native_path(Path::new(input)).as_os_str());
            assert_eq!(option, OsString::from(format!("--data={expected}")));
        }
    }
}
