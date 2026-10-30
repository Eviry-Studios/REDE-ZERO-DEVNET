# Runbook — máquina de coordenação da TESTNET

Complementa o [Guia do operador](../OPERACAO.md) (§7 é genérico). Vale para a máquina que roda os **10 validadores da coordenação**
(`rede-zero-validador@1..10`, portas 7100–7109), o **no-local** (`rede-zero-no-local`, 127.0.0.1:7110, não validador, usado pela Wallet e
pelo faucet) e o **faucet** (`rede-zero-faucet`, 127.0.0.1:7200, atrás do endereço onion de `testnet/faucet.txt`). Tudo roda como o usuário
`rede-zero`; binários e Genesis fixos em `~/bin`. Resultados dos exercícios: [RELATORIO-FASE2.md](RELATORIO-FASE2.md).

## 0. Regra de ouro
Com 10 validadores de mesmo peso, a finalização precisa de **mais de 2/3 → pelo menos 7 no ar**. Nunca pare mais de **3** ao mesmo tempo.
Nunca rode a mesma chave em dois processos (voto duplo queima 5 % do vínculo e exclui o validador).

## 1. Conferir a saúde
```
systemctl is-active rede-zero-validador@{1..10} rede-zero-no-local rede-zero-faucet | sort | uniq -c
zero-wallet status --genesis ~/bin/genesis.bin --node 127.0.0.1:7110     # altura, finalizado, ponta, pares
tail -5 ~/monitor/saude.log                                              # monitor a cada 15 min (scripts/monitor/)
```
Normal: 12 ativos; `finalizado` = `altura` (ou até 3 atrás); altura sobe ~0,48 bloco/s (≈ 41 700/dia).
**Ponto cego do monitor:** validador que não responde a `status` conta como "recusou" (limite de entradas por IP) e **não** dispara
alerta — e um validador sincronizando também não responde. Se `inativos=0` mas a finalização atrasar, confira cada validador à mão.

## 2. Um validador caiu (exercitado em 30/10, exercício 1)
1. `systemctl status rede-zero-validador@N` e `journalctl -u rede-zero-validador@N -n 50`.
2. `systemctl restart rede-zero-validador@N`.
3. Ele **não responde a `status` enquanto sincroniza**; espere. Medido: alcançou a ponta em **214 s** depois de 120 s fora.
4. A rede segue finalizando com 9 de 10 — não há pressa além de recuperar a margem.

## 3. Até 3 validadores caíram (exercitado em 30/10, exercício 2)
1. Mesmo procedimento do §2 para cada um.
2. Com 7 de 10 a rede **seguiu finalizando todos os blocos**, mais devagar (≈ 0,33 bloco/s com 3 fora).
3. Religados juntos, os 3 alcançaram a ponta em **~209 s**; durante a volta o ritmo da rede caiu para ≈ 0,3 bloco/s e normalizou depois.

## 4. Mais de 3 caíram — finalização parada
Sintoma: `altura` sobe (ou não) mas `finalizado` para. Prioridade: voltar a ter 7 no ar.
1. Religar primeiro os que têm processo parado (`systemctl start`), um a um, conferindo o journal.
2. Se a causa for a máquina (disco, memória): `df -h /`, `free -g`, `dmesg | tail`. Resolver a causa antes de religar em massa.
3. **Não exercitado** na TESTNET (de propósito: parar a finalização afeta quem usa a rede).

## 5. Disco corrompido / Node que não sobe
O Node corta o trecho corrompido e reconstrói pela rede (ADR-0024). Procedimento: parar a unit, mover o diretório de dados para
`*.corrompido-AAAAMMDD`, religar e acompanhar a sincronização; apagar a cópia só depois de conferido. **Ainda não exercitado** (RZ4: só no
no-local, que não é validador).

## 6. Chave
- Chave de validador vazada: `zero-wallet unbond` imediato e aviso à coordenação (o vínculo fica retido durante a desvinculação).
- Rotação/recuperação por guardiões: [OPERACAO.md §3](../OPERACAO.md). **Ainda não exercitado** na TESTNET (RZ4, com chave descartável).

## 7. Atualização de versão
Só depois de adotada por proposta constitucional; conferir o manifesto com `zero-wallet release-verify`, recompilar pelo
[§2 do guia](../OPERACAO.md) (build reproduzível), trocar os binários em `~/bin` e reiniciar **um validador por vez**, esperando cada um
alcançar a ponta (§2). Nunca reiniciar mais de 3 juntos. **Ainda não exercitado** (depende do primeiro manifesto de versão).

## 8. Faucet
`systemctl status rede-zero-faucet`; porta 7200 local; o onion é servido pelo Tor da máquina. O monitor confere a porta.
