# Plano de lançamento da TESTNET

**Versão:** 0.1.0
**Relacionamento:** `SPECIFICATIONS.md §66–§67`, ADR-0012, ADR-0019, ADR-0021, ADR-0023, ADR-0025, ADR-0026

A TESTNET é a primeira rede pública da Rede Zero. O ZERO dela **não tem valor econômico**: a rede existe para testar o protocolo com pessoas e operadores reais antes da MAINNET.

## Decisões

| Item | Decisão |
| --- | --- |
| Identificador | `rede-zero-testnet-1`; um reinício usa `-2`, `-3`… (§5) |
| Validadores iniciais | ao menos 10 operadores independentes; com 10, a rede tolera 3 falhas ou validadores mal-intencionados |
| Política monetária | ADR-0025: `block_reward = 0,1 ZERO`, `fee_burn_bps = 5 000` |
| Genesis | 100 000 000 ZERO: 10 000 vinculados por validador e o resto no faucet |
| Faucet | operado pela pessoa que coordena o lançamento; **não vincula ZERO e não vota** (regra pública, conferível na cadeia) |
| Ponte | uma rede de teste do Bitcoin (signet ou testnet4), com ponto de partida no Genesis |

## Fase 0 — Código e documentação

1. `rede-zero-node genesis` monta o Genesis a partir de chaves públicas. Cada operador gera a chave na própria máquina e envia só a chave pública.
2. Faucet público: um limite por endereço e uma prova de trabalho contra abuso. Nenhum endereço IP é guardado.
3. Node e Wallet mostram o ambiente correto, com o aviso de que o ZERO da TESTNET não tem valor.
4. `testnet/` publica `genesis.bin`, o hash dele e a lista de Nodes iniciais.
5. Guia do operador (`docs/OPERACAO.md`).
6. Versão `v0.1.0-testnet` com binários reproduzíveis e manifesto de versão (ADR-0021).

## Fase 1 — Cerimônia do Genesis

1. Cada operador gera a chave do validador cifrada (`zero-wallet key-new`) e envia a chave pública e o endereço público do seu Node.
2. A coordenação monta o Genesis, publica `genesis.bin` e o hash, e abre um prazo de conferência.
3. Cada operador recompila a versão pelo build reproduzível, confere o hash do binário e o do Genesis, e confirma publicamente.
4. Todos iniciam o Node. Os blocos começam quando mais de 2/3 do poder de voto está no ar.

## Fase 2 — Operação pública

* **Entrada aberta:** qualquer pessoa obtém ZERO no faucet e vincula para se tornar validadora.
* **Testes com uso real:**
  * governança, incluindo o voto secreto e uma mudança de `block_reward`;
  * Comunidades e nomes;
  * mercado;
  * ponte com o Bitcoin de teste;
  * módulos do Runtime;
  * Navegador Zero.
* **Exercícios de incidente:**
  * atualização de versão adotada por proposta constitucional;
  * rotação e recuperação de chave;
  * Node com disco corrompido;
  * queda de validadores;
  * modo de defesa.
* **Ensaio da distribuição da MAINNET (ADR-0026)**, numa TESTNET dedicada, quando estiver implementado.
* **Falhas** são relatadas por `SECURITY.md`; além disso, há testes de carga.

## Fase 3 — Critérios para seguir à MAINNET

* Auditoria externa concluída e achados corrigidos, incluindo a ponte (ADR-0019, ADR-0026).
* Três meses sem falha crítica de consenso ou de integridade monetária.
* Ao menos uma atualização de versão feita pela governança na TESTNET.
* Distribuição da MAINNET implementada e ensaiada.

## 5. Reinício

Um erro grave de consenso ou de integridade monetária não é remendado na cadeia. A TESTNET recomeça com um identificador novo e um Genesis novo, e o incidente é registrado em `CHANGELOG.md`.
