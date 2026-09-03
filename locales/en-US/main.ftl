app-about = Developer tooling for 1C:Enterprise projects
cli-usage = Usage
cli-usage-syntax = eska [OPTIONS] [COMMAND]
cli-options = Options
cli-lang-help = Select the interface language
cli-lang-value = LOCALE
cli-help = Print help
cli-version = Print version
cli-project-dir-help = Search for a project from this directory; base directory for new (defaults to the current directory)
cli-project-dir-value = DIRECTORY
project-not-found = No eska.toml found in { $path } or its parent directories.
project-start-not-directory = The starting path is not a directory: { $path }.
project-config-not-file = The configuration must be a regular file: { $path }.
project-source-not-directory = The source path is not a directory: { $path }.
project-config-invalid = Invalid TOML or configuration structure in { $path }. Check [project].type, preset when [vcs.workflow] is present, and the absence of unknown fields.
project-type-unknown = Unknown project type "{ $value }" in { $path }. Expected: configuration, extension, processing, report.
project-format-unknown = Unknown source format "{ $value }" in { $path }. Expected: designer-xml.
project-path-empty = The source path must not be empty.
project-path-relative-required = The source path must be relative: { $path }.
project-path-absolute-required = The project path must be absolute: { $path }.
project-path-parent-traversal = The path must not contain "..": { $path }.
project-source-outside-root = Sources { $source } are outside the project root { $root }.
project-io-error = Cannot read { $path }: { $reason }.
project-io-not-found = path does not exist
project-io-permission-denied = permission denied
project-io-invalid-data = file is not valid UTF-8 text
project-io-not-directory = a path component is not a directory
project-io-other = filesystem error
cli-commands = Commands
cli-arguments = Arguments
project-workflow-unknown = Unknown workflow "{ $value }" in { $path }. Expected: trunk, git-flow, github-flow, custom.
new-about = Create a new project scaffold
new-usage = eska new [OPTIONS] <DIRECTORY>
new-path-help = New project directory (its parent must exist)
new-type-help = Project type: configuration, extension, processing, report
new-type-value = TYPE
new-workflow-help = Workflow selection: trunk, git-flow, github-flow, custom (only saved for now)
new-workflow-value = WORKFLOW
new-no-vcs-help = Do not initialize Git
new-type-invalid = Unknown project type. Expected: configuration, extension, processing, report.
new-workflow-invalid = Unknown workflow. Expected: trunk, git-flow, github-flow, custom.
new-options-required = Without an interactive terminal, provide both --type and --workflow.
new-type-menu = Select the project type:
    1. Configuration (configuration)
    2. Extension (extension)
    3. External processing (processing)
    4. External report (report)
new-workflow-menu = Select a workflow (only saved for now):
    1. Trunk-based (trunk)
    2. Git Flow (git-flow)
    3. GitHub Flow (github-flow)
    4. Custom (custom)
new-choice-prompt = Enter a number or identifier:
new-choice-invalid = Unknown choice. Please try again.
new-cancelled = Creation cancelled: input ended. No files were created.
new-prompt-error = Could not read the selection or write the prompt. No files were created.
new-created = Project scaffold created: { $path }.
new-destination-invalid = Specify a new project directory without "..": { $path }.
new-destination-exists = The path already exists and will not be changed: { $path }.
new-io-error = Could not create the project or write files at { $path }. Check the parent directory and access permissions.
new-template-error = Could not render the project template.
new-git-error = Could not initialize Git. You can retry with --no-vcs.
new-rollback-error = { $reason } Rollback did not finish; inspect remaining files at { $path }.
