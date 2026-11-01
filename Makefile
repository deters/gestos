# Makefile for gestos (Meson wrapper)

PREFIX ?= /usr/local
BUILDDIR = build

.PHONY: all build install uninstall clean setup

all: build

setup:
	meson setup $(BUILDDIR) --prefix=$(PREFIX)

build:
	@if [ ! -d "$(BUILDDIR)" ]; then \
		meson setup $(BUILDDIR) --prefix=$(PREFIX); \
	fi
	meson compile -C $(BUILDDIR)

install:
	@if [ ! -d "$(BUILDDIR)" ]; then \
		meson setup $(BUILDDIR) --prefix=$(PREFIX); \
	fi
	meson install -C $(BUILDDIR)

uninstall:
	@if [ -d "$(BUILDDIR)" ]; then \
		systemctl disable --now gestos-input.socket 2>/dev/null || true; \
		systemctl stop 'gestos-input@*.service' 2>/dev/null || true; \
		ninja -C $(BUILDDIR) uninstall; \
		systemctl daemon-reload 2>/dev/null || true; \
	else \
		echo "Build directory not found, cannot uninstall automatically."; \
	fi

clean:
	rm -rf $(BUILDDIR)

.PHONY: version major minor patch

version:
	@if [ -z "$(VERSION)" ]; then \
		./packaging/bump-version.sh; \
	else \
		./packaging/bump-version.sh $(VERSION); \
	fi

major:
	./packaging/bump-version.sh major

minor:
	./packaging/bump-version.sh minor

patch:
	./packaging/bump-version.sh patch
