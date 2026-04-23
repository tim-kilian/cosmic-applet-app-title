set shell := ["sh", "-cu"]

default:
    @just --list

build:
    cargo build --release

flatpak-sources:
    ./build-flatpak.sh sources

flatpak-build:
    ./build-flatpak.sh build

flatpak-publish:
    ./build-flatpak.sh publish

install: build
    ./scripts/install-local.sh

restart-panel:
    ./scripts/restart-panel.sh

install-restart: install
    ./scripts/restart-panel.sh
