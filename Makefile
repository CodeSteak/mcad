# controller-cad: Bauen, Prüfen, Installieren, Arch-Paket.
#
# Die Ziele rufen die Skripte auf, die auch die CI nutzt (scripts/ci/*.sh, smlib/*.sh,
# web/scripts/build-wasm.sh); `make help` listet sie. Variablen lassen sich überschreiben:
#   make app SMLIB_PREBUILT=/pfad/build-native
#   make install PREFIX=/usr DESTDIR=/tmp/paket
#   make arch ARCH_BUILD=docker
#
# GNU Make, bash.

SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help
.DELETE_ON_ERROR:
MAKEFLAGS += --no-builtin-rules

# --- Werkzeuge -------------------------------------------------------------------------
CARGO   ?= cargo
PNPM    ?= pnpm
NODE    ?= node
GIT     ?= git
DOCKER  ?= docker
MAKEPKG ?= makepkg
INSTALL ?= install

# --- Pfade -----------------------------------------------------------------------------
# Vorgebaute SMLib (libSMLib.a, libSM_API.a in $(SMLIB_PREBUILT)/lib; scripts/ci/smlib-native.sh)
SMLIB_PREBUILT ?= $(CURDIR)/smlib/build-native
# SMLib-Quellen (smlib/fetch.sh legt sie in third_party/smlib ab; build.rs braucht die Header)
SMLIB_SRC      ?= $(CURDIR)/third_party/smlib
# Emscripten SDK für smlib.wasm (leer: emcc aus PATH, sonst die Vorgabe von smlib/build-wasm.sh)
EMSDK          ?=
CARGO_TARGET_DIR ?= $(CURDIR)/target
CARGO_FLAGS    ?= --locked

# --- Installation ----------------------------------------------------------------------
PREFIX     ?= /usr/local
DESTDIR    ?=
BINDIR     ?= $(PREFIX)/bin
DATADIR    ?= $(PREFIX)/share
APPDIR     ?= $(DATADIR)/applications
ICONDIR    ?= $(DATADIR)/icons/hicolor
DOCDIR     ?= $(DATADIR)/doc/mcad
LICENSEDIR ?= $(DATADIR)/licenses/mcad
MIMEDIR    ?= $(DATADIR)/mime/packages

# --- Arch-Paket ------------------------------------------------------------------------
# auto: makepkg auf Arch Linux, sonst Docker (archlinux:latest)
ARCH_BUILD   ?= auto
ARCH_OUT     ?= $(CURDIR)/target/arch
ARCH_IMAGE   ?= archlinux:latest
MAKEPKG_FLAGS ?= --syncdeps --force
# Für Docker (siehe packaging/arch/docker.sh): Netz des Containers, zusätzliche CA (Proxy),
# Volume für Cargo-/pnpm-Caches (leer: keines)
ARCH_DOCKER_NETWORK ?= host
ARCH_CA_BUNDLE      ?=
ARCH_CACHE_VOLUME   ?= controller-cad-arch-cache

export SMLIB_PREBUILT SMLIB_SRC CARGO PNPM CARGO_TARGET_DIR
ifneq ($(EMSDK),)
export EMSDK
endif

