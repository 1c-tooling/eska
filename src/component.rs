//! Legacy command aliases and localized presentation around the shared process client.

use crate::cli::localization::Localizer;
use eska_presentation::components::present;
pub use eska_tool_runtime::Component;
use eska_tool_runtime::{delegate, resolve};
use std::{
    ffi::OsString,
    process::{ExitCode, ExitStatus},
};

#[derive(Clone, Copy)]
pub enum Invocation {
    Direct,
    Check,
    Fmt,
    LegacyIde,
    LegacyValidate,
    LegacyNew,
    LegacyInit,
    Build { provenance: bool },
}

/// Preserve original command arguments while letting the component own its standard streams.
pub fn run(
    component: Component,
    args: &[OsString],
    invocation: Invocation,
    localizer: &Localizer,
) -> ExitCode {
    let mut args = forward_args(args, invocation);
    if matches!(invocation, Invocation::Build { provenance: true }) {
        let vcs = match resolve(Component::Vcs) {
            Ok(vcs) => vcs,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        args.splice(0..0, [OsString::from("--vcs-binary"), vcs.into_os_string()]);
    }
    match delegate(component, &args) {
        Ok(status) => status_code(status),
        Err(error) => {
            eprintln!("{}", present(&error, localizer));
            ExitCode::FAILURE
        }
    }
}

/// Find the actual command after global option values, preserving native argument bytes.
fn command_position(args: &[OsString]) -> Option<usize> {
    let mut position = 0;
    while let Some(argument) = args.get(position) {
        if argument == "--lang" || argument == "--project-dir" {
            position += 2;
        } else if argument == "--" {
            return args.get(position + 1).map(|_| position + 1);
        } else if argument.to_string_lossy().starts_with('-') {
            position += 1;
        } else {
            return Some(position);
        }
    }
    None
}

/// Adapt only legacy entry names; every other argument and standard stream passes through intact.
fn forward_args(args: &[OsString], invocation: Invocation) -> Vec<OsString> {
    let mut args = args.to_vec();
    match invocation {
        Invocation::Direct | Invocation::Build { .. } => {}
        Invocation::LegacyNew | Invocation::LegacyInit => {
            if let Some(position) = command_position(&args) {
                args[position] = OsString::from(if matches!(invocation, Invocation::LegacyNew) {
                    "__shell-new"
                } else {
                    "__shell-init"
                });
            }
        }
        Invocation::LegacyIde => {
            if let Some(position) = command_position(&args) {
                args[position] = OsString::from("serve");
                args.insert(position + 1, OsString::from("--legacy-identity"));
            }
        }
        Invocation::Check | Invocation::Fmt => {
            if let Some(position) = command_position(&args) {
                args.remove(position);
            }
        }
        Invocation::LegacyValidate => {
            let position = args
                .iter()
                .position(|arg| arg == "--")
                .unwrap_or(args.len());
            args.insert(position, OsString::from("__shell-validate"));
        }
    }
    args
}

/// Preserve child exit codes and represent Unix signal termination as the shell's 128+signal code.
fn status_code(status: ExitStatus) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitCode::from(
            u8::try_from(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
            )
            .unwrap_or(1),
        )
    }
    #[cfg(windows)]
    {
        let code = status.code().unwrap_or(1);
        match u8::try_from(code) {
            Ok(code) => ExitCode::from(code),
            // Stable ExitCode exposes only u8; exit preserves a full Windows child status.
            Err(_) => std::process::exit(code),
        }
    }
}
