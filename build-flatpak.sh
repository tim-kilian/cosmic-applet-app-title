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

has_flatpak_builder() {
    command -v flatpak-builder >/dev/null 2>&1
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

git_worktree_clean() {
    require_cmd git
    [[ -z "$(git -C "$SCRIPT_DIR" status --porcelain)" ]]
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
    stage_manifest_support_files

    if ! git_worktree_clean; then
        echo "Refusing to generate a publish manifest from a dirty worktree." >&2
        return 1
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

read_manifest_metadata() {
    python3 - "$MANIFEST" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1]))
print(manifest["runtime"])
print(manifest["runtime-version"])
print(manifest["sdk"])
print(manifest["command"])
PY
}

read_finish_args() {
    python3 - "$MANIFEST" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1]))
for arg in manifest.get("finish-args", []):
    print(arg)
PY
}

build_bundle_with_builder() {
    local manifest_path="$1"

    echo "=== Building Flatpak with flatpak-builder ==="
    rm -rf "$BUILD_DIR"
    rm -rf "$REPO_DIR"
    mkdir -p "$REPO_DIR"
    flatpak-builder --force-clean --repo="$REPO_DIR" "$BUILD_DIR" "$manifest_path"
}

build_bundle_manually() {
    local arch runtime runtime_version sdk command
    local -a manifest_metadata finish_args

    require_cmd cargo
    require_cmd flatpak

    mapfile -t manifest_metadata < <(read_manifest_metadata)
    runtime="${manifest_metadata[0]}"
    runtime_version="${manifest_metadata[1]}"
    sdk="${manifest_metadata[2]}"
    command="${manifest_metadata[3]}"
    arch="$(flatpak --default-arch 2>/dev/null || echo x86_64)"

    mapfile -t finish_args < <(read_finish_args)

    echo "=== flatpak-builder not found; using manual local Flatpak build ==="
    echo "=== Building release binary ==="
    cargo build --release

    echo "=== Staging Flatpak filesystem ==="
    rm -rf "$BUILD_DIR"
    rm -rf "$REPO_DIR"
    mkdir -p "$REPO_DIR"
    install -Dm755 \
        "${SCRIPT_DIR}/target/release/cosmic-applet-workspace-windows" \
        "${BUILD_DIR}/files/bin/cosmic-applet-workspace-windows"
    install -Dm644 \
        "${SCRIPT_DIR}/data/${APP_ID}.desktop" \
        "${BUILD_DIR}/files/share/applications/${APP_ID}.desktop"
    install -Dm644 \
        "${SCRIPT_DIR}/data/${APP_ID}.metainfo.xml" \
        "${BUILD_DIR}/files/share/metainfo/${APP_ID}.metainfo.xml"
    install -Dm644 \
        "${SCRIPT_DIR}/data/icons/scalable/apps/${APP_ID}.svg" \
        "${BUILD_DIR}/files/share/icons/hicolor/scalable/apps/${APP_ID}.svg"

    cat > "$BUILD_DIR/metadata" << EOF
[Application]
name=${APP_ID}
runtime=${runtime}/${arch}/${runtime_version}
runtime-version=${runtime_version}
sdk=${sdk}/${arch}/${runtime_version}
command=${command}
EOF

    echo "=== Finishing Flatpak metadata ==="
    flatpak build-finish "$BUILD_DIR" "${finish_args[@]}"

    echo "=== Exporting repository ==="
    flatpak build-export "$REPO_DIR" "$BUILD_DIR" "$BRANCH"
}

build_bundle() {
    local manifest_path="$1"

    validate_metadata

    if has_flatpak_builder; then
        build_bundle_with_builder "$manifest_path"
    else
        build_bundle_manually
    fi

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
    build_bundle "$LOCAL_MANIFEST"

    echo "=== Updating published repository metadata ==="
    flatpak build-update-repo "$REPO_DIR" --generate-static-deltas
    cp "$CARGO_SOURCES" "$REPO_DIR/cargo-sources.json"

    if write_publish_manifest; then
        cp "$PUBLISH_MANIFEST" "$REPO_DIR/${APP_ID}.json"
        echo "Publish manifest: $PUBLISH_MANIFEST"
    else
        echo "Skipped commit-pinned publish manifest because the git worktree is dirty." >&2
        echo "Run './build-flatpak.sh manifest' from a clean worktree to generate it." >&2
    fi

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

    if ! has_flatpak_builder; then
        echo "Optional: install flatpak-builder for manifest-based local builds." >&2
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
        echo "  manifest - Generate a commit-pinned publish manifest (requires a clean git worktree)"
        echo "  build  - Build the Flatpak package from the local worktree"
        echo "  publish - Build the bundle and local repo; emits a publish manifest when the git worktree is clean"
        echo "  all    - Clean, install deps, and build"
        exit 1
        ;;
esac