# --- abgeleitete Werte -----------------------------------------------------------------
MCAD_BIN     := $(CARGO_TARGET_DIR)/release/mcad
SMLIB_HEADER := $(SMLIB_SRC)/source/SMLib/inc/SmSmlibAll.h
SMLIB_LIBS   := $(SMLIB_PREBUILT)/lib/libSMLib.a
SMLIB_WASM   := web/public/smlib/smlib.wasm
WEB_DEPS     := web/node_modules/.modules.yaml
WASM_PKG     := web/src/wasm-pkg/cad_wasm.js
UI_DIST      := web/dist-tauri/index.html
# App-Symbol (assets/icon): PNG je Größe und SVG für hicolor
ICON_SIZES   := $(sort $(patsubst assets/icon/png/%.png,%,$(wildcard assets/icon/png/*.png)))
ICON_SIZES   := $(foreach s,$(ICON_SIZES),$(firstword $(subst x, ,$(s))))
UI_SOURCES   := $(shell find web/src -type f -not -path 'web/src/wasm-pkg/*' 2>/dev/null) \
                web/index.html web/vite.config.ts web/tsconfig.json web/package.json web/pnpm-lock.yaml
# App-ID der Desktop-App (Tauri identifier): Name der Desktop-Datei, Wayland-app_id, WM_CLASS
APP_ID  := $(shell sed -n 's/.*"identifier": *"\([^"]*\)".*/\1/p' crates/cad-app/tauri.conf.json)
# Version: Cargo-Workspace; Paketversion mit Git-Stand (Arch-Schema für VCS-Pakete)
VERSION := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
GIT_REV := $(shell $(GIT) rev-list --count HEAD 2>/dev/null)
GIT_SHA := $(shell $(GIT) rev-parse --short=7 HEAD 2>/dev/null)
PKGVER  := $(VERSION)$(if $(GIT_SHA),.r$(GIT_REV).g$(GIT_SHA))
DIST    := $(ARCH_OUT)/mcad-$(PKGVER).tar.gz

ifeq ($(ARCH_BUILD),auto)
override ARCH_BUILD := $(if $(and $(wildcard /etc/arch-release),$(shell command -v $(MAKEPKG) 2>/dev/null)),native,docker)
endif

# $(call need,befehl,hinweis): bricht mit einer verständlichen Meldung ab, wenn der Befehl fehlt
need = command -v $(firstword $(1)) >/dev/null 2>&1 || { echo "make: '$(firstword $(1))' nicht gefunden – $(2)" >&2; exit 127; }
NEED_CARGO = $(call need,$(CARGO),Rust 1.97 installieren: https://rustup.rs (oder make CARGO=/pfad/zu/cargo))
NEED_PNPM  = $(call need,$(PNPM),pnpm 10.28 installieren: corepack enable oder npm i -g pnpm@10.28.0 (oder make PNPM=…))
NEED_NODE  = $(call need,$(NODE),Node.js 22.18 oder neuer installieren)
NEED_GIT   = $(call need,$(GIT),git installieren)
NEED_CMAKE = $(call need,cmake,CMake 3.20 oder neuer installieren (Ninja optional))
NEED_CXX   = $(call need,$${CXX:-c++},einen C++17-Compiler installieren (g++ 13 oder clang))

##@ Hilfe
.PHONY: help
help: ## Diese Übersicht (Standardziel)
	@echo "mcad $(VERSION) (Paketversion $(PKGVER))"
	@echo "Aufruf: make <ziel> [VARIABLE=wert …]"
	@awk 'BEGIN { FS = ":.*## " } \
	  /^##@/ { printf "\n%s\n", substr($$0, 5); next } \
	  /^[a-zA-Z0-9_.-]+:.*## / { printf "  %-14s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo
	@echo "Variablen (Auszug): CARGO=$(CARGO) PNPM=$(PNPM) SMLIB_PREBUILT=$(SMLIB_PREBUILT)"
	@echo "  SMLIB_SRC=$(SMLIB_SRC) EMSDK=$(or $(EMSDK),<PATH/Vorgabe>) CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)"
	@echo "  PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) ARCH_BUILD=$(ARCH_BUILD) ARCH_OUT=$(ARCH_OUT)"

##@ Abhängigkeiten
.PHONY: deps smlib smlib-fetch smlib-native smlib-wasm emsdk web-deps sysdeps
deps: smlib web-deps ## SMLib (nativ + Wasm) und pnpm-Pakete

smlib: smlib-native smlib-wasm ## SMLib nativ (-> SMLIB_PREBUILT) und als Wasm (-> web/public/smlib)

smlib-fetch: ## SMLib-Quellen (Fork + Patches) nach third_party/smlib
	@$(NEED_GIT)
	smlib/fetch.sh

smlib-native: ## SMLib nativ bauen, falls die Bibliotheken fehlen (scripts/ci/smlib-native.sh)
	@$(NEED_GIT); $(NEED_CMAKE); $(NEED_CXX)
	scripts/ci/smlib-native.sh "$(SMLIB_PREBUILT)"

smlib-wasm: ## smlib.wasm bauen, falls es fehlt (braucht emcc: make emsdk)
	@$(NEED_GIT); $(NEED_CMAKE)
	@command -v emcc >/dev/null 2>&1 || [ -x "$${EMSDK:-/opt/tools/emsdk}/upstream/emscripten/emcc" ] || \
	  { echo "make: emcc nicht gefunden – make emsdk EMSDK=~/emsdk, danach make … EMSDK=~/emsdk" >&2; exit 127; }
	scripts/ci/smlib-wasm.sh

emsdk: ## Emscripten 6.0.13 nach EMSDK (Vorgabe ~/emsdk) installieren (scripts/ci/emsdk.sh)
	@$(NEED_GIT)
	scripts/ci/emsdk.sh

web-deps: $(WEB_DEPS) ## pnpm install --frozen-lockfile (nur wenn package.json/Lockfile neuer)

sysdeps: ## Systempakete für Tauri/SMLib per apt (Debian/Ubuntu, scripts/ci/desktop.sh deps)
	@command -v apt-get >/dev/null 2>&1 || { echo "make: sysdeps braucht apt-get; Arch: siehe packaging/arch/PKGBUILD (depends/makedepends)" >&2; exit 127; }
	scripts/ci/desktop.sh deps

# Dateiziele: nur bauen, was fehlt (die Skripte prüfen das zusätzlich selbst)
$(SMLIB_HEADER):
	@$(NEED_GIT)
	smlib/fetch.sh

$(SMLIB_LIBS): | $(SMLIB_HEADER)
	@$(NEED_CMAKE); $(NEED_CXX)
	scripts/ci/smlib-native.sh "$(SMLIB_PREBUILT)"

$(SMLIB_WASM): | $(SMLIB_HEADER)
	@$(MAKE) --no-print-directory smlib-wasm

$(WEB_DEPS): web/package.json web/pnpm-lock.yaml
	@$(NEED_NODE); $(NEED_PNPM)
	scripts/ci/web.sh install
	@touch $@

##@ Bauen
.PHONY: wasm web ui cli app build
wasm: $(SMLIB_WASM) ## cad-wasm + wasm-bindgen + wasm-opt -> web/src/wasm-pkg (scripts/ci/web.sh wasm)
	@$(NEED_CARGO)
	scripts/ci/wasm-bindgen.sh
	scripts/ci/web.sh wasm

$(WASM_PKG):
	@$(MAKE) --no-print-directory wasm

web: $(WASM_PKG) $(WEB_DEPS) ## Browser-App -> web/dist (vite build)
	scripts/ci/web.sh build

ui: $(WEB_DEPS) ## UI der Desktop-App -> web/dist-tauri (pnpm build:tauri, ohne Wasm)
	scripts/ci/desktop.sh ui

# Ohne gebaute UI legt crates/cad-app/build.rs eine Platzhalterseite an (Datei `placeholder`):
# dann immer neu bauen.
ifneq ($(wildcard web/dist-tauri/placeholder),)
.PHONY: $(UI_DIST)
endif
$(UI_DIST): $(UI_SOURCES) $(WEB_DEPS)
	scripts/ci/desktop.sh ui

cli: $(SMLIB_LIBS) ## mcad ohne Desktop-UI (headless, ohne WebKitGTK) -> target/release/mcad
	@$(NEED_CARGO)
	$(CARGO) build --release $(CARGO_FLAGS) -p cad-cli --no-default-features
	@ls -l "$(MCAD_BIN)"

app: $(SMLIB_LIBS) $(UI_DIST) ## mcad mit Desktop-UI (Tauri 2, eingebettete UI) -> target/release/mcad
	@$(NEED_CARGO)
	$(CARGO) build --release $(CARGO_FLAGS) -p cad-cli
	@ls -l "$(MCAD_BIN)"

build: smlib web app ## Alles: SMLib nativ + Wasm, Browser-App, Desktop-App

##@ Prüfen
.PHONY: test test-rust test-web test-node e2e e2e-desktop lint fmt fmt-check clippy typecheck
test: test-rust test-web ## Rust-Tests (Workspace) und Vitest

test-rust: $(SMLIB_LIBS) ## cargo test --workspace (scripts/ci/rust.sh test)
	@$(NEED_CARGO)
	scripts/ci/rust.sh test

test-web: $(WEB_DEPS) ## Vitest (scripts/ci/web.sh unit)
	scripts/ci/web.sh unit

test-node: $(WASM_PKG) ## Node-Tests: smlib.wasm über die Brücke, Engine-Smoke-Test, Transport
	@$(NEED_NODE)
	scripts/ci/web.sh node

e2e: $(WASM_PKG) $(WEB_DEPS) ## Playwright (Chromium; baut und startet vite preview selbst)
	scripts/ci/web.sh e2e

e2e-desktop: app ## Tauri-E2E (tauri-driver + WebKitWebDriver, Xvfb ohne Display)
	@$(NEED_NODE); $(call need,tauri-driver,scripts/ci/desktop.sh e2e-deps)
	scripts/ci/desktop.sh e2e

lint: fmt-check clippy typecheck ## cargo fmt --check, clippy (nativ + wasm32), tsc

fmt: ## Rust-Quellen formatieren (cargo fmt --all)
	@$(NEED_CARGO)
	$(CARGO) fmt --all

fmt-check: ## cargo fmt --all --check
	@$(NEED_CARGO)
	scripts/ci/rust.sh fmt

clippy: $(SMLIB_LIBS) ## cargo clippy -D warnings, nativ und cad-wasm (wasm32)
	@$(NEED_CARGO)
	scripts/ci/rust.sh clippy clippy-wasm

typecheck: $(WASM_PKG) $(WEB_DEPS) ## tsc --noEmit (braucht die Typen aus web/src/wasm-pkg)
	scripts/ci/web.sh typecheck

##@ Installieren
.PHONY: install uninstall
install: ## mcad, Desktop-Datei, Icons, Lizenzen, Beispiele nach DESTDIR+PREFIX (vorher make app)
	@[ -x "$(MCAD_BIN)" ] || { echo "make: $(MCAD_BIN) fehlt – erst make app (oder make cli für headless)" >&2; exit 2; }
	$(INSTALL) -Dm755 "$(MCAD_BIN)" "$(DESTDIR)$(BINDIR)/mcad"
	$(INSTALL) -Dm644 packaging/linux/$(APP_ID).desktop "$(DESTDIR)$(APPDIR)/$(APP_ID).desktop"
	$(INSTALL) -Dm644 packaging/linux/mcad.xml "$(DESTDIR)$(MIMEDIR)/mcad.xml"
	@for s in $(ICON_SIZES); do \
	  echo "$(INSTALL) -Dm644 assets/icon/png/$${s}x$${s}.png $(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png"; \
	  $(INSTALL) -Dm644 "assets/icon/png/$${s}x$${s}.png" "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png"; done
	$(INSTALL) -Dm644 assets/icon/cad.svg "$(DESTDIR)$(ICONDIR)/scalable/apps/mcad.svg"
	$(INSTALL) -Dm644 -t "$(DESTDIR)$(LICENSEDIR)" packaging/linux/NOTICE crates/cad-font/fonts/OFL-*.txt
	$(INSTALL) -Dm644 crates/cad-font/fonts/README.md "$(DESTDIR)$(LICENSEDIR)/fonts-README.md"
	@if [ -f "$(SMLIB_SRC)/LICENSE" ]; then \
	  echo "$(INSTALL) -Dm644 $(SMLIB_SRC)/LICENSE $(DESTDIR)$(LICENSEDIR)/SMLib-LICENSE"; \
	  $(INSTALL) -Dm644 "$(SMLIB_SRC)/LICENSE" "$(DESTDIR)$(LICENSEDIR)/SMLib-LICENSE"; \
	  $(INSTALL) -Dm644 "$(SMLIB_SRC)/THIRD_PARTY_NOTICES.md" "$(DESTDIR)$(LICENSEDIR)/SMLib-THIRD_PARTY_NOTICES.md"; \
	else echo "make: Warnung: $(SMLIB_SRC)/LICENSE fehlt, SMLib-Lizenz nicht installiert (make smlib-fetch)" >&2; fi
	$(INSTALL) -Dm644 README.md "$(DESTDIR)$(DOCDIR)/README.md"
	$(INSTALL) -Dm644 .claude/skills/cad-cli/SKILL.md "$(DESTDIR)$(DOCDIR)/cad-cli.md"
	@cd examples && find . -type f | while read -r f; do \
	  $(INSTALL) -Dm644 "$$f" "$(DESTDIR)$(DOCDIR)/examples/$${f#./}"; done
	@echo "installiert: $(DESTDIR)$(BINDIR)/mcad (+ Desktop-Datei, Dateitypen, Icons, $(LICENSEDIR), $(DOCDIR))"

# Entfernt auch Desktop-Datei, Icons, Lizenzen und Doku unter den Namen vor der Umbenennung
# in mcad (0.3.0, controller-cad); ein altes $(BINDIR)/cad bleibt (könnte ein anderes Programm sein).
OLD_APP_ID := dev.controller-cad.desktop
uninstall: ## Entfernt, was make install angelegt hat (gleiche PREFIX/DESTDIR; auch alte controller-cad-Namen)
	rm -f "$(DESTDIR)$(BINDIR)/mcad" "$(DESTDIR)$(APPDIR)/$(APP_ID).desktop" "$(DESTDIR)$(MIMEDIR)/mcad.xml"
	rm -f "$(DESTDIR)$(APPDIR)/$(OLD_APP_ID).desktop" "$(DESTDIR)$(APPDIR)/controller-cad.desktop"
	for s in $(ICON_SIZES); do rm -f "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png" "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/controller-cad.png"; done
	rm -f "$(DESTDIR)$(ICONDIR)/scalable/apps/mcad.svg" "$(DESTDIR)$(ICONDIR)/scalable/apps/controller-cad.svg"
	rm -rf "$(DESTDIR)$(LICENSEDIR)" "$(DESTDIR)$(DOCDIR)" "$(DESTDIR)$(DATADIR)/licenses/controller-cad" "$(DESTDIR)$(DATADIR)/doc/controller-cad"

##@ Paket
.PHONY: dist arch arch-test arch-clean
dist: $(DIST) ## Quelltext-Archiv des Git-Stands HEAD -> target/arch/mcad-<pkgver>.tar.gz

$(DIST):
	@$(NEED_GIT)
	@[ -n "$(GIT_SHA)" ] || { echo "make: dist braucht ein Git-Checkout" >&2; exit 2; }
	@$(GIT) diff --quiet HEAD -- || echo "make: Hinweis: nicht eingecheckte Änderungen sind nicht im Archiv (git archive HEAD)" >&2
	mkdir -p "$(ARCH_OUT)"
	$(GIT) archive --format=tar.gz --prefix=mcad-$(PKGVER)/ -o "$@" HEAD

arch: $(DIST) ## Arch-Paket bauen (makepkg auf Arch, sonst Docker) -> target/arch/*.pkg.tar.zst
	sed -e 's/^pkgver=.*/pkgver=$(PKGVER)/' \
	    -e "s/^_srcsum=.*/_srcsum=$$(sha256sum "$(DIST)" | cut -d' ' -f1)/" \
	    packaging/arch/PKGBUILD > "$(ARCH_OUT)/PKGBUILD"
ifeq ($(ARCH_BUILD),native)
	@$(call need,$(MAKEPKG),pacman/makepkg (Arch Linux) – sonst make arch ARCH_BUILD=docker)
	cd "$(ARCH_OUT)" && $(MAKEPKG) $(MAKEPKG_FLAGS)
else ifeq ($(ARCH_BUILD),docker)
	@$(call need,$(DOCKER),Docker installieren oder auf Arch mit makepkg bauen (ARCH_BUILD=native))
	DOCKER="$(DOCKER)" ARCH_IMAGE="$(ARCH_IMAGE)" ARCH_DOCKER_NETWORK="$(ARCH_DOCKER_NETWORK)" \
	  ARCH_CA_BUNDLE="$(ARCH_CA_BUNDLE)" ARCH_CACHE_VOLUME="$(ARCH_CACHE_VOLUME)" \
	  packaging/arch/docker.sh build "$(ARCH_OUT)"
else
	@echo "make: ARCH_BUILD=$(ARCH_BUILD) unbekannt (auto, native, docker)" >&2; exit 2
endif
	@ls -l "$(ARCH_OUT)"/mcad-$(PKGVER)-*.pkg.tar.zst

arch-test: ## Paket in frischem archlinux-Container installieren und prüfen (CLI, xvfb-UI)
	@$(call need,$(DOCKER),Docker installieren)
	DOCKER="$(DOCKER)" ARCH_IMAGE="$(ARCH_IMAGE)" ARCH_DOCKER_NETWORK="$(ARCH_DOCKER_NETWORK)" \
	  ARCH_CA_BUNDLE="$(ARCH_CA_BUNDLE)" packaging/arch/docker.sh test "$(ARCH_OUT)"

arch-clean: ## target/arch, Builder-Image und Cache-Volume entfernen
	rm -rf "$(ARCH_OUT)"
	-DOCKER="$(DOCKER)" ARCH_CACHE_VOLUME="$(ARCH_CACHE_VOLUME)" packaging/arch/docker.sh clean

##@ Aufräumen
.PHONY: clean distclean
clean: ## Bauergebnisse: cargo clean, web/dist, web/dist-tauri, web/src/wasm-pkg, target/arch
	-$(CARGO) clean
	rm -rf web/dist web/dist-tauri web/src/wasm-pkg "$(ARCH_OUT)"

distclean: clean ## zusätzlich node_modules, smlib/build-native, web/public/smlib (Quellen bleiben)
	rm -rf web/node_modules smlib/build-native smlib/build-wasm
	find web/public/smlib -mindepth 1 -not -name .gitkeep -delete 2>/dev/null || true
