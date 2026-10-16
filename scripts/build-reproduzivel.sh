#!/bin/sh
# Build reproduzível dos binários da Rede Zero (REQ-004, THR-DEV-003).
#
# Uso: scripts/build-reproduzivel.sh [DIR_DE_SAIDA]
#
# Qualquer pessoa, com o mesmo código (mesmo commit), o mesmo Cargo.lock e o
# compilador de rust-toolchain.toml, obtém binários idênticos byte a byte,
# em qualquer diretório. Os caminhos locais são substituídos por prefixos
# fixos; nada do ambiente entra no binário.
#
# Saída: os binários em DIR_DE_SAIDA/release e, em DIR_DE_SAIDA/artefatos.txt,
# o SHA-256 de cada um e o hash usado nos manifestos de versão
# (zero-wallet release-new --artifacts nome=arquivo; spec/RELEASES.md §3).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/target/reproduzivel}
cargo_home=${CARGO_HOME:-$HOME/.cargo}
cd "$root"
export SOURCE_DATE_EPOCH=0
export RUSTFLAGS="--remap-path-prefix=$root=/rede-zero --remap-path-prefix=$cargo_home=/cargo"
cargo build --release --locked --target-dir "$out" \
    -p rz-node -p rz-wallet -p rz-browser -p rz-conformance
: > "$out/artefatos.txt"
for b in rede-zero-node zero-wallet zero-navegador zero-conformidade; do
    sha256sum "$out/release/$b" | awk -v n="$b" '{print n, "sha256", $1}' >> "$out/artefatos.txt"
done
cat "$out/artefatos.txt"
