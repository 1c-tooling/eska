//! Presentation owns translated headings and escapes authored text rather than interpreting it.

use eska::tool_cli::localization::Localizer;

use super::super::markup::escape;
use crate::{HoverKind, SymbolHover};

use super::Format;

/// Typed card data becomes either literal plain text or safely delimited Markdown.
pub(super) fn render(card: &SymbolHover, format: Format, localizer: &Localizer) -> String {
    let heading = localizer.text(kind_key(card.kind));
    let mut parts = vec![match format {
        Format::Plaintext => format!("{} ({heading})", card.symbol),
        Format::Markdown => format!("**{}** ({})", escape(&card.symbol), escape(&heading)),
    }];
    if let Some(declaration) = &card.declaration {
        let label = localizer.text("tool-hover-declaration");
        parts.push(match format {
            Format::Plaintext => format!("{label}:\n{declaration}"),
            Format::Markdown => {
                let fence = "`".repeat(
                    declaration
                        .split(|character| character != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1)
                        .max(3),
                );
                format!(
                    "**{}:**\n{fence}bsl\n{declaration}\n{fence}",
                    escape(&label)
                )
            }
        });
    }
    if let Some(return_type) = &card.return_type {
        let label = localizer.text("tool-hover-return-type");
        parts.push(match format {
            Format::Plaintext => format!("{label}: {return_type}"),
            Format::Markdown => format!("**{}:** {}", escape(&label), escape(return_type)),
        });
    }
    if let Some(documentation) = &card.documentation {
        let label = localizer.text("tool-hover-documentation");
        parts.push(match format {
            Format::Plaintext => format!("{label}:\n{documentation}"),
            Format::Markdown => format!("**{}:**\n{}", escape(&label), escape(documentation)),
        });
    }
    parts.join("\n\n")
}

/// These keys are transport presentation; their native source kind is a stable typed enum.
const fn kind_key(kind: HoverKind) -> &'static str {
    match kind {
        HoverKind::Method => "tool-hover-kind-method",
        HoverKind::ModuleVariable => "tool-hover-kind-module-variable",
        HoverKind::Parameter => "tool-hover-kind-parameter",
        HoverKind::LocalVariable => "tool-hover-kind-local-variable",
        HoverKind::PlatformFunction => "tool-hover-kind-platform-function",
        HoverKind::Module => "tool-hover-kind-module",
        HoverKind::MetadataObject => "tool-hover-kind-metadata-object",
        HoverKind::ManagerModule => "tool-hover-kind-manager-module",
        HoverKind::Field => "tool-hover-kind-field",
    }
}
