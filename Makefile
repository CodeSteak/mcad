# controller-cad: build, test, install, Arch package.
#
# The targets call the scripts the CI uses too (scripts/ci/*.sh, smlib/*.sh,
# web/scripts/build-wasm.sh); `make help` lists them. Variables can be overridden:
#   make app SMLIB_PREBUILT=/path/build-native
#   make install PREFIX=/usr DESTDIR=/tmp/pkg
#   make arch-pkg ARCH_BUILD=docker
#
# GNU Make, bash.

SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help
.DELETE_ON_ERROR:
MAKEFLAGS += --no-builtin-rules

# --- Tools ----------------------------------------------------------------------------
CARGO   ?= cargo
PNPM    ?= pnpm
NODE    ?= node
GIT     ?= git
DOCKER  ?= docker
MAKEPKG ?= makepkg
INSTALL ?= install

# --- Paths -----------------------------------------------------------------------------
# Prebuilt SMLib (libSMLib.a, libSM_API.a in $(SMLIB_PREBUILT)/lib; scripts/ci/smlib-native.sh)
SMLIB_PREBUILT ?= $(CURDIR)/smlib/build-native
# SMLib sources (smlib/fetch.sh puts them in third_party/smlib; build.rs needs the headers)
SMLIB_SRC      ?= $(CURDIR)/third_party/smlib
# Emscripten SDK for smlib.wasm (empty: emcc from PATH, else the default of smlib/build-wasm.sh)
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

# --- Arch package ----------------------------------------------------------------------
# auto: makepkg on Arch Linux, otherwise Docker (archlinux:latest)
ARCH_BUILD   ?= auto
ARCH_OUT     ?= $(CURDIR)/target/arch
ARCH_IMAGE   ?= archlinux:latest
MAKEPKG_FLAGS ?= --syncdeps --force
# For Docker (see packaging/arch/docker.sh): container network, extra CA (proxy),
# volume for Cargo/pnpm caches (empty: none)
ARCH_DOCKER_NETWORK ?= host
ARCH_CA_BUNDLE      ?=
ARCH_CACHE_VOLUME   ?= controller-cad-arch-cache

export SMLIB_PREBUILT SMLIB_SRC CARGO PNPM CARGO_TARGET_DIR
ifneq ($(EMSDK),)
export EMSDK
endif

