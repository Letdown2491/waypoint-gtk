#!/usr/bin/env bash
set -euo pipefail

ACTION=${1:-install}
PROJECT_ROOT=$(cd -- "$(dirname -- "$0")" && pwd)
INSTALL_PREFIX="/usr"
BINDIR="${INSTALL_PREFIX}/bin"
DATADIR="${INSTALL_PREFIX}/share"

require_sudo() {
    sudo -v
}

install_dependencies() {
    local deps=(
        rustup
        gtk4-devel
        libadwaita-devel
        glib-devel
        pango-devel
        pkg-config
        polkit-devel
        dbus-devel
    )

    if ! command -v xbps-query >/dev/null 2>&1; then
        echo "xbps-query not found; installing full dependency set"
        sudo xbps-install -S "${deps[@]}"
        return
    fi

    local missing=()
    echo "Checking build dependencies..."
    for dep in "${deps[@]}"; do
        if xbps-query -p pkgver "$dep" >/dev/null 2>&1; then
            echo " ✓ present: $dep"
        else
            echo " ✗ missing: $dep"
            missing+=("$dep")
        fi
    done

    if (( ${#missing[@]} > 0 )); then
        echo "Installing missing packages: ${missing[*]}"
        sudo xbps-install -S "${missing[@]}"
    else
        echo "All dependencies satisfied."
    fi
}

check_btrfs() {
    echo "Checking filesystem requirements..."
    if ! findmnt -n -o FSTYPE / | grep -q btrfs; then
        echo "⚠ WARNING: Root filesystem is not Btrfs!"
        echo "  Waypoint requires a Btrfs root filesystem to create snapshots."
        echo "  Continue anyway? [y/N]"
        read -r response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            echo "Installation cancelled."
            exit 1
        fi
    else
        echo " ✓ Btrfs root filesystem detected"
    fi
}

build_release() {
    echo "Building release binaries..."
    if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
        sudo -u "$SUDO_USER" env PROJECT_ROOT="$PROJECT_ROOT" bash -lc '
            cd "$PROJECT_ROOT"
            if [[ -f "$HOME/.cargo/env" ]]; then
                source "$HOME/.cargo/env"
            fi
            if ! command -v cargo >/dev/null 2>&1; then
                echo "cargo not found in user environment" >&2
                exit 1
            fi
            cargo build --release
        '
    else
        if [[ -f "$HOME/.cargo/env" ]]; then
            source "$HOME/.cargo/env"
        fi
        if ! command -v cargo >/dev/null 2>&1; then
            echo "cargo not found; install Rust toolchain" >&2
            exit 1
        fi
        cargo build --release
    fi
}

install_binaries() {
    echo "Installing binaries..."

    if [[ ! -f "target/release/waypoint" ]]; then
        echo "Release binary missing: target/release/waypoint" >&2
        exit 1
    fi
    if [[ ! -f "target/release/waypoint-helper" ]]; then
        echo "Release binary missing: target/release/waypoint-helper" >&2
        exit 1
    fi

    echo " → Installing waypoint to ${BINDIR}/waypoint"
    sudo install -D -m755 target/release/waypoint "${BINDIR}/waypoint"

    echo " → Installing waypoint-helper to ${BINDIR}/waypoint-helper"
    sudo install -D -m755 target/release/waypoint-helper "${BINDIR}/waypoint-helper"

    # Install CLI wrapper
    if [[ -f "waypoint-cli" ]]; then
        echo " → Installing waypoint-cli to ${BINDIR}/waypoint-cli"
        sudo install -D -m755 waypoint-cli "${BINDIR}/waypoint-cli"
    fi
}

install_desktop_entry() {
    if [[ ! -f "data/tech.geektoshi.waypoint.desktop" ]]; then
        echo "Desktop entry missing: data/tech.geektoshi.waypoint.desktop"
        return
    fi

    echo "Installing desktop entry..."
    sudo install -D -m644 data/tech.geektoshi.waypoint.desktop \
        "${DATADIR}/applications/tech.geektoshi.waypoint.desktop"

    if command -v update-desktop-database >/dev/null 2>&1; then
        sudo update-desktop-database "${DATADIR}/applications"
    fi
}

install_polkit_policy() {
    if [[ ! -f "data/tech.geektoshi.waypoint.policy" ]]; then
        echo "Polkit policy missing: data/tech.geektoshi.waypoint.policy"
        return
    fi

    echo "Installing Polkit policy..."
    sudo install -D -m644 data/tech.geektoshi.waypoint.policy \
        "${DATADIR}/polkit-1/actions/tech.geektoshi.waypoint.policy"
}

install_dbus_service() {
    if [[ ! -f "data/dbus-1/tech.geektoshi.waypoint.service" ]]; then
        echo "D-Bus service file missing"
        return
    fi
    if [[ ! -f "data/dbus-1/tech.geektoshi.waypoint.conf" ]]; then
        echo "D-Bus policy file missing"
        return
    fi

    echo "Installing D-Bus service files..."
    sudo install -D -m644 data/dbus-1/tech.geektoshi.waypoint.service \
        "${DATADIR}/dbus-1/system-services/tech.geektoshi.waypoint.service"

    sudo install -D -m644 data/dbus-1/tech.geektoshi.waypoint.conf \
        /etc/dbus-1/system.d/tech.geektoshi.waypoint.conf
}


create_metadata_dir() {
    echo "Creating metadata directory..."
    sudo install -d -m755 /var/lib/waypoint
}

install_icons() {
    local icon_source_dir="assets/icons/hicolor"

    if [[ ! -d "$icon_source_dir" ]]; then
        echo "Icon directory not found: $icon_source_dir"
        return
    fi

    echo "Installing application icons..."

    # Install PNG icons
    while IFS= read -r -d '' icon; do
        local relative_path="${icon#$icon_source_dir/}"
        local target="${DATADIR}/icons/hicolor/${relative_path}"
        echo " → Installing icon: $relative_path"
        sudo install -D -m644 "$icon" "$target"
    done < <(find "$icon_source_dir" -type f -name '*.png' -print0)

    # Install SVG icons
    while IFS= read -r -d '' icon; do
        local relative_path="${icon#$icon_source_dir/}"
        # SVG icons go in scalable/apps directory
        local target="${DATADIR}/icons/hicolor/scalable/apps/waypoint.svg"
        echo " → Installing icon: $relative_path"
        sudo install -D -m644 "$icon" "$target"
    done < <(find "$icon_source_dir" -type f -name '*.svg' -print0)

    # Update icon cache
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        echo " → Updating icon cache..."
        sudo gtk-update-icon-cache -f -t "${DATADIR}/icons/hicolor" 2>/dev/null || true
    fi
}

install_scheduler_service() {
    if [[ ! -f "services/waypoint-scheduler/run" ]]; then
        echo "Scheduler service script missing"
        return
    fi
    if [[ ! -f "services/waypoint-scheduler/log/run" ]]; then
        echo "Scheduler log script missing"
        return
    fi
    if [[ ! -f "data/waypoint-scheduler.conf" ]]; then
        echo "Scheduler config missing"
        return
    fi

    echo "Installing scheduler service..."
    sudo install -D -m755 services/waypoint-scheduler/run \
        /etc/sv/waypoint-scheduler/run

    sudo install -D -m755 services/waypoint-scheduler/log/run \
        /etc/sv/waypoint-scheduler/log/run

    sudo install -D -m644 data/waypoint-scheduler.conf \
        /etc/waypoint/scheduler.conf

    sudo install -d -m755 /var/log/waypoint-scheduler

    echo " ℹ To enable scheduler: sudo ln -s /etc/sv/waypoint-scheduler /var/service/"
    echo " ℹ To disable scheduler: sudo rm /var/service/waypoint-scheduler"
}

reload_dbus() {
    echo "Reloading D-Bus configuration..."

    # Kill any running waypoint-helper instances
    if sudo pkill -9 waypoint-helper 2>/dev/null; then
        echo " → Stopped old waypoint-helper instances"
    fi

    # Reload D-Bus using runit
    if command -v sv >/dev/null 2>&1 && sudo sv reload dbus 2>/dev/null; then
        echo " ✓ D-Bus reloaded successfully"
    else
        echo " ⚠ Could not reload D-Bus. You may need to reboot."
    fi
}

clean_build_artifacts() {
    echo "Cleaning build artifacts..."
    if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
        sudo -u "$SUDO_USER" env PROJECT_ROOT="$PROJECT_ROOT" bash -lc '
            cd "$PROJECT_ROOT"
            if [[ -f "$HOME/.cargo/env" ]]; then
                source "$HOME/.cargo/env"
            fi
            if command -v cargo >/dev/null 2>&1; then
                cargo clean
            fi
        '
    else
        if [[ -f "$HOME/.cargo/env" ]]; then
            source "$HOME/.cargo/env"
        fi
        if command -v cargo >/dev/null 2>&1; then
            cargo clean
        fi
    fi
}

uninstall_binaries() {
    echo "Removing binaries..."
    if [[ -f "${BINDIR}/waypoint" ]]; then
        echo " → Removing ${BINDIR}/waypoint"
        sudo rm -f "${BINDIR}/waypoint"
    fi
    if [[ -f "${BINDIR}/waypoint-helper" ]]; then
        echo " → Removing ${BINDIR}/waypoint-helper"
        sudo rm -f "${BINDIR}/waypoint-helper"
    fi
    if [[ -f "${BINDIR}/waypoint-cli" ]]; then
        echo " → Removing ${BINDIR}/waypoint-cli"
        sudo rm -f "${BINDIR}/waypoint-cli"
    fi
}

uninstall_desktop_entry() {
    local old_desktop_file="${DATADIR}/applications/com.voidlinux.waypoint.desktop"
    local new_desktop_file="${DATADIR}/applications/tech.geektoshi.waypoint.desktop"

    echo "Removing desktop entry..."
    # Remove both old and new namespace files
    [[ -f "$old_desktop_file" ]] && sudo rm -f "$old_desktop_file"
    [[ -f "$new_desktop_file" ]] && sudo rm -f "$new_desktop_file"

    if command -v update-desktop-database >/dev/null 2>&1; then
        sudo update-desktop-database "${DATADIR}/applications"
    fi
}

uninstall_polkit_policy() {
    local old_policy_file="${DATADIR}/polkit-1/actions/com.voidlinux.waypoint.policy"
    local new_policy_file="${DATADIR}/polkit-1/actions/tech.geektoshi.waypoint.policy"

    echo "Removing Polkit policy..."
    # Remove both old and new namespace files
    [[ -f "$old_policy_file" ]] && sudo rm -f "$old_policy_file"
    [[ -f "$new_policy_file" ]] && sudo rm -f "$new_policy_file"
}

uninstall_dbus_service() {
    echo "Removing D-Bus service files..."

    # Old namespace
    local old_service_file="${DATADIR}/dbus-1/system-services/com.voidlinux.waypoint.service"
    local old_conf_file="/etc/dbus-1/system.d/com.voidlinux.waypoint.conf"

    # New namespace
    local new_service_file="${DATADIR}/dbus-1/system-services/tech.geektoshi.waypoint.service"
    local new_conf_file="/etc/dbus-1/system.d/tech.geektoshi.waypoint.conf"

    # Remove both old and new namespace files
    [[ -f "$old_service_file" ]] && sudo rm -f "$old_service_file"
    [[ -f "$old_conf_file" ]] && sudo rm -f "$old_conf_file"
    [[ -f "$new_service_file" ]] && sudo rm -f "$new_service_file"
    [[ -f "$new_conf_file" ]] && sudo rm -f "$new_conf_file"
}


uninstall_icons() {
    echo "Removing application icons..."

    # Remove PNG icons in all sizes
    for size in 128x128 256x256 512x512; do
        local icon_path="${DATADIR}/icons/hicolor/${size}/apps/waypoint.png"
        if [[ -f "$icon_path" ]]; then
            echo " → Removing $icon_path"
            sudo rm -f "$icon_path"
        fi
    done

    # Remove SVG icon
    local svg_icon="${DATADIR}/icons/hicolor/scalable/apps/waypoint.svg"
    if [[ -f "$svg_icon" ]]; then
        echo " → Removing $svg_icon"
        sudo rm -f "$svg_icon"
    fi

    # Update icon cache
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        echo " → Updating icon cache..."
        sudo gtk-update-icon-cache -f -t "${DATADIR}/icons/hicolor" 2>/dev/null || true
    fi
}

uninstall_scheduler_service() {
    echo "Removing scheduler service..."

    # Stop and remove service if running
    if [[ -L /var/service/waypoint-scheduler ]]; then
        echo " → Stopping scheduler service"
        sudo sv stop waypoint-scheduler 2>/dev/null || true
        sudo rm -f /var/service/waypoint-scheduler
    fi

    # Remove service files
    if [[ -d /etc/sv/waypoint-scheduler ]]; then
        sudo rm -rf /etc/sv/waypoint-scheduler
    fi

    # Remove config (but preserve /etc/waypoint if waypoint.conf exists)
    if [[ -f /etc/waypoint/scheduler.conf ]]; then
        sudo rm -f /etc/waypoint/scheduler.conf
    fi

    # Remove log directory
    if [[ -d /var/log/waypoint-scheduler ]]; then
        sudo rm -rf /var/log/waypoint-scheduler
    fi
}

case "$ACTION" in
    install)
        cd "$PROJECT_ROOT"
        echo "=== Waypoint Installation ==="
        echo
        require_sudo
        check_btrfs
        install_dependencies
        build_release
        install_binaries
        install_desktop_entry
        install_icons
        install_polkit_policy
        install_dbus_service
        install_scheduler_service
        create_metadata_dir
        reload_dbus
        clean_build_artifacts
        echo
        echo "✓ Installation complete!"
        echo
        echo "You can now:"
        echo "  • Launch Waypoint from your application menu"
        echo "  • Run 'waypoint' for the GUI"
        echo "  • Run 'waypoint-cli' for CLI operations (create, list, delete, restore)"
        echo "  • Configure scheduled snapshots via GUI or /etc/waypoint/scheduler.conf"
        echo
        echo "Note: Snapshot metadata is stored in /var/lib/waypoint/"
        echo "Note: Enable scheduler: sudo ln -s /etc/sv/waypoint-scheduler /var/service/"
        ;;
    uninstall)
        cd "$PROJECT_ROOT"
        echo "=== Waypoint Uninstallation ==="
        echo
        require_sudo
        uninstall_binaries
        uninstall_desktop_entry
        uninstall_icons
        uninstall_polkit_policy
        uninstall_dbus_service
        uninstall_scheduler_service
        reload_dbus
        echo
        echo "✓ Uninstallation complete!"
        echo
        echo "Note: Snapshot metadata in /var/lib/waypoint/ was preserved."
        echo "      Remove manually if desired: sudo rm -rf /var/lib/waypoint"
        ;;
    *)
        echo "Usage: $0 [install|uninstall]" >&2
        echo
        echo "  install    - Build and install Waypoint system-wide"
        echo "  uninstall  - Remove Waypoint from the system"
        exit 1
        ;;
esac

echo "Done."
