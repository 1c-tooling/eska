    # Component receipts belong to Explorer/Python; a full installer must not replace those files.
    local _eska_bin_dir="$_install_dir"
    if [ -d "$_eska_bin_dir" ]; then
        _eska_bin_dir="$(CDPATH='' cd "$_eska_bin_dir" && pwd -P)"
    fi
    if [ "$(basename "$_eska_bin_dir")" = bin ]; then
        local _eska_component
        for _eska_component in eska eska-project eska-check eska-build eska-vcs eska-fmt; do
            local _eska_receipt="$_eska_bin_dir/../components/$_eska_component.json"
            if [ -e "$_eska_receipt" ] || [ -L "$_eska_receipt" ]; then
                case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
                    ru*) err "component-installation-conflict: Компонент $_eska_component установлен через Explorer/components.py. Обновите его тем же способом или задайте отдельный каталог ESKA_INSTALL_DIR для полного комплекта." ;;
                    *) err "component-installation-conflict: $_eska_component is managed by Explorer/components.py. Update it with that installer or select a separate ESKA_INSTALL_DIR for the full bundle." ;;
                esac
            fi
        done
    fi
