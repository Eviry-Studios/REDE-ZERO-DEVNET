# TESTNET — relatório da Fase 2 (uso real e incidentes)

Resultados dos testes feitos na TESTNET pública depois do lançamento (`v0.1.0-testnet`, 24/10/2026). Todos os testes de uso usaram
**chaves descartáveis** recebidas pelo faucet; nenhuma chave de validador ou do faucet foi usada, salvo onde dito.
Plano da fase: [docs/TESTNET.md](../TESTNET.md). Operação: [RUNBOOK.md](RUNBOOK.md).

## Monitor de saúde (25/10)
`scripts/monitor/rz-saude.sh` + timer de 15 min: confere os 12 serviços, a altura subindo, a finalização em dia (≤ 3 blocos), pares ≥ 3,
validadores na mesma ponta (≤ 5 blocos) e pelo menos 7 de 10 respondendo, e a porta do faucet. Testado com falha simulada (unidade
inexistente → PROBLEMA → volta ao normal). Rodando desde então sem PROBLEMA.

## Uso real (25/10)
- **Exonet + nomes:** conteúdo empacotado → `zero://testefasedois.blog` registrado (taxa de 10 ZERO para o Pool) → publicado → resolvido →
  baixado com os **mesmos bytes, verificados por hash**. O Node recusa conteúdo/manifesto que o estado ainda não referencia (ordem certa).
- **Anticonfusão de nomes (THR-BRW-002):** `zero://testefasedoi5.blog` recusado ("nome já usado ou confundível").
  Achado: a mensagem de erro da Wallet não lista os tipos de nome aceitos (a do Navegador lista).
- **Comunidade:** declarada (1 de 1 chaves), aguardando reconhecimento por governança.
- **Runtime:** `examples/runtime/contador.wat` → `module-check` e `module-publish` aceitos; `module-bind`/`call` recusados enquanto a
  Comunidade não é reconhecida (comportamento esperado).
- **Navegador Zero:** serviu o site publicado com CSP própria, `nosniff` e `no-referrer`; Host fora de localhost → 421. Achado: nome
  inexistente respondia 502 → corrigido para **404** com página própria (PR "rz-browser: nome inexistente responde 404").
- **Grande Mercado:** nenhum ativo externo registrado; depende da ponte de teste (proposta constitucional) → depois.

## Governança (29–30/10)
- Estado lido: nenhuma proposta; voto secreto **desligado** (`vote_reveal_blocks` = 0) e mudá-lo é **constitucional** (≈ 39 dias de
  ciclo); ritmo medido ≈ 41 700 blocos/dia. Logo, ligar o voto secreto vem antes de uma mudança de `block_reward` com voto secreto.
- **Proposta aberta** (categoria comunidade, depósito de 100 ZERO da chave descartável) para reconhecer a Comunidade de teste:
  `b93f9bd8224014a2570db5fc5e94ec3cfcdebd3dfbf46454b431ff357e4ad7fe`. Análise até a altura 308567, ativação em 610967.

## Incidentes (30/10)
| Exercício | Resultado |
| --- | --- |
| 1 validador fora por 120 s (9 de 10) | rede **seguiu finalizando** todo bloco; o validador voltou e alcançou a ponta em **214 s** |
| 3 validadores fora por 120 s (7 de 10, limite de 2/3) | rede **seguiu finalizando** todo bloco, ritmo ≈ 0,33 bloco/s (normal ≈ 0,48); os 3 alcançaram a ponta em **~209 s**, com o ritmo da rede caindo para ≈ 0,3 bloco/s durante a volta |

Achados: (1) validador sincronizando não responde a `status` até chegar à ponta; (2) por isso o monitor não distingue "recusou pelo
limite de entradas" de "está sincronizando" — ver o ponto cego no [RUNBOOK.md](RUNBOOK.md) §1.
Faltam do plano: disco corrompido (no no-local), rotação/recuperação de chave descartável, atualização por proposta constitucional
(depende do primeiro manifesto de versão), carga leve.
