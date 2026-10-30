# Monitor de saúde da TESTNET

`rz-saude.sh` roda como `rede-zero` a cada 15 min (`rede-zero-saude.timer`). Uma linha por rodada em `~/monitor/saude.log` (últimas
3000); só a **mudança de estado** (OK ↔ PROBLEMA) vai para o diário do operador (`~/SESSOES.md`). Retorno 0 = OK.

Instalar (root):
```
install -d -o rede-zero ~rede-zero/monitor
install -m 755 -o rede-zero scripts/monitor/rz-saude.sh ~rede-zero/monitor/
install -m 644 scripts/monitor/rede-zero-saude.service scripts/monitor/rede-zero-saude.timer /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now rede-zero-saude.timer
```
Testar uma falha: `sudo -u rede-zero RZ_SAUDE_EXTRA_UNITS=unidade-inexistente ~rede-zero/monitor/rz-saude.sh` → PROBLEMA (lembre de
apagar a linha de teste do diário). Limitação conhecida: ver [RUNBOOK.md §1](../../docs/testnet/RUNBOOK.md).
