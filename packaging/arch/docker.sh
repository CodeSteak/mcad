#!/usr/bin/env bash
# Arch-Paket in Docker bauen und prüfen (für `make arch` / `make arch-test` ohne Arch Linux).
#
#   packaging/arch/docker.sh build OUT   # makepkg in OUT (PKGBUILD + Quelltext-Archiv von make arch)
#   packaging/arch/docker.sh test OUT    # OUT/controller-cad-*.pkg.tar.zst in frischem Container
#                                        # installieren, packaging/arch/smoke-test.sh ausführen
#   packaging/arch/docker.sh clean       # Builder-Image und Cache-Volume entfernen
#
# Umgebung (Vorgaben aus dem Makefile):
#   ARCH_IMAGE=archlinux:latest            Basis-Image
#   ARCH_BUILDER_IMAGE=controller-cad-arch-builder
#                                          Image mit base-devel, depends, makedepends, namcap
#   ARCH_DOCKER_NETWORK=host               Netz der Container (host: ein Proxy auf 127.0.0.1
#                                          des Hosts ist erreichbar)
#   ARCH_CA_BUNDLE=                        zusätzliche CA (PEM) im Container, z. B. für einen
#                                          TLS-Proxy; HTTPS_PROXY/NO_PROXY werden durchgereicht
#   ARCH_CACHE_VOLUME=controller-cad-arch-cache
#                                          Volume für Cargo- und pnpm-Caches (leer: keines)
#
# makepkg läuft als Benutzer `builder` (makepkg verweigert root); die Ergebnisse gehören
# danach wieder dem aufrufenden Benutzer.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER="${DOCKER:-docker}"
IMAGE="${ARCH_IMAGE:-archlinux:latest}"
BUILDER="${ARCH_BUILDER_IMAGE:-controller-cad-arch-builder}"
NETWORK="${ARCH_DOCKER_NETWORK:-host}"
CA="${ARCH_CA_BUNDLE:-}"
CACHE="${ARCH_CACHE_VOLUME-controller-cad-arch-cache}"

usage() { sed -n '2,7p' "$0" >&2; exit 2; }

# docker-run-Argumente: Netz, Proxy, CA
common=(--rm --network "$NETWORK")
build_args=(--network "$NETWORK")
for v in HTTPS_PROXY https_proxy HTTP_PROXY http_proxy NO_PROXY no_proxy; do
  if [ -n "${!v:-}" ]; then
    common+=(-e "$v=${!v}")
    build_args+=(--build-arg "$v=${!v}")
  fi
done
if [ -n "$CA" ]; then
  [ -f "$CA" ] || { echo "ARCH_CA_BUNDLE: $CA nicht gefunden" >&2; exit 2; }
  common+=(-v "$(cd "$(dirname "$CA")" && pwd)/$(basename "$CA"):/etc/ca-certificates/trust-source/anchors/extra-ca.crt:ro")
fi
# Vor pacman: zusätzliche CA in den Trust-Store übernehmen
TRUST='if [ -f /etc/ca-certificates/trust-source/anchors/extra-ca.crt ]; then update-ca-trust; fi'

builder_image() {
  local ctx
  ctx="$(mktemp -d)"
  # Nur die Paketlisten kommen ins Image (pkgver ändert sich mit jedem Commit, die Liste selten)
  bash -c 'source "$1"; printf "%s\n" "${depends[@]}" "${makedepends[@]}" "${checkdepends[@]}"' _ \
    "$HERE/PKGBUILD" | sort -u >"$ctx/deps.txt"
  if [ -n "$CA" ]; then cp "$CA" "$ctx/extra-ca.crt"; else : >"$ctx/extra-ca.crt"; fi
  cat >"$ctx/Dockerfile" <<EOF
FROM $IMAGE
COPY extra-ca.crt /etc/ca-certificates/trust-source/anchors/extra-ca.crt
RUN if [ -s /etc/ca-certificates/trust-source/anchors/extra-ca.crt ]; then update-ca-trust; \\
    else rm /etc/ca-certificates/trust-source/anchors/extra-ca.crt; fi
COPY deps.txt /tmp/deps.txt
RUN pacman -Syu --noconfirm --needed base-devel namcap \$(cat /tmp/deps.txt) && \\
    pacman -Scc --noconfirm && rm -rf /var/cache/pacman/pkg/*
RUN useradd -m -u 1000 builder
EOF
  local rc=0
  "$DOCKER" build "${build_args[@]}" -t "$BUILDER" "$ctx" || rc=$?
  rm -rf "$ctx"
  return $rc
}

cmd="${1:-}"
case "$cmd" in
  build)
    OUT="$(cd "${2:?Ausgabeordner fehlt}" && pwd)"
    [ -f "$OUT/PKGBUILD" ] || { echo "$OUT/PKGBUILD fehlt (make arch)" >&2; exit 2; }
    builder_image
    vol=()
    [ -n "$CACHE" ] && vol=(-v "$CACHE:/home/builder/.cache")
    "$DOCKER" run "${common[@]}" "${vol[@]}" -v "$OUT:/pkg" -w /pkg \
      -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -e JOBS="${JOBS:-}" -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-}" \
      "$BUILDER" bash -euo pipefail -c '
        '"$TRUST"'
        mkdir -p /home/builder/.cache
        chown -R builder: /pkg /home/builder/.cache
        [ -n "$JOBS" ] || unset JOBS
        [ -n "$CARGO_BUILD_JOBS" ] || unset CARGO_BUILD_JOBS
        rc=0
        runuser -u builder -- env CARGO_HOME=/home/builder/.cache/cargo \
          XDG_DATA_HOME=/home/builder/.cache/share \
          PKGDEST=/pkg SRCDEST=/pkg makepkg --force --noconfirm || rc=$?
        if [ "$rc" = 0 ]; then
          { echo "== namcap PKGBUILD"; namcap PKGBUILD || true
            for p in /pkg/*.pkg.tar.zst; do echo "== namcap $(basename "$p")"; namcap "$p" || true; done
          } 2>&1 | tee /pkg/namcap.txt
        fi
        chown -R "$HOST_UID:$HOST_GID" /pkg
        exit $rc'
    ;;
  test)
    OUT="$(cd "${2:?Ausgabeordner fehlt}" && pwd)"
    pkg="$(ls -t "$OUT"/controller-cad-*.pkg.tar.zst 2>/dev/null | head -n 1)"
    [ -n "$pkg" ] || { echo "kein Paket in $OUT (make arch)" >&2; exit 2; }
    ROOT="$(cd "$HERE/../.." && pwd)"
    mkdir -p "$OUT/test"
    "$DOCKER" run "${common[@]}" -v "$OUT:/pkg" -v "$ROOT/examples:/examples:ro" \
      -v "$HERE/smoke-test.sh:/smoke-test.sh:ro" -e PKG="/pkg/$(basename "$pkg")" -e HOST_UID="$(id -u)" \
      -e HOST_GID="$(id -g)" "$IMAGE" bash -euo pipefail -c '
        '"$TRUST"'
        rc=0
        bash /smoke-test.sh /pkg/test 2>&1 | tee /pkg/test/smoke-test.txt || rc=$?
        chown -R "$HOST_UID:$HOST_GID" /pkg/test
        exit $rc'
    ;;
  clean)
    "$DOCKER" image rm -f "$BUILDER" 2>/dev/null || true
    [ -z "$CACHE" ] || "$DOCKER" volume rm -f "$CACHE" 2>/dev/null || true
    ;;
  *) usage ;;
esac