# --- derived values --------------------------------------------------------------------
MCAD_BIN     := $(CARGO_TARGET_DIR)/release/mcad
SMLIB_HEADER := $(SMLIB_SRC)/source/SMLib/inc/SmSmlibAll.h
SMLIB_LIBS   := $(SMLIB_PREBUILT)/lib/libSMLib.a
SMLIB_WASM   := web/public/smlib/smlib.wasm
WEB_DEPS     := web/node_modules/.modules.yaml
WASM_PKG     := web/src/wasm-pkg/cad_wasm.js
UI_DIST      := web/dist-tauri/index.html
# App icon (assets/icon): PNG per size and SVG for hicolor
ICON_SIZES   := $(sort $(patsubst assets/icon/png/%.png,%,$(wildcard assets/icon/png/*.png)))
ICON_SIZES   := $(foreach s,$(ICON_SIZES),$(firstword $(subst x, ,$(s))))
UI_SOURCES   := $(shell find web/src -type f -not -path 'web/src/wasm-pkg/*' 2>/dev/null) \
                web/index.html web/vite.config.ts web/tsconfig.json web/package.json web/pnpm-lock.yaml
# App ID of the desktop app (Tauri identifier): desktop file name, Wayland app_id, WM_CLASS
APP_ID  := $(shell sed -n 's/.*"identifier": *"\([^"]*\)".*/\1/p' crates/cad-app/tauri.conf.json)
# Version: Cargo workspace; package version with Git state (Arch scheme for VCS packages)
VERSION := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
GIT_REV := $(shell $(GIT) rev-list --count HEAD 2>/dev/null)
GIT_SHA := $(shell $(GIT) rev-parse --short=7 HEAD 2>/dev/null)
PKGVER  := $(VERSION)$(if $(GIT_SHA),.r$(GIT_REV).g$(GIT_SHA))
DIST    := $(ARCH_OUT)/mcad-$(PKGVER).tar.gz

ifeq ($(ARCH_BUILD),auto)
override ARCH_BUILD := $(if $(and $(wildcard /etc/arch-release),$(shell command -v $(MAKEPKG) 2>/dev/null)),native,docker)
endif

# $(call need,command,hint): aborts with a clear message when the command is missing
need = command -v $(firstword $(1)) >/dev/null 2>&1 || { echo "make: '$(firstword $(1))' not found – $(2)" >&2; exit 127; }
NEED_CARGO = $(call need,$(CARGO),install Rust 1.97: https://rustup.rs (or make CARGO=/path/to/cargo))
NEED_PNPM  = $(call need,$(PNPM),install pnpm 10.28: corepack enable or npm i -g pnpm@10.28.0 (or make PNPM=…))
NEED_NODE  = $(call need,$(NODE),install Node.js 22.18 or newer)
NEED_GIT   = $(call need,$(GIT),install git)
NEED_CMAKE = $(call need,cmake,install CMake 3.20 or newer (Ninja optional))
NEED_CXX   = $(call need,$${CXX:-c++},install a C++17 compiler (g++ 13 or clang))

##@ Help
.PHONY: help
help: ## This overview (default target)
	@echo "mcad $(VERSION) (package version $(PKGVER))"
	@echo "Usage: make <target> [VARIABLE=value …]"
	@awk 'BEGIN { FS = ":.*## " } \
	  /^##@/ { printf "\n%s\n", substr($$0, 5); next } \
	  /^[a-zA-Z0-9_.-]+:.*## / { printf "  %-14s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo
	@echo "Variables (excerpt): CARGO=$(CARGO) PNPM=$(PNPM) SMLIB_PREBUILT=$(SMLIB_PREBUILT)"
	@echo "  SMLIB_SRC=$(SMLIB_SRC) EMSDK=$(or $(EMSDK),<PATH/default>) CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)"
	@echo "  PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) ARCH_BUILD=$(ARCH_BUILD) ARCH_OUT=$(ARCH_OUT)"

##@ Dependencies
.PHONY: deps smlib smlib-fetch smlib-native smlib-wasm emsdk web-deps sysdeps
deps: smlib web-deps ## SMLib (native + Wasm) and pnpm packages

smlib: smlib-native smlib-wasm ## SMLib native (-> SMLIB_PREBUILT) and as Wasm (-> web/public/smlib)

smlib-fetch: ## SMLib sources (fork + patches) into third_party/smlib
	@$(NEED_GIT)
	smlib/fetch.sh

smlib-native: ## Build SMLib natively if the libraries are missing (scripts/ci/smlib-native.sh)
	@$(NEED_GIT); $(NEED_CMAKE); $(NEED_CXX)
	scripts/ci/smlib-native.sh "$(SMLIB_PREBUILT)"

smlib-wasm: ## Build smlib.wasm if it is missing (needs emcc: make emsdk)
	@$(NEED_GIT); $(NEED_CMAKE)
	@command -v emcc >/dev/null 2>&1 || [ -x "$${EMSDK:-/opt/tools/emsdk}/upstream/emscripten/emcc" ] || \
	  { echo "make: emcc not found – make emsdk EMSDK=~/emsdk, then make … EMSDK=~/emsdk" >&2; exit 127; }
	scripts/ci/smlib-wasm.sh

emsdk: ## Install Emscripten 6.0.13 into EMSDK (default ~/emsdk) (scripts/ci/emsdk.sh)
	@$(NEED_GIT)
	scripts/ci/emsdk.sh

web-deps: $(WEB_DEPS) ## pnpm install --frozen-lockfile (only if package.json/lockfile is newer)

sysdeps: ## System packages for Tauri/SMLib via apt (Debian/Ubuntu, scripts/ci/desktop.sh deps)
	@command -v apt-get >/dev/null 2>&1 || { echo "make: sysdeps needs apt-get; Arch: see packaging/arch/PKGBUILD (depends/makedepends)" >&2; exit 127; }
	scripts/ci/desktop.sh deps

# File targets: build only what is missing (the scripts check this themselves as well)
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

##@ Build
.PHONY: wasm web ui cli app build
wasm: $(SMLIB_WASM) ## cad-wasm + wasm-bindgen + wasm-opt -> web/src/wasm-pkg (scripts/ci/web.sh wasm)
	@$(NEED_CARGO)
	scripts/ci/wasm-bindgen.sh
	scripts/ci/web.sh wasm

$(WASM_PKG):
	@$(MAKE) --no-print-directory wasm

web: $(WASM_PKG) $(WEB_DEPS) ## Browser app -> web/dist (vite build)
	scripts/ci/web.sh build

ui: $(WEB_DEPS) ## UI of the desktop app -> web/dist-tauri (pnpm build:tauri, without Wasm)
	scripts/ci/desktop.sh ui

# Without a built UI, crates/cad-app/build.rs creates a placeholder page (file `placeholder`):
# then always rebuild.
ifneq ($(wildcard web/dist-tauri/placeholder),)
.PHONY: $(UI_DIST)
endif
$(UI_DIST): $(UI_SOURCES) $(WEB_DEPS)
	scripts/ci/desktop.sh ui

cli: $(SMLIB_LIBS) ## mcad without desktop UI (headless, without WebKitGTK) -> target/release/mcad
	@$(NEED_CARGO)
	$(CARGO) build --release $(CARGO_FLAGS) -p cad-cli --no-default-features
	@ls -l "$(MCAD_BIN)"

app: $(SMLIB_LIBS) $(UI_DIST) ## mcad with desktop UI (Tauri 2, embedded UI) -> target/release/mcad
	@$(NEED_CARGO)
	$(CARGO) build --release $(CARGO_FLAGS) -p cad-cli
	@ls -l "$(MCAD_BIN)"

build: smlib web app ## Everything: SMLib native + Wasm, browser app, desktop app

##@ Test
.PHONY: test test-rust test-web test-node e2e e2e-desktop lint fmt fmt-check clippy typecheck
test: test-rust test-web ## Rust tests (workspace) and Vitest

test-rust: $(SMLIB_LIBS) ## cargo test --workspace (scripts/ci/rust.sh test)
	@$(NEED_CARGO)
	scripts/ci/rust.sh test

test-web: $(WEB_DEPS) ## Vitest (scripts/ci/web.sh unit)
	scripts/ci/web.sh unit

test-node: $(WASM_PKG) ## Node tests: smlib.wasm via the bridge, engine smoke test, transport
	@$(NEED_NODE)
	scripts/ci/web.sh node

e2e: $(WASM_PKG) $(WEB_DEPS) ## Playwright (Chromium; builds and starts vite preview itself)
	scripts/ci/web.sh e2e

e2e-desktop: app ## Tauri E2E (tauri-driver + WebKitWebDriver, Xvfb without a display)
	@$(NEED_NODE); $(call need,tauri-driver,scripts/ci/desktop.sh e2e-deps)
	scripts/ci/desktop.sh e2e

lint: fmt-check clippy typecheck ## cargo fmt --check, clippy (native + wasm32), tsc

fmt: ## Format the Rust sources (cargo fmt --all)
	@$(NEED_CARGO)
	$(CARGO) fmt --all

fmt-check: ## cargo fmt --all --check
	@$(NEED_CARGO)
	scripts/ci/rust.sh fmt

clippy: $(SMLIB_LIBS) ## cargo clippy -D warnings, native and cad-wasm (wasm32)
	@$(NEED_CARGO)
	scripts/ci/rust.sh clippy clippy-wasm

typecheck: $(WASM_PKG) $(WEB_DEPS) ## tsc --noEmit (needs the types from web/src/wasm-pkg)
	scripts/ci/web.sh typecheck

##@ Install
.PHONY: install uninstall
install: ## mcad, desktop file, icons, licenses, examples into DESTDIR+PREFIX (make app first)
	@[ -x "$(MCAD_BIN)" ] || { echo "make: $(MCAD_BIN) missing – run make app first (or make cli for headless)" >&2; exit 2; }
	$(INSTALL) -Dm755 "$(MCAD_BIN)" "$(DESTDIR)$(BINDIR)/mcad"
	$(INSTALL) -Dm644 packaging/linux/$(APP_ID).desktop "$(DESTDIR)$(APPDIR)/$(APP_ID).desktop"
	$(INSTALL) -Dm644 packaging/linux/mcad.xml "$(DESTDIR)$(MIMEDIR)/mcad.xml"
	@for s in $(ICON_SIZES); do \
	  echo "$(INSTALL) -Dm644 assets/icon/png/$${s}x$${s}.png $(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png"; \
	  $(INSTALL) -Dm644 "assets/icon/png/$${s}x$${s}.png" "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png"; done
	$(INSTALL) -Dm644 assets/icon/cad.svg "$(DESTDIR)$(ICONDIR)/scalable/apps/mcad.svg"
	$(INSTALL) -Dm644 -t "$(DESTDIR)$(LICENSEDIR)" LICENSE packaging/linux/NOTICE crates/cad-font/fonts/OFL-*.txt
	$(INSTALL) -Dm644 crates/cad-font/fonts/README.md "$(DESTDIR)$(LICENSEDIR)/fonts-README.md"
	@if [ -f "$(SMLIB_SRC)/LICENSE" ]; then \
	  echo "$(INSTALL) -Dm644 $(SMLIB_SRC)/LICENSE $(DESTDIR)$(LICENSEDIR)/SMLib-LICENSE"; \
	  $(INSTALL) -Dm644 "$(SMLIB_SRC)/LICENSE" "$(DESTDIR)$(LICENSEDIR)/SMLib-LICENSE"; \
	  $(INSTALL) -Dm644 "$(SMLIB_SRC)/THIRD_PARTY_NOTICES.md" "$(DESTDIR)$(LICENSEDIR)/SMLib-THIRD_PARTY_NOTICES.md"; \
	else echo "make: warning: $(SMLIB_SRC)/LICENSE missing, SMLib license not installed (make smlib-fetch)" >&2; fi
	$(INSTALL) -Dm644 README.md "$(DESTDIR)$(DOCDIR)/README.md"
	$(INSTALL) -Dm644 .claude/skills/cad-cli/SKILL.md "$(DESTDIR)$(DOCDIR)/cad-cli.md"
	@cd examples && find . -type f | while read -r f; do \
	  $(INSTALL) -Dm644 "$$f" "$(DESTDIR)$(DOCDIR)/examples/$${f#./}"; done
	@echo "installed: $(DESTDIR)$(BINDIR)/mcad (+ desktop file, file types, icons, $(LICENSEDIR), $(DOCDIR))"

# Also removes desktop file, icons, licenses and docs under the names before the rename
# to mcad (0.3.0, controller-cad); an old $(BINDIR)/cad stays (could be another program).
OLD_APP_ID := dev.controller-cad.desktop
uninstall: ## Removes what make install created (same PREFIX/DESTDIR; also old controller-cad names)
	rm -f "$(DESTDIR)$(BINDIR)/mcad" "$(DESTDIR)$(APPDIR)/$(APP_ID).desktop" "$(DESTDIR)$(MIMEDIR)/mcad.xml"
	rm -f "$(DESTDIR)$(APPDIR)/$(OLD_APP_ID).desktop" "$(DESTDIR)$(APPDIR)/controller-cad.desktop"
	for s in $(ICON_SIZES); do rm -f "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/mcad.png" "$(DESTDIR)$(ICONDIR)/$${s}x$${s}/apps/controller-cad.png"; done
	rm -f "$(DESTDIR)$(ICONDIR)/scalable/apps/mcad.svg" "$(DESTDIR)$(ICONDIR)/scalable/apps/controller-cad.svg"
	rm -rf "$(DESTDIR)$(LICENSEDIR)" "$(DESTDIR)$(DOCDIR)" "$(DESTDIR)$(DATADIR)/licenses/controller-cad" "$(DESTDIR)$(DATADIR)/doc/controller-cad"

##@ Package
.PHONY: dist arch-pkg arch-test arch-clean
dist: $(DIST) ## Source archive of Git HEAD -> target/arch/mcad-<pkgver>.tar.gz

$(DIST):
	@$(NEED_GIT)
	@[ -n "$(GIT_SHA)" ] || { echo "make: dist needs a Git checkout" >&2; exit 2; }
	@$(GIT) diff --quiet HEAD -- || echo "make: note: uncommitted changes are not in the archive (git archive HEAD)" >&2
	mkdir -p "$(ARCH_OUT)"
	$(GIT) archive --format=tar.gz --prefix=mcad-$(PKGVER)/ -o "$@" HEAD

arch-pkg: $(DIST) ## Build the Arch package (makepkg on Arch, otherwise Docker) -> target/arch/*.pkg.tar.zst
	sed -e 's/^pkgver=.*/pkgver=$(PKGVER)/' \
	    -e "s/^_srcsum=.*/_srcsum=$$(sha256sum "$(DIST)" | cut -d' ' -f1)/" \
	    packaging/arch/PKGBUILD > "$(ARCH_OUT)/PKGBUILD"
ifeq ($(ARCH_BUILD),native)
	@$(call need,$(MAKEPKG),pacman/makepkg (Arch Linux) – otherwise make arch-pkg ARCH_BUILD=docker)
	cd "$(ARCH_OUT)" && $(MAKEPKG) $(MAKEPKG_FLAGS)
else ifeq ($(ARCH_BUILD),docker)
	@$(call need,$(DOCKER),install Docker or build with makepkg on Arch (ARCH_BUILD=native))
	DOCKER="$(DOCKER)" ARCH_IMAGE="$(ARCH_IMAGE)" ARCH_DOCKER_NETWORK="$(ARCH_DOCKER_NETWORK)" \
	  ARCH_CA_BUNDLE="$(ARCH_CA_BUNDLE)" ARCH_CACHE_VOLUME="$(ARCH_CACHE_VOLUME)" \
	  packaging/arch/docker.sh build "$(ARCH_OUT)"
else
	@echo "make: ARCH_BUILD=$(ARCH_BUILD) unknown (auto, native, docker)" >&2; exit 2
endif
	@ls -l "$(ARCH_OUT)"/mcad-$(PKGVER)-*.pkg.tar.zst
	@echo "Install: sudo pacman -U $$(ls -t "$(ARCH_OUT)"/mcad-$(PKGVER)-*.pkg.tar.zst | head -1)"

arch-test: ## Install and check the package in a fresh archlinux container (CLI, xvfb UI)
	@$(call need,$(DOCKER),install Docker)
	DOCKER="$(DOCKER)" ARCH_IMAGE="$(ARCH_IMAGE)" ARCH_DOCKER_NETWORK="$(ARCH_DOCKER_NETWORK)" \
	  ARCH_CA_BUNDLE="$(ARCH_CA_BUNDLE)" packaging/arch/docker.sh test "$(ARCH_OUT)"

arch-clean: ## Remove target/arch, builder image and cache volume
	rm -rf "$(ARCH_OUT)"
	-DOCKER="$(DOCKER)" ARCH_CACHE_VOLUME="$(ARCH_CACHE_VOLUME)" packaging/arch/docker.sh clean

##@ Clean
.PHONY: clean distclean
clean: ## Build outputs: cargo clean, web/dist, web/dist-tauri, web/src/wasm-pkg, target/arch
	-$(CARGO) clean
	rm -rf web/dist web/dist-tauri web/src/wasm-pkg "$(ARCH_OUT)"

distclean: clean ## additionally node_modules, smlib/build-native, web/public/smlib (sources stay)
	rm -rf web/node_modules smlib/build-native smlib/build-wasm
	find web/public/smlib -mindepth 1 -not -name .gitkeep -delete 2>/dev/null || true
