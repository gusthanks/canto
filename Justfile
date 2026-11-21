build := "build"

compile *ARGS:
    just meson compile {{ARGS}}

run: install
    env RUST_BACKTRACE=full RUST_LOG='canto=debug,librespot=error' canto

install:
    just meson install

meson command *ARGS:
    meson {{command}} -C {{build}} {{ARGS}}

update-sources:
    python scripts/packaging/flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json

generate-spotify-api:
    ./scripts/api/generate-spotify-api.sh

init *ARGS:
    meson setup -Dbuildtype=release -Doffline=true --prefix="$HOME/.local" {{build}} {{ARGS}}

init-debug *ARGS:
    meson setup -Dbuildtype=debug -Doffline=true --prefix="$HOME/.local" {{build}} {{ARGS}}

