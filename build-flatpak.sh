#!/bin/bash
set -euo pipefail

APP_ID="io.github.tkilian.CosmicAppletWorkspaceWindows"
BRANCH="stable"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="${SCRIPT_DIR}/${APP_ID}.json"
METINFO_FILE="${SCRIPT_DIR}/data/${APP_ID}.metainfo.xml"
CARGO_SOURCES="${SCRIPT_DIR}/cargo-sources.json"
BUILD_DIR="${SCRIPT_DIR}/builddir"
REPO_DIR="${SCRIPT_DIR}/repo"
OUTPUT_FILE="${SCRIPT_DIR}/${APP_ID}.flatpak"
STATE_DIR="${SCRIPT_DIR}/.flatpak-builder"
TOOLS_DIR="${STATE_DIR}/tools"
LOCAL_SOURCE_DIR="${STATE_DIR}/source"
LOCAL_MANIFEST="${STATE_DIR}/${APP_ID}.local.json"
PUBLISH_MANIFEST="${STATE_DIR}/${APP_ID}.publish.json"
GENERATOR_URL="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

ensure_flatpak_builder() {
    require_cmd flatpak-builder
}

ensure_cargo_generator() {
    require_cmd curl
    require_cmd python3

    mkdir -p "$TOOLS_DIR"

    if [[ ! -f "${TOOLS_DIR}/flatpak-cargo-generator.py" ]]; then
        curl -L --fail --silent --show-error \
            "$GENERATOR_URL" \
            -o "${TOOLS_DIR}/flatpak-cargo-generator.py"
    fi

    if [[ ! -x "${TOOLS_DIR}/venv/bin/python" ]]; then
        python3 -m venv "${TOOLS_DIR}/venv"
        "${TOOLS_DIR}/venv/bin/pip" install --quiet tomlkit aiohttp
    fi
}

validate_metadata() {
    require_cmd appstreamcli
    appstreamcli validate --no-net "$METINFO_FILE"
    python3 -m json.tool "$MANIFEST" >/dev/null
}

cleanup() {
    rm -rf "$BUILD_DIR" "$REPO_DIR" "$OUTPUT_FILE" "$STATE_DIR"
}

generate_sources() {
    echo "=== Generating cargo-sources.json ==="
    ensure_cargo_generator
    "${TOOLS_DIR}/venv/bin/python" \
        "${TOOLS_DIR}/flatpak-cargo-generator.py" \
        "${SCRIPT_DIR}/Cargo.lock" \
        -o "$CARGO_SOURCES"
}

prepare_local_source() {
    echo "=== Preparing local source snapshot ==="
    rm -rf "$LOCAL_SOURCE_DIR"
    mkdir -p "$LOCAL_SOURCE_DIR"

    tar -C "$SCRIPT_DIR" \
        --exclude=".git" \
        --exclude="target" \
        --exclude="builddir" \
        --exclude="repo" \
        --exclude=".flatpak-builder" \
        --exclude="${APP_ID}.flatpak" \
        -cf - . | tar -C "$LOCAL_SOURCE_DIR" -xf -
}

stage_manifest_support_files() {
    mkdir -p "$STATE_DIR"
    cp "$CARGO_SOURCES" "${STATE_DIR}/cargo-sources.json"
}

write_local_manifest() {
    stage_manifest_support_files
    python3 - "$MANIFEST" "$LOCAL_MANIFEST" "$LOCAL_SOURCE_DIR" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
source_path = Path(sys.argv[3]).as_posix()

manifest = json.loads(manifest_path.read_text())
manifest["modules"][0]["sources"][0] = {"type": "dir", "path": source_path}
output_path.write_text(json.dumps(manifest, indent=2) + "\n")
PY
}

