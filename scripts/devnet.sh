#!/usr/bin/env bash
# Inicia uma DEVNET local com N validadores (padrão 3).
#
#   scripts/devnet.sh [N]
#
# Dados e chaves ficam em ./devnet-data (ignorado pelo Git).
# Encerre com Ctrl+C.
set -euo pipefail

N="${1:-3}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/devnet-data"
BASE_PORT=7100

cd "$ROOT"
cargo build --release --quiet
NODE="$ROOT/target/release/rede-zero-node"

if [[ ! -f "$DATA/genesis.bin" ]]; then
  "$NODE" init-devnet --out "$DATA" --validators "$N" --base-port "$BASE_PORT"
fi

pids=()
cleanup() {
  echo "encerrando nodes..."
  kill "${pids[@]}" 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

for ((i = 0; i < N; i++)); do
  port=$((BASE_PORT + i))
  peer=()
  if ((i > 0)); then peer=(--peer "127.0.0.1:$BASE_PORT"); fi
  "$NODE" run \
    --genesis "$DATA/genesis.bin" \
    --data "$DATA/node-$i" \
    --listen "127.0.0.1:$port" \
    --validator-key "$DATA/validator-$i.key" \
    "${peer[@]}" &
  pids+=($!)
done

echo
echo "DEVNET com $N nodes em 127.0.0.1:$BASE_PORT..$((BASE_PORT + N - 1))"
echo "Exemplo:"
echo "  target/release/zero-wallet balance --genesis $DATA/genesis.bin --node 127.0.0.1:$BASE_PORT --key $DATA/faucet.key"
echo
wait
