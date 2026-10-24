#!/bin/sh
# Inicia ou para os Nodes validadores desta máquina: um por chave
# DIR/validador-*.key, cada um com porta e pasta de dados próprias
# (docs/OPERACAO.md §5.1).
#
# Uso: scripts/testnet-nos.sh iniciar GENESIS DIR PORTA_BASE [PAR ...]
#      scripts/testnet-nos.sh parar DIR
#      scripts/testnet-nos.sh estado DIR
#
#   PAR       IP:porta de Nodes de OUTRAS máquinas (repita para vários)
#   ANUNCIAR  variável opcional com o IP ou nome público desta máquina; com
#             ela, os Nodes escutam em 0.0.0.0 e anunciam ANUNCIAR:porta.
#             Sem ela (celular, PC atrás de roteador), escutam só em
#             127.0.0.1 e apenas discam para os pares.
#
# O validador-1 usa PORTA_BASE, o validador-2 PORTA_BASE+1, e assim por
# diante. Os Nodes da mesma máquina também se conectam entre si.
set -eu
node=${RZ_NODE:-rede-zero-node}
cmd=${1:?uso: $0 iniciar|parar|estado ...}
shift
case "$cmd" in
iniciar)
    genesis=${1:?GENESIS}
    dir=${2:?DIR}
    base=${3:?PORTA_BASE}
    shift 3
    peers=""
    for p in "$@"; do peers="$peers --peer $p"; done
    if [ -n "${ANUNCIAR:-}" ]; then bind=0.0.0.0; else bind=127.0.0.1; fi
    : > "$dir/pids"
    i=1
    for k in "$dir"/validador-*.key; do
        [ -e "$k" ] || { echo "nenhuma chave em $dir" >&2; exit 1; }
        port=$((base + i - 1))
        extra=""
        [ "$i" -gt 1 ] && extra="--peer 127.0.0.1:$base"
        [ -n "${ANUNCIAR:-}" ] && extra="$extra --advertise $ANUNCIAR:$port"
        # shellcheck disable=SC2086 # listas de opções intencionais
        nohup "$node" run --genesis "$genesis" --data "$dir/dados-$i" \
            --listen "$bind:$port" --validator-key "$k" \
            --passphrase-file "$dir/senha" $peers $extra \
            > "$dir/no-$i.log" 2>&1 < /dev/null &
        echo $! >> "$dir/pids"
        echo "validador $i: porta $port, registro $dir/no-$i.log"
        i=$((i + 1))
    done
    ;;
parar)
    dir=${1:?DIR}
    [ -f "$dir/pids" ] && xargs kill < "$dir/pids" 2> /dev/null || true
    rm -f "$dir/pids"
    ;;
estado)
    dir=${1:?DIR}
    for l in "$dir"/no-*.log; do
        printf '%s: ' "$l"
        grep 'status:' "$l" | tail -1 || echo "sem status ainda"
    done
    ;;
*)
    echo "comando desconhecido: $cmd" >&2
    exit 1
    ;;
esac
