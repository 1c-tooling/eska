//! Contract tests cover real layered manifests rather than reproducing setter implementations.

use super::{
    Config, IndentStyle, KeywordCase, Language, Layer, Patch, matches, migration, protocol, resolve,
};
use std::path::Path;

/// Additive layout options retain pinned v1 defaults for older pure-text clients.
#[test]
fn old_protocol_options_receive_only_pinned_additive_defaults() {
    let mut value = serde_json::to_value(super::Options::default()).unwrap();
    for key in [
        "condition_items",
        "group_items",
        "order_items",
        "blank_lines_between_queries",
        "align_call_arguments",
        "closing_parenthesis_on_new_line",
        "preserve_existing_line_breaks",
        "indent_multiline_strings",
        "blank_lines_around_multiline_statements",
        "blank_lines_between_object_groups",
        "blank_lines_inside_methods",
        "blank_lines_around_blocks",
        "blank_lines_inside_multistatement_blocks",
        "blank_lines_before_return",
        "separate_object_initialization",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(
        serde_json::from_value::<super::Options>(value).unwrap(),
        super::Options::default()
    );
}

/// Each test parses the same table shape accepted by project and workspace manifests.
fn config(input: &str) -> Config {
    toml::from_str(input).expect("valid format table")
}

/// A source outside any manifest still receives the pinned 1C style without an analyzer.
#[test]
fn default_policy_is_complete_and_language_specific() {
    for language in [Language::Bsl, Language::Sdbl] {
        let policy = resolve(&[], Path::new("module.bsl"), language, &Patch::default()).unwrap();
        assert_eq!(policy.profile, "1c");
        assert_eq!(policy.profile_version, 1);
        assert_eq!(policy.options.indent_style, IndentStyle::Tab);
        assert_eq!(policy.options.indent_width, 4);
        assert_eq!(policy.options.column_limit, 120);
        assert_eq!(
            policy.options.keyword_case,
            if language == Language::Bsl {
                KeywordCase::Canonical
            } else {
                KeywordCase::Upper
            }
        );
        assert!(policy.origins.values().all(|value| value == "profile:1c@1"));
    }
}

/// The immutable ESKA preset changes BSL layout while queries keep the pinned 1C policy.
#[test]
fn eska_profile_is_versioned_language_specific_and_inheritable() {
    let cfg = config("profile='eska'\nprofile_version=1");
    let layers = [Layer::new(Path::new("."), "manifest", &cfg, "").unwrap()];
    let bsl = resolve(
        &layers,
        Path::new("a.bsl"),
        Language::Bsl,
        &Patch::default(),
    )
    .unwrap();
    assert_eq!(bsl.profile, "eska");
    assert_eq!(bsl.profile_version, 1);
    assert!(bsl.options.align_assignments && bsl.options.align_call_arguments);
    assert!(bsl.options.separate_object_initialization);
    assert!(bsl.options.closing_parenthesis_on_new_line);
    assert!(bsl.options.indent_multiline_strings);
    assert_eq!(bsl.options.blank_lines_inside_methods, 1);
    assert_eq!(bsl.options.blank_lines_around_blocks, 1);
    assert_eq!(bsl.options.blank_lines_inside_multistatement_blocks, 1);
    assert_eq!(bsl.options.blank_lines_before_return, 1);
    assert_eq!(bsl.options.blank_lines_around_multiline_statements, 1);
    assert_eq!(bsl.options.blank_lines_between_object_groups, 1);
    for key in [
        "blank_lines_around_blocks",
        "blank_lines_inside_multistatement_blocks",
        "blank_lines_before_return",
        "indent_multiline_strings",
        "blank_lines_around_multiline_statements",
        "blank_lines_between_object_groups",
    ] {
        assert_eq!(bsl.origins[key], "profile:eska@1");
    }
    assert_eq!(bsl.options.wrap_arguments, super::Wrapping::Fit);
    assert_eq!(bsl.options.wrap_binary, super::Wrapping::Fit);
    assert!(bsl.options.preserve_existing_line_breaks);
    assert_eq!(bsl.options.column_limit, 150);
    assert_eq!(bsl.origins["align_call_arguments"], "profile:eska@1");
    assert_eq!(bsl.origins["indent_width"], "profile:1c@1");
    let sdbl = resolve(
        &layers,
        Path::new("a.sdbl"),
        Language::Sdbl,
        &Patch::default(),
    )
    .unwrap();
    assert_eq!(sdbl.options, super::policy::defaults(Language::Sdbl));

    let cfg = config(
        "profile='team'\n[profiles.team]\nbased_on='eska'\n[profiles.team.bsl]\nalign_call_arguments=false\nseparate_object_initialization=false\nblank_lines_inside_methods=0\nblank_lines_around_blocks=0\nblank_lines_inside_multistatement_blocks=0\nblank_lines_before_return=0",
    );
    let inherited = resolve(
        &[Layer::new(Path::new("."), "manifest", &cfg, "").unwrap()],
        Path::new("a.bsl"),
        Language::Bsl,
        &Patch {
            column_limit: Some(90),
            ..Patch::default()
        },
    )
    .unwrap();
    assert_eq!(inherited.profile, "team");
    assert!(!inherited.options.align_call_arguments);
    assert!(!inherited.options.separate_object_initialization);
    assert_eq!(inherited.options.blank_lines_inside_methods, 0);
    assert_eq!(inherited.options.blank_lines_around_blocks, 0);
    assert_eq!(
        inherited.options.blank_lines_inside_multistatement_blocks,
        0
    );
    assert_eq!(inherited.options.blank_lines_before_return, 0);
    assert!(inherited.options.align_assignments);
    assert_eq!(inherited.options.column_limit, 90);
    assert_eq!(inherited.origins["column_limit"], "operation");
    assert_eq!(inherited.origins["align_call_arguments"], "profile:team@1");
    assert_eq!(inherited.origins["align_assignments"], "profile:eska@1");
}

/// Workspace/member rules use their declaring roots; operation values remain highest priority.
#[test]
fn inheritance_paths_false_zero_and_provenance_are_deterministic() {
    let root = config(
        "profile='team'\nexclude=['**/generated/**']\n[profiles.team]\nbased_on='1c'\n[profiles.team.common]\ncolumn_limit=100\n[common]\nspace_after_comma=false\n[[overrides]]\ninclude=['members/**/Module.bsl']\n[overrides.bsl]\ncolumn_limit=90\n",
    );
    let member = config(
        "exclude=[]\n[bsl]\ncolumn_limit=80\n[[overrides]]\ninclude=['src/**']\n[overrides.bsl]\ncolumn_limit=0\n",
    );
    let layers = [
        Layer::new(Path::new("workspace"), "workspace", &root, "").unwrap(),
        Layer::new(Path::new("workspace/members/app"), "member", &member, "").unwrap(),
    ];
    let path = Path::new("workspace/members/app/src/generated/Module.bsl");
    let effective = resolve(&layers, path, Language::Bsl, &Patch::default()).unwrap();
    assert_eq!(effective.options.column_limit, 0);
    assert!(!effective.options.space_after_comma);
    assert!(!effective.excluded);
    assert_eq!(effective.origins["column_limit"], "member:overrides[0]");
    let request = Patch {
        column_limit: Some(120),
        ..Patch::default()
    };
    let effective = resolve(&layers, path, Language::Bsl, &request).unwrap();
    assert_eq!(effective.options.column_limit, 120);
    assert_eq!(effective.origins["column_limit"], "operation");
}

/// Single-star patterns never cross directories and recursive prefixes can match zero levels.
#[test]
fn portable_globs_handle_unicode_and_zero_directory_cases() {
    for (pattern, path, expected) in [
        ("**/generated/**", "generated/a.bsl", true),
        ("src/**/Module.bsl", "src/Module.bsl", true),
        ("src/**/Module.bsl", "src/a/b/Module.bsl", true),
        ("src/*/Module.bsl", "src/a/b/Module.bsl", false),
        ("src/?/Модуль.bsl", "src/Я/Модуль.bsl", true),
        ("**/a/**/b", "a/b", true),
        ("**/a/**/b", "x/a/y/b", true),
        ("a*.bsl", "a/path.bsl", false),
    ] {
        assert_eq!(matches(pattern, path), expected, "{pattern}: {path}");
    }
}

/// Unknown fields and inapplicable language options are errors, even in unused overrides.
#[test]
fn strict_policies_reject_invalid_or_unsupported_options() {
    for input in [
        "unknown=true",
        "profile_version=2",
        "[bsl]\ncolumn_limit=5000",
        "[bsl]\nselect_items='one_per_line'",
        "[common]\nalign_aliases=true",
        "[sdbl]\nblank_lines_between_methods=1",
        "[sdbl]\nalign_call_arguments=false",
        "[common]\nclosing_parenthesis_on_new_line=true",
        "[sdbl]\npreserve_existing_line_breaks=true",
        "[sdbl]\nindent_multiline_strings=true",
        "[common]\nindent_multiline_strings=false",
        "[bsl]\nblank_lines_around_multiline_statements=17",
        "[bsl]\nblank_lines_between_object_groups=17",
        "[sdbl]\nblank_lines_between_object_groups=1",
        "[common]\nblank_lines_around_multiline_statements=1",
        "[bsl]\nblank_lines_inside_methods=17",
        "[bsl]\nblank_lines_around_blocks=17",
        "[bsl]\nblank_lines_inside_multistatement_blocks=17",
        "[bsl]\nblank_lines_before_return=17",
        "[sdbl]\nblank_lines_around_blocks=0",
        "[sdbl]\nblank_lines_inside_multistatement_blocks=1",
        "[common]\nblank_lines_before_return=1",
        "[sdbl]\nseparate_object_initialization=false",
        "[profiles.eska]\nbased_on='1c'",
        "[profiles.1c]\nbased_on='eska'",
        "exclude=['../elsewhere/**']",
        "[bsl]\nindent_style='tabs'",
        "[[overrides]]\ninclude=[]",
    ] {
        assert!(toml::from_str::<Config>(input).is_err(), "{input}");
    }
}

/// Profiles can be declared by ancestors, but selected missing or cyclic names fail explicitly.
#[test]
fn profiles_detect_cycles_and_unknown_bases() {
    for (input, code) in [
        ("profile='missing'", "unknown_profile"),
        ("[profiles.unused]\nbased_on='missing'", "unknown_profile"),
        (
            "profile='a'\n[profiles.a]\nbased_on='b'\n[profiles.b]\nbased_on='a'",
            "profile_cycle",
        ),
    ] {
        let cfg = config(input);
        let layer = Layer::new(Path::new("."), "manifest", &cfg, "").unwrap();
        assert_eq!(
            resolve(
                &[layer],
                Path::new("a.bsl"),
                Language::Bsl,
                &Patch::default()
            )
            .unwrap_err()
            .code,
            code
        );
    }
}

/// Existing explicit settings survive the new default, while inconsistent dual policies fail.
#[test]
fn legacy_indentation_is_not_reinterpreted_as_tab_width() {
    let legacy = "[tools.bsl-analyzer.formatting]\nuse_tabs=false\nindent_size=2\n";
    let cfg = Config::default();
    let layer = Layer::new(Path::new("."), "manifest", &cfg, legacy).unwrap();
    let result = resolve(
        &[layer],
        Path::new("a.bsl"),
        Language::Bsl,
        &Patch::default(),
    )
    .unwrap();
    assert_eq!(result.options.indent_style, IndentStyle::Space);
    assert_eq!(result.options.indent_width, 2);
    let cfg = config("[bsl]\nindent_width=4");
    assert_eq!(
        Layer::new(Path::new("."), "manifest", &cfg, legacy)
            .err()
            .unwrap()
            .code,
        "legacy_conflict"
    );
    assert_eq!(
        migration::preview("[tools.bsl-analyzer.formatting]\nuse_tabs=true\nindent_size=2\n")
            .unwrap_err()
            .code,
        "ambiguous_legacy_indent"
    );
}

/// Migration preserves BOM/CRLF and unrelated options and becomes a no-op on its own result.
#[test]
fn migration_is_explicit_and_idempotent() {
    let before = "\u{feff}# keep\r\n[project]\r\ntype='configuration'\r\n[tools.bsl-analyzer.formatting]\r\nuse_tabs=true\r\nindent_size=1\r\n[tools.bsl-analyzer.diagnostics.parameters]\r\nLineLength=false\r\n";
    let plan = migration::preview(before).unwrap().unwrap();
    assert_eq!(plan.before, before);
    assert!(
        plan.after.starts_with("\u{feff}# keep\r\n"),
        "{:?}",
        plan.after
    );
    assert!(!plan.after.replace("\r\n", "").contains('\n'));
    assert!(plan.after.contains("LineLength=false"));
    assert!(migration::preview(&plan.after).unwrap().is_none());
    let project = crate::config::ProjectConfig::from_toml(&plan.after).unwrap();
    assert_eq!(
        project
            .configuration()
            .formatting_settings()
            .bsl
            .indent_style,
        Some(IndentStyle::Tab)
    );
}

/// The ordinary strict loaders must retain format tables through canonical serialization.
#[test]
fn project_and_workspace_round_trip_format_settings() {
    let project = crate::config::ProjectConfig::from_toml(
        "[project]\ntype='configuration'\n[format.bsl]\ncolumn_limit=100",
    )
    .unwrap();
    assert_eq!(
        crate::config::ProjectConfig::from_toml(&project.to_toml().unwrap()).unwrap(),
        project
    );
    let workspace =
        crate::config::WorkspaceConfig::from_toml("[workspace]\n[format.common]\ncolumn_limit=100")
            .unwrap();
    assert_eq!(
        crate::config::WorkspaceConfig::from_toml(&workspace.to_toml().unwrap()).unwrap(),
        workspace
    );
}

/// Byte ranges inside a Unicode codepoint or a future protocol cannot reach a formatter.
#[test]
fn component_contract_checks_schema_and_utf8_ranges() {
    let mut request = protocol::Request {
        schema_version: 1,
        language: Language::Bsl,
        text: "Я = 1;".into(),
        options: super::Options::default(),
        range: Some(protocol::Range { start: 1, end: 3 }),
    };
    assert_eq!(request.validate().unwrap_err().code, "invalid_range");
    request.range = None;
    request.schema_version = 2;
    assert_eq!(
        request.validate().unwrap_err().code,
        "incompatible_protocol"
    );
    request.schema_version = 1;
    request.validate().unwrap();
    let value = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<protocol::Request>(&value).unwrap(),
        request
    );
}

