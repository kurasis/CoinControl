#!/usr/bin/env bash
# Debian cloud setup without modifying the host's package database.
set -euo pipefail
cd /workspace/CoinControl
export CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup
export PATH="$CARGO_HOME/bin:$PATH"
node --version
if ! command -v rustup >/dev/null; then
  curl --fail --location https://sh.rustup.rs --output /tmp/coincontrol-rustup.sh
  sh /tmp/coincontrol-rustup.sh -y --no-modify-path --profile minimal --default-toolchain 1.97.0
fi
rustup show active-toolchain
npm ci --cache /tmp/coincontrol-npm-cache --no-audit --no-fund
export PKG_CONFIG_PATH=/workspace/.system/usr/lib/x86_64-linux-gnu/pkgconfig:/workspace/.system/usr/share/pkgconfig
export PKG_CONFIG_SYSROOT_DIR=/workspace/.system
export LD_LIBRARY_PATH=/workspace/.system/usr/lib/x86_64-linux-gnu:/workspace/.system/lib/x86_64-linux-gnu
if ! pkg-config --exists gtk+-3.0 webkit2gtk-4.1; then
  mkdir -p /workspace/.system/debs /tmp/coincontrol-apt/lists/partial /tmp/coincontrol-apt/cache/archives/partial
  cat > /tmp/coincontrol-apt/apt.conf <<'APT'
Dir::Etc::main "-";
Dir::Etc::parts "-";
Dir::Etc::sourcelist "/tmp/coincontrol-apt/sources.list";
Dir::Etc::sourceparts "-";
Dir::State::lists "/tmp/coincontrol-apt/lists";
Dir::Cache "/tmp/coincontrol-apt/cache";
APT::Sandbox::User "agent";
Acquire::Retries "1";
APT
  cat > /tmp/coincontrol-apt/sources.list <<'SOURCES'
deb https://deb.debian.org/debian trixie main
deb https://deb.debian.org/debian trixie-updates main
deb https://security.debian.org/debian-security trixie-security main
SOURCES
  export APT_CONFIG=/tmp/coincontrol-apt/apt.conf
  apt-get update
  apt-cache depends --recurse --no-recommends --no-suggests --no-conflicts --no-breaks --no-replaces --no-enhances \
    libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
    librsvg2-dev libayatana-appindicator3-dev libdbus-1-dev |
    rg '^[A-Za-z0-9][A-Za-z0-9.+:-]*$' | sort -u > /tmp/coincontrol-system-packages
  (cd /workspace/.system/debs && xargs -a /tmp/coincontrol-system-packages apt-get download)
  for package in /workspace/.system/debs/*.deb; do dpkg-deb --extract "$package" /workspace/.system; done
fi
cat > /workspace/coincontrol-env.sh <<'ENV'
export CARGO_HOME=/workspace/.cargo
export RUSTUP_HOME=/workspace/.rustup
export PATH=/workspace/.cargo/bin:$PATH
export PKG_CONFIG_PATH=/workspace/.system/usr/lib/x86_64-linux-gnu/pkgconfig:/workspace/.system/usr/share/pkgconfig
export PKG_CONFIG_SYSROOT_DIR=/workspace/.system
export LD_LIBRARY_PATH=/workspace/.system/usr/lib/x86_64-linux-gnu:/workspace/.system/lib/x86_64-linux-gnu
ENV
npm run build
cargo fetch --locked
