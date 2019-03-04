#! /usr/bin/sh

set -o errexit
set -o nounset
set -o pipefail

export MANIFEST_PATH="org.gnome.PodcastsDevel.json"
export FLATPAK_MODULE="gnome-podcasts"
export CONFIGURE_ARGS="-Dprofile=development"
# export DBUS_ID="org.gnome.PodcastsDevel"
# export BUNDLE="org.gnome.Podcasts.Devel.flatpak"
# export RUNTIME_REPO="https://sdk.gnome.org/gnome-nightly.flatpakrepo"

flatpak-builder --stop-at=${FLATPAK_MODULE} --force-clean app ${MANIFEST_PATH}

# Build the flatpak repo
flatpak-builder --run app ${MANIFEST_PATH} meson --prefix=/app ${CONFIGURE_ARGS} build
flatpak-builder --run \
    --env=CARGO_HOME="target/cargo-home" \
    --env=CARGO_TARGET_DIR="target_test/" \
    --env=RUSTFLAGS="" \
    app ${MANIFEST_PATH} \
    ninja -C build

# Run the tests
xvfb-run -a -s "-screen 0 1024x768x24" \
    flatpak-builder --run \
    --env=CARGO_HOME="target/cargo-home" \
    --env=CARGO_TARGET_DIR="target_test/" \
    --env=RUSTFLAGS="" \
    app ${MANIFEST_PATH} \
    cargo test -j 1 -- --test-threads=1 --nocapture

# Create a flatpak bundle
# flatpak-builder --finish-only app ${MANIFEST_PATH}
# flatpak build-export repo app
# flatpak build-bundle repo ${BUNDLE} --runtime-repo=${RUNTIME_REPO} ${DBUS_ID}
