#!/usr/bin/env bash
# Downloads the tools that every solc build uses into .cache/ and checks their SHA-256 hashes.
# scripts/build.sh runs it. Files that are already there are only checked again.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .cache && cd .cache

fetch() { # url file sha256
  [ -f "$2" ] || curl -sSLf -o "$2" "$1"
  echo "$3  $2" | sha256sum -c --quiet
}

# Succinct's GCC 15.2 for rv64im/lp64, with newlib and libstdc++ (the one `sp1up --c-toolchain` installs).
fetch https://github.com/succinctlabs/riscv-gnu-toolchain/releases/download/2026.02.13/riscv64im-linux-amd64.tar.gz \
  riscv64im-linux-amd64.tar.gz 7294ee279f52a75b0ad669fff60277d52e520b2b5fd7065c8264ac1ad93c1334
# Boost 1.83.0: the version and hash that solc's own WebAssembly build uses.
fetch https://archives.boost.io/release/1.83.0/source/boost_1_83_0.tar.bz2 \
  boost_1_83_0.tar.bz2 6478edfe2f3305127cffe8caf73ea0176c53769f4bf1585be237eb30798c3b8e
fetch https://github.com/Kitware/CMake/releases/download/v4.4.5/cmake-4.4.5-linux-x86_64.tar.gz \
  cmake-4.4.5-linux-x86_64.tar.gz ed85135c5a5135b45e50423cb406dea04ca3338da0b8ef3c9ede963559680be0

[ -d riscv ] || { mkdir riscv && tar -xzf riscv64im-linux-amd64.tar.gz -C riscv; }
[ -d boost_1_83_0 ] || tar -xjf boost_1_83_0.tar.bz2
[ -d cmake-4.4.5-linux-x86_64 ] || tar -xzf cmake-4.4.5-linux-x86_64.tar.gz
