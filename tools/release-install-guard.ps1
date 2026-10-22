  # Component receipts belong to Explorer/Python; a full installer must not replace those files.
  $eska_bin_dir = [System.IO.Path]::GetFullPath($dest_dir)
  if ((Split-Path -Path $eska_bin_dir -Leaf) -eq "bin") {
    $eska_components = Join-Path (Split-Path -Path $eska_bin_dir -Parent) "components"
    foreach ($eska_component in @("eska", "eska-project", "eska-check", "eska-build", "eska-vcs")) {
      if (Test-Path -LiteralPath (Join-Path $eska_components "$eska_component.json")) {
        if ($PSUICulture -like "ru*") {
          throw "component-installation-conflict: Компонент $eska_component установлен через Explorer/components.py. Обновите его тем же способом или задайте отдельный каталог ESKA_INSTALL_DIR для полного комплекта."
        }
        throw "component-installation-conflict: $eska_component is managed by Explorer/components.py. Update it with that installer or select a separate ESKA_INSTALL_DIR for the full bundle."
      }
    }
  }