write_publish_manifest() {
    require_cmd git
    stage_manifest_support_files

    if [[ -n "$(git -C "$SCRIPT_DIR" status --porcelain)" ]]; then
        echo "Refusing to generate a publish manifest from a dirty worktree." >&2
        exit 1
    fi

    python3 - "$MANIFEST" "$PUBLISH_MANIFEST" \
        "$(git -C "$SCRIPT_DIR" remote get-url origin)" \
        "$(git -C "$SCRIPT_DIR" branch --show-current)" \
        "$(git -C "$SCRIPT_DIR" rev-parse HEAD)" \
        "$(git -C "$SCRIPT_DIR" tag --points-at HEAD | head -n 1)" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
remote = sys.argv[3]
branch = sys.argv[4]
commit = sys.argv[5]
tag = sys.argv[6]

if remote.startswith("git@github.com:"):
    remote = "https://github.com/" + remote[len("git@github.com:"):]
elif remote.startswith("ssh://git@github.com/"):
    remote = "https://github.com/" + remote[len("ssh://git@github.com/"):]

source = {"type": "git", "url": remote, "branch": branch, "commit": commit}
if tag:
    source["tag"] = tag

manifest = json.loads(manifest_path.read_text())
manifest["modules"][0]["sources"][0] = source
output_path.write_text(json.dumps(manifest, indent=2) + "\n")
PY
}

build_bundle() {
    local manifest_path="$1"

    ensure_flatpak_builder
    validate_metadata

    echo "=== Building Flatpak ==="
    rm -rf "$BUILD_DIR"
    rm -rf "$REPO_DIR"
    mkdir -p "$REPO_DIR"
    flatpak-builder --force-clean --repo="$REPO_DIR" "$BUILD_DIR" "$manifest_path"

    echo "=== Creating bundle ==="
    rm -f "$OUTPUT_FILE"
    flatpak build-bundle "$REPO_DIR" "$OUTPUT_FILE" "$APP_ID" "$BRANCH"

    echo "=== Done! ==="
    ls -lh "$OUTPUT_FILE"
}

build() {
    generate_sources
    prepare_local_source
    write_local_manifest
    build_bundle "$LOCAL_MANIFEST"
}

publish() {
    generate_sources
    prepare_local_source
    write_local_manifest
    write_publish_manifest
    build_bundle "$LOCAL_MANIFEST"

    echo "=== Updating published repository metadata ==="
    flatpak build-update-repo "$REPO_DIR" --generate-static-deltas
    cp "$PUBLISH_MANIFEST" "$REPO_DIR/${APP_ID}.json"
    cp "$CARGO_SOURCES" "$REPO_DIR/cargo-sources.json"

    echo "Publish manifest: $PUBLISH_MANIFEST"
    echo "Published repository: $REPO_DIR"
}

write_manifest() {
    generate_sources
    write_publish_manifest
    echo "$PUBLISH_MANIFEST"
}

install_deps() {
    echo "=== Installing Flatpak dependencies ==="
    require_cmd flatpak
    flatpak install -y --user flathub \
        org.freedesktop.Platform/x86_64/24.08 \
        org.freedesktop.Sdk/x86_64/24.08 \
        org.freedesktop.Sdk.Extension.rust-stable/x86_64/24.08

    if ! command -v flatpak-builder >/dev/null 2>&1; then
        echo "Install flatpak-builder with your system package manager before building." >&2
    fi
}

case "${1:-build}" in
    clean)
        cleanup
        echo "Cleaned build directories"
        ;;
    deps)
        install_deps
        ;;
    sources)
        generate_sources
        ;;
    manifest)
        write_manifest
        ;;
    build)
        build
        ;;
    publish)
        publish
        ;;
    all)
        cleanup
        install_deps
        build
        ;;
    *)
        echo "Usage: $0 {clean|deps|sources|manifest|build|publish|all}"
        echo "  clean  - Clean build directories"
        echo "  deps   - Install Flatpak dependencies"
        echo "  sources - Generate cargo-sources.json"
        echo "  manifest - Generate a commit-pinned publish manifest"
        echo "  build  - Build the Flatpak package from the local worktree"
        echo "  publish - Build the bundle, update the local repo, and emit a publish manifest"
        echo "  all    - Clean, install deps, and build"
        exit 1
        ;;
esac
