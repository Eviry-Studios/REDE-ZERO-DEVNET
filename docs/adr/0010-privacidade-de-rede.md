# ADR-0010 — Privacidade de rede: Dandelion++

**Estado:** Aceita
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** REQ-026, REQ-028, `ARCHITECTURE.md §19`, `SPECIFICATIONS.md §30`, THR-PRIV-002
**Especificação:** [`spec/P2P.md §4`](../../spec/P2P.md)

## Contexto

`ARCHITECTURE.md §19` separa privacidade da transação e privacidade da comunicação. Mesmo com transações privadas (ADR-0009), a difusão "todos para todos" revela a origem: o primeiro Node a anunciar uma transação é, com alta probabilidade, o de quem a criou. Um observador conectado a muitos Nodes explora isso para ligar transações a endereços IP (THR-PRIV-002).

## Decisão

Implementar **Dandelion++** na propagação de transações:

1. **Haste:** uma transação nova, recebida de um cliente (Wallet), é enviada a **um único** par — o relay da época — com a mensagem `STEM_TRANSACTION`. Ela não entra no mempool nem é difundida.
2. Cada Node que recebe uma transação em haste a valida e, com probabilidade 1/10, passa à **flor**; caso contrário, encaminha ao seu próprio relay (nunca de volta a quem enviou, quando houver alternativa).
3. **Flor:** a transação entra no mempool e é difundida normalmente (`TRANSACTION`).
4. **Relay por época:** cada Node escolhe aleatoriamente seu relay e o mantém por 10 minutos. Trocar de relay a cada transação facilitaria a inferência da origem por observação repetida.
5. **Embargo:** cada Node da haste guarda a transação com prazo aleatório de 10 a 20 s. Se ela não aparecer em fase de flor até lá, o próprio Node a difunde. Isso garante entrega mesmo que um Node da haste descarte a transação.
6. **Ciclo:** se uma transação em haste voltar a um Node que já a encaminhou, ele a difunde imediatamente.
7. Transações em haste são **totalmente validadas** antes de serem encaminhadas, para impedir que a haste seja usada para amplificar lixo.

## Alternativas consideradas

| Alternativa | Situação |
| --- | --- |
| Difusão simples | Revela a origem (estado anterior) |
| Tor / I2P obrigatórios | Dependência externa pesada; latência. Permanecem **complementares**: um Node pode ser operado atrás de Tor/I2P, e o suporte nativo fica como evolução |
| Mixnets com cobertura de tráfego | Proteção mais forte contra observador global, custo alto; evolução futura |

## Consequências

* A origem aparente de uma transação passa a ser um Node aleatório ao longo da haste.
* Latência de propagação um pouco maior (poucos saltos; no pior caso, o embargo).

## Limitações honestas (REQ-028)

* **A Wallet revela seu IP ao Node ao qual se conecta.** Dandelion++ protege contra o restante da rede, não contra o primeiro Node. Recomenda-se usar um Node próprio ou conectar-se via Tor.
* Um adversário que controla muitos Nodes da haste pode reduzir a proteção.
* Não protege contra um observador global passivo capaz de correlacionar todo o tráfego (THREAT_MODEL ADV-05).
* O canal entre Nodes ainda não é cifrado (THR-P2P-005): a haste não é oculta de quem observa os enlaces.

## Condições de revisão

Canal cifrado entre Nodes; suporte nativo a Tor/I2P; simulações de desanonimização com fração crescente de Nodes adversários.
