.PHONY: all build release install uninstall clean check test run

PREFIX ?= /usr
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share

all: build

build:
	cargo build

release:
	cargo build --release

install: release
	# Install binaries
	install -Dm755 target/release/waypoint $(DESTDIR)$(BINDIR)/waypoint
	install -Dm755 target/release/waypoint-helper $(DESTDIR)$(BINDIR)/waypoint-helper
	# Install desktop file
	install -Dm644 data/com.voidlinux.waypoint.desktop $(DESTDIR)$(DATADIR)/applications/com.voidlinux.waypoint.desktop
	# Install Polkit policy
	install -Dm644 data/com.voidlinux.waypoint.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/com.voidlinux.waypoint.policy
	# Install D-Bus service and policy
	install -Dm644 data/dbus-1/com.voidlinux.waypoint.service $(DESTDIR)$(DATADIR)/dbus-1/system-services/com.voidlinux.waypoint.service
	install -Dm644 data/dbus-1/com.voidlinux.waypoint.conf $(DESTDIR)/etc/dbus-1/system.d/com.voidlinux.waypoint.conf
	# Install XBPS hook
	install -Dm755 hooks/waypoint-pre-upgrade.sh $(DESTDIR)/etc/xbps.d/waypoint-pre-upgrade.sh
	install -Dm644 hooks/waypoint.conf $(DESTDIR)/etc/waypoint/waypoint.conf
	# Create metadata directory
	install -dm755 $(DESTDIR)/var/lib/waypoint

uninstall:
	# Remove binaries
	rm -f $(DESTDIR)$(BINDIR)/waypoint
	rm -f $(DESTDIR)$(BINDIR)/waypoint-helper
	# Remove desktop file
	rm -f $(DESTDIR)$(DATADIR)/applications/com.voidlinux.waypoint.desktop
	# Remove Polkit policy
	rm -f $(DESTDIR)$(DATADIR)/polkit-1/actions/com.voidlinux.waypoint.policy
	# Remove D-Bus service and policy
	rm -f $(DESTDIR)$(DATADIR)/dbus-1/system-services/com.voidlinux.waypoint.service
	rm -f $(DESTDIR)/etc/dbus-1/system.d/com.voidlinux.waypoint.conf
	# Remove XBPS hook
	rm -f $(DESTDIR)/etc/xbps.d/waypoint-pre-upgrade.sh
	rm -f $(DESTDIR)/etc/waypoint/waypoint.conf
	rmdir $(DESTDIR)/etc/waypoint 2>/dev/null || true
	# Note: /var/lib/waypoint is preserved to keep snapshot metadata

clean:
	cargo clean

check:
	cargo check

test:
	cargo test

run:
	cargo run --bin waypoint

run-helper:
	sudo cargo run --bin waypoint-helper

help:
	@echo "Waypoint Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make build      - Build debug binary"
	@echo "  make release    - Build optimized release binary"
	@echo "  make install    - Install to system (requires root)"
	@echo "  make uninstall  - Remove from system (requires root)"
	@echo "  make clean      - Remove build artifacts"
	@echo "  make check      - Check code without building"
	@echo "  make test       - Run tests"
	@echo "  make run        - Build and run (debug mode)"
	@echo ""
	@echo "Variables:"
	@echo "  PREFIX          - Installation prefix (default: /usr)"
	@echo "  DESTDIR         - Staging directory for packaging"
