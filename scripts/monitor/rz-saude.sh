#!/bin/bash
# Monitor de saúde da TESTNET da Rede Zero (RZ2, sessão local 25/10). Roda como rede-zero pelo
# rede-zero-saude.timer (a cada 15 min). Confere: os 10 validadores + no-local + faucet ativos; a altura do
# no-local subindo desde a rodada anterior e com finalização em dia; os validadores que respondem na mesma
# ponta (até 5 blocos de diferença); a porta do faucet aberta.
# Cada rodada vira 1 linha em ~/monitor/saude.log; só a MUDANÇA de estado (OK <-> PROBLEMA) vai para o ~/SESSOES.md.
# Validador que recusa a consulta (limite de entradas por IP) mas está ativo não é falha.
set -uo pipefail
H=/home/rede-zero
G=$H/bin/genesis.bin
W=$H/bin/zero-wallet
M=$H/monitor
LOG=$M/saude.log
STATE=$M/estado
DIARIO=$H/SESSOES.md
MAX_ATRASO=5           # blocos de diferença tolerados entre validadores
MAX_NAO_FINAL=3        # altura - finalizado tolerado
EXTRA_UNITS="${RZ_SAUDE_EXTRA_UNITS:-}"   # só para teste (ex.: unidade inexistente simula falha)
mkdir -p "$M"

problemas=()
units="rede-zero-no-local rede-zero-faucet $(printf 'rede-zero-validador@%s ' $(seq 1 10)) $EXTRA_UNITS"
inativos=0
for u in $units; do
  systemctl is-active --quiet "$u" || { problemas+=("$u inativo"); inativos=$((inativos + 1)); }
done

status_of() { timeout 8 "$W" status --genesis "$G" --node "127.0.0.1:$1" 2>/dev/null; }
campo() { awk -v k="$1:" '$1 == k { print $2 }'; }

local_out=$(status_of 7110)
alt=$(campo altura <<<"$local_out"); fin=$(campo finalizado <<<"$local_out"); ponta=$(campo ponta <<<"$local_out"); pares=$(campo pares <<<"$local_out")
if [ -z "$alt" ]; then
  problemas+=("no-local nao respondeu")
  alt=0; fin=0; pares=0
else
  ant=$(awk '{print $1}' "$STATE" 2>/dev/null || echo 0)
  [ "$alt" -gt "${ant:-0}" ] || problemas+=("altura parada em $alt (anterior ${ant:-0})")
  [ $((alt - fin)) -le $MAX_NAO_FINAL ] || problemas+=("finalizacao atrasada ($fin de $alt)")
  [ "${pares:-0}" -ge 3 ] || problemas+=("no-local com poucos pares ($pares)")
fi

responderam=0; recusaram=0; max=$alt
for i in $(seq 0 9); do
  out=$(status_of $((7100 + i)))
  a=$(campo altura <<<"$out")
  if [ -z "$a" ]; then recusaram=$((recusaram + 1)); continue; fi
  responderam=$((responderam + 1))
  [ "$a" -gt "$max" ] && max=$a
  d=$((alt - a)); d=${d#-}
  [ "$d" -le $MAX_ATRASO ] || problemas+=("validador-$((i + 1)) fora de sincronia (altura $a, no-local $alt)")
done
[ "$responderam" -ge 7 ] || problemas+=("so $responderam de 10 validadores responderam")

(exec 3<>/dev/tcp/127.0.0.1/7200) 2>/dev/null || problemas+=("faucet: porta 7200 fechada")

agora=$(date -Is)
if [ ${#problemas[@]} -eq 0 ]; then estado=OK; else estado=PROBLEMA; fi
detalhe=$(IFS='; '; echo "${problemas[*]:-}")
echo "$agora $estado altura=$alt finalizado=$fin pares=$pares responderam=$responderam recusaram=$recusaram inativos=$inativos${detalhe:+ | $detalhe}" >> "$LOG"
tail -n 3000 "$LOG" > "$LOG.tmp" && mv "$LOG.tmp" "$LOG"

anterior=$(awk '{print $2}' "$STATE" 2>/dev/null || echo "")
echo "$alt $estado" > "$STATE"
if [ "$estado" != "${anterior:-OK}" ]; then
  if [ "$estado" = PROBLEMA ]; then
    printf -- '- %s **monitor de saúde: PROBLEMA** — %s (altura %s). Ver ~/monitor/saude.log.\n' "$agora" "$detalhe" "$alt" >> "$DIARIO"
  else
    printf -- '- %s monitor de saúde: voltou ao normal (altura %s).\n' "$agora" "$alt" >> "$DIARIO"
  fi
fi
[ "$estado" = OK ]
