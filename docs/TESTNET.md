# Plano de lançamento da TESTNET

**Versão:** 0.3.0
**Relacionamento:** `SPECIFICATIONS.md §66–§67`, ADR-0012, ADR-0019, ADR-0021, ADR-0023, ADR-0025, ADR-0026

A TESTNET é a primeira rede pública da Rede Zero. O ZERO dela **não tem valor econômico**: a rede existe para testar o protocolo com pessoas e operadores reais antes da MAINNET.

## Decisões

| Item | Decisão |
| --- | --- |
| Identificador | `rede-zero-testnet-1`; um reinício usa `-2`, `-3`… (§5) |
| Validadores iniciais | **TESTNET inicial da coordenação**: 10 validadores numa única máquina da pessoa que coordena (§1). Validadores independentes entram depois, pelo faucet (Fase 2) |
| Política monetária | ADR-0025: `block_reward = 0,1 ZERO`, `fee_burn_bps = 5 000` |
| Genesis | 100 000 000 ZERO: 10 000 vinculados por validador e o resto no faucet |
| Faucet | operado pela pessoa que coordena o lançamento; **não vincula ZERO e não vota** (regra pública, conferível na cadeia) |
| Ponte | uma rede de teste do Bitcoin (signet ou testnet4), adotada **depois do lançamento** por proposta constitucional (`PublishBridgeChain`), o que também exercita a governança |

## 1. Configuração inicial

A rede só finaliza blocos com **mais de 2/3** do poder de voto no ar (`spec/CONSENSUS.md`). O que importa é quanto poder cai junto quando uma máquina fica sem energia ou internet. Validadores de mesmo vínculo, todos numa só máquina:

| Aparelho | Validadores | Poder |
| --- | --- | --- |
| VPS (Contabo) | 10 | 100 % |

* A VPS é um **ponto único de falha**: se ela cair, a rede **para** até ela voltar. Nada se perde, e os blocos recomeçam sozinhos quando os Nodes voltam.
* Os 10 validadores rodam como 10 processos, cada um com chave, porta (7100 a 7109) e pasta próprias (`scripts/testnet-nos.sh`). Isso testa o protocolo entre Nodes, mas não protege contra a queda da máquina.
* A divisão anterior (VPS 7, PC 1 e dois celulares 1 cada) ficou para depois. Novos aparelhos da coordenação, e depois os validadores independentes, entram vinculando ZERO (Fase 2), sem mudar o Genesis.

**Transparência:** enquanto todo o poder de voto for da coordenação, e todo numa só máquina, a TESTNET é anunciada como **inicial, operada pela coordenação**. Ela testa o software, mas ainda não a descentralização. O critério da Fase 3 não muda: nenhum operador e nenhuma máquina com 1/3 ou mais do poder antes da MAINNET.

**Transparência:** enquanto todo o poder de voto for da coordenação, a TESTNET é anunciada como **inicial, operada pela coordenação**. Ela testa o software, mas ainda não a descentralização.

## Fase 0 — Código e documentação

1. ✅ `rede-zero-node genesis-testnet` monta o Genesis a partir de chaves públicas (um arquivo com uma chave por linha e a chave pública do faucet). Cada operador gera a chave na própria máquina e envia só a chave pública, que `zero-wallet new` e `zero-wallet address` mostram.
   * Os temporizadores do consenso são maiores que os da DEVNET, para a latência da Internet e do Tor (`ConsensusParams::testnet`: proposta 3 s, votos 1 s, época de 1 000 blocos).
2. ✅ Faucet público (`zero-wallet faucet-serve`; quem pede usa `zero-wallet faucet`).
   * Cada conexão recebe um desafio novo de prova de trabalho, de 22 bits por padrão.
   * Cada endereço recebe 1 000 ZERO a cada 24 h, e há um teto de 120 pagamentos por hora.
   * O limite usa só o endereço transparente, que é público: **nenhum endereço IP é guardado**.
   * Pode ficar atrás de um serviço onion; o pedido sai pela Wallet com `--proxy`.
   * 1 000 ZERO é o vínculo mínimo, então um pedido basta para virar validador.
3. ✅ Node e Wallet mostram o ambiente correto, com o aviso de que o ZERO da TESTNET não tem valor.
4. ✅ `testnet/` descreve o que será publicado na cerimônia: validadores, `genesis.bin` e o hash dele, Nodes iniciais e o faucet. Qualquer pessoa regenera o Genesis e compara.
5. ✅ Guia do operador (`docs/OPERACAO.md`).
6. Versão `v0.1.0-testnet` com binários reproduzíveis, marcada **no dia da cerimônia**, depois do merge em `main`. O manifesto de versão é publicado na cadeia logo após o lançamento (ADR-0021).

## Fase 1 — Cerimônia do Genesis

1. ✅ Em cada aparelho, `scripts/testnet-chaves.sh N DIR` gera as `N` chaves cifradas daquele aparelho e lista as chaves públicas: as 10 na VPS (§1). A chave do faucet é gerada à parte e não está entre as validadoras. As chaves privadas nunca saem do aparelho.
2. ✅ A coordenação monta o Genesis, publica `genesis.bin` e o hash em `testnet/`, e abre um prazo de conferência.
3. Cada operador recompila a versão pelo build reproduzível, confere o hash do binário e o do Genesis, e confirma publicamente.
4. Todos iniciam o Node. Os blocos começam quando mais de 2/3 do poder de voto está no ar.

## Fase 2 — Operação pública

* **Entrada aberta:** qualquer pessoa obtém ZERO no faucet e vincula para se tornar validadora. À medida que validadores independentes entram, a coordenação desvincula parte dos seus, até nenhum operador ter 1/3 do poder.
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
* **Nenhum operador, a coordenação incluída, e nenhuma máquina com 1/3 ou mais do poder de voto** durante pelo menos um mês, para mostrar que a rede funciona sem um ponto central (Manifesto §2, REQ-099).

## 5. Reinício

Um erro grave de consenso ou de integridade monetária não é remendado na cadeia. A TESTNET recomeça com um identificador novo e um Genesis novo, e o incidente é registrado em `CHANGELOG.md`.
