#!/usr/bin/env bash
# Builds one solc version for the SP1 zkVM.
#   1. Gets the official release of that version: the source tarball and the Linux binary.
#   2. Cross-builds libsolc for RISC-V with solc's own CMake files.
#   3. Builds the zkVM program with `cargo prove build`.
# Output in builds/<long version>/: program.elf, solc (the official binary) and build.json.
# Usage: scripts/build.sh 0.8.38
# JOBS sets the parallel compile jobs (default 4). Each job needs up to 1.5 GB of RAM.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?usage: scripts/build.sh <solc version, for example 0.8.38>}
ROOT=$PWD
CACHE=$ROOT/.cache
JOBS=${JOBS:-4}
PATH=$HOME/.sp1/bin:$PATH # sp1up installs cargo-prove here

scripts/setup.sh
RISCV=$CACHE/riscv
BOOST=$CACHE/boost_1_83_0
CMAKE=$CACHE/cmake-4.4.5-linux-x86_64/bin/cmake
GXX=$RISCV/bin/riscv64-unknown-elf-g++
FLAGS=(-march=rv64im -mabi=lp64 -mcmodel=medany -ffunction-sections -fdata-sections
  "-ffile-prefix-map=$ROOT=." -O3 -DNDEBUG -std=c++20)

# 1. The official release. soliditylang.org lists the commit and the hash of each Linux binary.
curl -sSLf -o "$CACHE/solc-list.json" https://binaries.soliditylang.org/linux-amd64/list.json
BIN=$(jq -r --arg v "$VERSION" '.releases[$v] // empty' "$CACHE/solc-list.json")
[ -n "$BIN" ] || { echo "solc $VERSION is not an official release" >&2; exit 1; }
read -r LONG SHA < <(jq -r --arg p "$BIN" '.builds[] | select(.path == $p) | "\(.longVersion) \(.sha256)"' "$CACHE/solc-list.json")
COMMIT=${LONG##*commit.}
WORK=$CACHE/solc-$VERSION
OUT=$ROOT/builds/$LONG
mkdir -p "$WORK" "$OUT"

[ -f "$WORK/solc" ] || curl -sSLf -o "$WORK/solc" "https://binaries.soliditylang.org/linux-amd64/$BIN"
echo "${SHA#0x}  $WORK/solc" | sha256sum -c --quiet
install -m 755 "$WORK/solc" "$OUT/solc"

TARBALL=$WORK/solidity_$VERSION.tar.gz
[ -f "$TARBALL" ] || curl -sSLf -o "$TARBALL" \
  "https://github.com/argotorg/solidity/releases/download/v$VERSION/solidity_$VERSION.tar.gz"
TARBALL_SHA=$(sha256sum "$TARBALL" | cut -d' ' -f1)
# GitHub publishes a SHA-256 digest for newer release files. If there is one, it must match.
DIGEST=$(curl -sSLf "https://api.github.com/repos/argotorg/solidity/releases/tags/v$VERSION" |
  jq -r --arg n "solidity_$VERSION.tar.gz" '.assets[] | select(.name == $n) | .digest // empty')
if [ -n "$DIGEST" ]; then
  [ "$DIGEST" = "sha256:$TARBALL_SHA" ] || { echo "the source tarball does not match GitHub's digest" >&2; exit 1; }
else
  echo "GitHub has no digest for this tarball. Only the commit check applies."
fi
SRC=$WORK/source
[ -d "$SRC" ] || { mkdir "$SRC" && tar -xzf "$TARBALL" -C "$SRC" --strip-components=1; }
[ "$(cat "$SRC/commit_hash.txt")" = "$COMMIT" ] || { echo "the source is not commit $COMMIT" >&2; exit 1; }

# 2. Boost.Filesystem (the path code, plus status() for a system without files), then libsolc.
FS=$CACHE/boost-filesystem
if [ ! -f "$FS/libboost_filesystem.a" ]; then
  mkdir -p "$FS"
  for f in codecvt_error_category exception path path_traits portability utf8_codecvt_facet; do
    "$GXX" "${FLAGS[@]}" -DBOOST_FILESYSTEM_SOURCE -DBOOST_FILESYSTEM_STATIC_LINK=1 -DBOOST_ALL_NO_LIB \
      -I"$BOOST" -c "$BOOST/libs/filesystem/src/$f.cpp" -o "$FS/$f.o"
  done
  "$GXX" "${FLAGS[@]}" -DBOOST_FILESYSTEM_STATIC_LINK=1 -DBOOST_ALL_NO_LIB -I"$BOOST" \
    -c solc-build/boost-fs-nofiles.cpp -o "$FS/nofiles.o"
  "$RISCV/bin/riscv64-unknown-elf-ar" rcsD "$FS/libboost_filesystem.a" "$FS"/*.o
fi

"$CMAKE" -S "$SRC" -B "$WORK/build" \
  -DCMAKE_TOOLCHAIN_FILE="$ROOT/solc-build/toolchain.cmake" -DSOLC_ZK_RISCV="$RISCV" -DSOLC_ZK_ROOT="$ROOT" \
  -DSOLC_ZK_BOOST="$BOOST" -DBoost_DIR="$ROOT/solc-build/boost-config" \
  -DCMAKE_PROJECT_solidity_INCLUDE="$ROOT/solc-build/zkvm.cmake" \
  -DCMAKE_BUILD_TYPE=Release -DONLY_BUILD_SOLIDITY_LIBRARIES=ON -DTESTS=OFF -DTOOLS=OFF \
  -DPEDANTIC=OFF -DSTRICT_Z3_VERSION=OFF
"$CMAKE" --build "$WORK/build" --target libsolc -j "$JOBS"
LIBS=$WORK/lib
mkdir -p "$LIBS"
for lib in solc solidity yul evmasm smtutil langutil solutil; do
  cp "$(find "$WORK/build" -name "lib$lib.a" | head -1)" "$LIBS/"
done
cp "$FS/libboost_filesystem.a" "$LIBS/"

# 3. The zkVM program. The path prefix maps keep local folders out of it, so the program key
# depends only on the sources and the tool versions.
(cd guest/program && SOLC_ZK_LIBS="$LIBS" SOLC_ZK_RISCV="$RISCV" cargo prove build --locked \
  --output-directory "$OUT" --elf-name program.elf \
  --rustflags="--remap-path-prefix=$ROOT=.,--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo")

jq -n --arg version "$LONG" --arg source "$TARBALL_SHA" --arg official "${SHA#0x}" \
  --arg program "$(sha256sum "$OUT/program.elf" | cut -d' ' -f1)" \
  '{solcVersion: $version, sourceTarballSha256: $source, officialBinarySha256: $official, programElfSha256: $program}' \
  > "$OUT/build.json"
echo "Built solc $LONG for the zkVM: $OUT"
