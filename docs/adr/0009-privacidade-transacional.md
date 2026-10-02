# ADR-0009 — Privacidade transacional: RingCT sobre Ristretto255

**Estado:** Aceita (DEVNET), sujeita a auditoria antes da TESTNET
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** REQ-023..028, `ARCHITECTURE.md §17–18`, `SPECIFICATIONS.md §31–32`, THR-PRIV-001, THR-TX-002
**Especificação:** [`spec/PRIVACY.md`](../../spec/PRIVACY.md)
**Implementação:** `crates/rz-privacy`, `crates/rz-core/src/private.rs`

## Contexto

REQ-024 exige proteger remetente, destinatário, valor, relação entre transações e histórico. `ARCHITECTURE.md §18` lista famílias a estudar: assinaturas em grupo, provas de conhecimento-zero, compromissos criptográficos e endereços furtivos. A ADR-0005 deixou a DEVNET com transações transparentes como aceitação temporária.

## Decisão

Adotar o modelo **RingCT**, combinando três dessas famílias:

| Protege | Mecanismo | Família (`ARCHITECTURE §18`) |
| --- | --- | --- |
| Destinatário | Endereços furtivos com chaves de visualização e gasto separadas; chave de uso único por saída | endereços furtivos |
| Valor | Compromissos de Pedersen + provas de faixa Bulletproofs | compromissos criptográficos, conhecimento-zero |
| Remetente e ligação entre transações | Assinaturas em anel **CLSAG** (anel de 11) com imagem de chave | assinaturas em grupo |

Escolhas adicionais:

* **Grupo Ristretto255** (ordem prima): elimina a classe de ataques de cofator e pontos de torção que já afetou implementações de imagem de chave sobre Ed25519.
* **Modelo híbrido**: contas transparentes continuam existindo (Genesis, taxas de validadores, futuros bloqueios de governança). A Wallet usa endereços privados por padrão (REQ-023); entradas e saídas do conjunto privado (`shield` / `unshield`) são explicitamente públicas.
* **Controle de oferta privada**: o protocolo rastreia publicamente o total de ZERO em notas privadas (somando blindagens e subtraindo retiradas e taxas) e rejeita qualquer transação que o tornaria negativo. Se uma falha criptográfica permitir criar moeda oculta, ela não poderá sair do conjunto privado além do que entrou (THR-TX-002).
* **Troco sempre presente**: toda transação privada cria ao menos duas saídas, para não revelar se houve troco.
* **Varredura local**: a Wallet baixa todas as saídas e imagens de chave; o Node não aprende quais notas pertencem a quem.

## Alternativas consideradas

| Alternativa | Motivo para não adotar agora |
| --- | --- |
| zk-SNARKs com conjunto de anonimato global (tipo Zcash Sapling/Orchard) | Anonimato muito maior, mas exige circuitos complexos e, em alguns sistemas, cerimônia de parâmetros confiáveis; custo de implementação e auditoria muito superior. Candidata natural para evolução |
| Mimblewimble | Não oculta o grafo de transações de observadores da rede sem agregação adicional; interação obrigatória entre remetente e destinatário |
| Somente valores ocultos | Não protege remetente nem destinatário |
| Mixers opcionais | Privacidade opcional reduz o conjunto de anonimato e contraria REQ-023 |

## Consequências

* Transações privadas têm ~1,5 a 3 KB (principalmente provas de faixa de 672 bytes por saída e assinaturas proporcionais ao anel).
* Verificação custa alguns milissegundos por transação.
* O estado cresce com todas as saídas já criadas (não é possível podar notas sem saber se foram gastas).

## Limitações honestas (REQ-028)

* **Anel de 11** oferece anonimato probabilístico: análises estatísticas (ex.: preferência por saídas recentes, heurísticas de disfarce) podem reduzir o conjunto efetivo. Ver `spec/PRIVACY.md §9`.
* No início da DEVNET, com poucas saídas, os anéis são menores (`min(11, total)`).
* **Blindagem e retirada são públicas**: valores e contas transparentes envolvidos ficam visíveis.
* A privacidade de rede (IP de origem) é tratada separadamente (ADR-0010).
* **Implementação não auditada.** CLSAG foi implementado neste projeto seguindo o artigo original; Bulletproofs vem de biblioteca madura. A auditoria é condição para a TESTNET.

## Condições de revisão

* Resultado de auditoria.
* Avaliação de migração para sistema com conjunto de anonimato global.
* Evidência de ataques estatísticos efetivos contra o tamanho de anel escolhido.