/// A formatter reply cannot authorize stale text, split UTF-8 codepoints or overlapping native edits.
#[test]
fn component_response_rejects_stale_or_unsafe_coordinate_plans() {
    let request = protocol::Request {
        schema_version: 1,
        language: Language::Bsl,
        text: "Я=1;".into(),
        options: super::Options::default(),
        range: None,
    };
    let mut response = protocol::Response {
        schema_version: 1,
        source_sha256: protocol::source_sha256(&request.text),
        edits: vec![protocol::Edit {
            range: protocol::Range { start: 2, end: 3 },
            text: " = ".into(),
        }],
        formatted_range: protocol::Range {
            start: 0,
            end: request.text.len(),
        },
    };
    response.validate(&request).unwrap();
    response.edits[0].range.start = 1;
    assert_eq!(response.validate(&request).unwrap_err().field, "edits");
    response.edits[0].range.start = 2;
    response.edits.push(response.edits[0].clone());
    assert_eq!(response.validate(&request).unwrap_err().field, "edits");
    response.edits.pop();
    response.source_sha256 = "0".repeat(64);
    assert_eq!(response.validate(&request).unwrap_err().field, "snapshot");
}

/// Filesystem migration owns a backup, preserves bytes and detects intervening edits.
#[test]
fn migration_writes_only_the_reviewed_snapshot_and_keeps_a_recovery_copy() {
    let temporary = crate::test_support::TestDir::new();
    let path = temporary.0.join("eska.toml");
    let before = "[project]\ntype='configuration'\n[tools.bsl-analyzer.formatting]\nuse_tabs=false\nindent_size=2\n";
    std::fs::write(&path, before).unwrap();
    let plan = migration::preview(before).unwrap().unwrap();
    let backup = migration::apply(&path, &plan).unwrap();
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), plan.after);
    assert_eq!(migration::apply(&path, &plan).unwrap(), backup);
    std::fs::write(&path, "# user edit\n").unwrap();
    assert_eq!(
        migration::apply(&path, &plan).unwrap_err().code,
        "source_conflict"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# user edit\n");
    assert_eq!(std::fs::read_dir(&temporary.0).unwrap().count(), 2);
}

