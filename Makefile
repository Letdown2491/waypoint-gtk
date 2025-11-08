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
	install -Dm755 waypoint-cli $(DESTDIR)$(BINDIR)/waypoint-cli
	# Install desktop file
	install -Dm644 data/tech.geektoshi.waypoint.desktop $(DESTDIR)$(DATADIR)/applications/tech.geektoshi.waypoint.desktop
	# Install Polkit policy
	install -Dm644 data/tech.geektoshi.waypoint.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/tech.geektoshi.waypoint.policy
	# Install D-Bus service and policy
	install -Dm644 data/dbus-1/tech.geektoshi.waypoint.service $(DESTDIR)$(DATADIR)/dbus-1/system-services/tech.geektoshi.waypoint.service
	install -Dm644 data/dbus-1/tech.geektoshi.waypoint.conf $(DESTDIR)/etc/dbus-1/system.d/tech.geektoshi.waypoint.conf
	# Install XBPS hook
	install -Dm755 hooks/waypoint-pre-upgrade.sh $(DESTDIR)/etc/xbps.d/waypoint-pre-upgrade.sh
	install -Dm644 hooks/waypoint.conf $(DESTDIR)/etc/waypoint/waypoint.conf
	# Install scheduler service
	install -dm755 $(DESTDIR)/etc/sv/waypoint-scheduler
	install -dm755 $(DESTDIR)/etc/sv/waypoint-scheduler/log
	install -Dm755 services/waypoint-scheduler/run $(DESTDIR)/etc/sv/waypoint-scheduler/run
	install -Dm755 services/waypoint-scheduler/log/run $(DESTDIR)/etc/sv/waypoint-scheduler/log/run
	install -Dm644 data/waypoint-scheduler.conf $(DESTDIR)/etc/waypoint/scheduler.conf
	# Create directories
	install -dm755 $(DESTDIR)/var/lib/waypoint
	install -dm755 $(DESTDIR)/var/log/waypoint-scheduler

uninstall:
	# Remove binaries
	rm -f $(DESTDIR)$(BINDIR)/waypoint
	rm -f $(DESTDIR)$(BINDIR)/waypoint-helper
	rm -f $(DESTDIR)$(BINDIR)/waypoint-cli
	# Remove desktop file
	rm -f $(DESTDIR)$(DATADIR)/applications/tech.geektoshi.waypoint.desktop
	# Remove Polkit policy
	rm -f $(DESTDIR)$(DATADIR)/polkit-1/actions/tech.geektoshi.waypoint.policy
	# Remove D-Bus service and policy
	rm -f $(DESTDIR)$(DATADIR)/dbus-1/system-services/tech.geektoshi.waypoint.service
	rm -f $(DESTDIR)/etc/dbus-1/system.d/tech.geektoshi.waypoint.conf
	# Remove XBPS hook
	rm -f $(DESTDIR)/etc/xbps.d/waypoint-pre-upgrade.sh
	rm -f $(DESTDIR)/etc/waypoint/waypoint.conf
	# Remove scheduler service
	rm -rf $(DESTDIR)/etc/sv/waypoint-scheduler
	rm -f $(DESTDIR)/etc/waypoint/scheduler.conf
	rmdir $(DESTDIR)/etc/waypoint 2>/dev/null || true
	# Note: /var/lib/waypoint and /var/log/waypoint-scheduler are preserved

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
