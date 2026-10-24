#!/bin/sh
# Gera N chaves de validador cifradas nesta máquina e lista as chaves
# públicas (docs/TESTNET.md, Fase 1; docs/OPERACAO.md §3).
#
# Uso: scripts/testnet-chaves.sh N DIR
#
# A senha é a primeira linha de DIR/senha, criada antes pelo operador:
#   mkdir -p DIR && (umask 077; printf '%s\n' 'sua senha longa' > DIR/senha)
#
# Saída: DIR/validador-1.key … DIR/validador-N.key (cifradas, 0600, NUNCA
# envie nem versione) e DIR/publicas.txt, que é o que se envia à coordenação.
# Chaves já existentes são mantidas.
set -eu
n=${1:?uso: $0 N DIR}
dir=${2:?uso: $0 N DIR}
w=${ZERO_WALLET:-zero-wallet}
if [ ! -f "$dir/senha" ]; then
    echo "crie antes $dir/senha (uma linha, 8 ou mais caracteres, permissão 0600)" >&2
    exit 1
fi
host=$(hostname 2>/dev/null || echo maquina)
: > "$dir/publicas.txt"
i=1
while [ "$i" -le "$n" ]; do
    k="$dir/validador-$i.key"
    [ -e "$k" ] || "$w" new --key "$k" --passphrase-file "$dir/senha" > /dev/null
    "$w" address --key "$k" --passphrase-file "$dir/senha" |
        awk -v h="$host" -v i="$i" '/chave pública/ { print $3, "#", h, i }' >> "$dir/publicas.txt"
    i=$((i + 1))
done
cat "$dir/publicas.txt"