/// A stale or foreign backup never becomes permission to replace either file.
#[test]
fn migration_refuses_a_foreign_backup_and_a_forged_plan() {
    let temporary = crate::test_support::TestDir::new();
    let path = temporary.0.join("eska.toml");
    let before = "[tools.bsl-analyzer.formatting]\nuse_tabs=true\nindent_size=1\n";
    std::fs::write(&path, before).unwrap();
    let mut plan = migration::preview(before).unwrap().unwrap();
    let backup = temporary.0.join(format!(
        ".eska-fmt-backup-{}.toml",
        protocol::source_sha256(before)
    ));
    std::fs::write(&backup, "foreign").unwrap();
    assert_eq!(
        migration::apply(&path, &plan).unwrap_err().code,
        "backup_conflict"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    plan.after.push_str("# injected\n");
    assert_eq!(
        migration::apply(&path, &plan).unwrap_err().code,
        "invalid_migration_plan"
    );
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), "foreign");
}

/// The public editor schema must expose every actual option and the query-specific default.
#[test]
fn published_schema_tracks_options_and_language_defaults() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../../schemas/format-v1.schema.json")).unwrap();
    let defaults = serde_json::to_value(super::Options::default()).unwrap();
    let definitions = &schema["$defs"];
    let keys: std::collections::BTreeSet<_> = ["bsl", "sdbl"]
        .into_iter()
        .flat_map(|language| {
            definitions[language]["properties"]
                .as_object()
                .unwrap()
                .keys()
        })
        .collect();
    assert_eq!(keys, defaults.as_object().unwrap().keys().collect());
    assert_eq!(
        definitions["sdbl"]["properties"]["keyword_case"]["default"],
        "upper"
    );
    assert_eq!(
        definitions["bsl"]["properties"]["keyword_case"]["default"],
        "canonical"
    );
}
