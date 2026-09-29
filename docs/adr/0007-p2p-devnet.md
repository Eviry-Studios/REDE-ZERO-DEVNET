# ADR-0007 — P2P da DEVNET sobre TCP

**Estado:** Aceita
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** SPEC §23–29, THR-P2P-001..006, THR-PRIV-002, THR-PRIV-003

## Contexto

O protocolo P2P definitivo, o mecanismo de descoberta e a privacidade de rede estão **A DEFINIR**.

## Decisão

* Transporte **TCP**, mensagens com prefixo de tamanho `u32` e corpo em codificação canônica (ADR-0004).
* **Handshake** `HELLO` obrigatório com: versão do protocolo, identificador da rede, hash do Genesis e altura atual. Qualquer divergência encerra a conexão (`AT-P2P-002`, `AT-GEN-002`).
* O handshake **não** envia versão do software, sistema operacional, fuso horário nem outros metadados desnecessários (THR-PRIV-003).
* Tamanho máximo de mensagem verificado antes da leitura do corpo (`AT-P2P-004`).
* Limite de taxa por conexão e pontuação local de mau comportamento, com desconexão (`AT-P2P-005`).
* Descoberta por lista de pares iniciais (*bootstrap*) e troca de endereços conhecidos.
* Sincronização por requisição de blocos a partir de uma altura; todo bloco recebido é validado integralmente (`SPEC §28`).

## Limitações aceitas na DEVNET

* Canal **sem cifragem** (THR-P2P-005) — a integridade vem das assinaturas, mas o tráfego é observável.
* Sem anonimização de rede (THR-PRIV-002).
* Proteção contra eclipse limitada (THR-P2P-001).

## Condições de revisão

Antes da TESTNET: canal autenticado e cifrado, estratégia anti-eclipse testada e avaliação de privacidade de rede.
