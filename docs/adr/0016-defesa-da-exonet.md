# ADR-0016 — Defesa da Exonet por atestação de validadores, com expiração automática

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-10-01
**Relacionamento:** REQ-051..063, REQ-046..050, `SPECIFICATIONS.md §48–§57`, `ARCHITECTURE.md §35–§40`, AC-DEF-001..010, AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003, THR-DEF-001..005, INV-009, INV-010, Manifesto §17–§20
**Especificação:** [`spec/DEFENSE.md`](../../spec/DEFENSE.md) v0.1.0
**Implementação:** `crates/rz-core/src/defense.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node` (isolamento), `crates/rz-wallet`

## Contexto

A defesa precisa de três coisas que se puxam em direções opostas:

* poder de reação (limitar, isolar, recuperar);
* nenhum poder permanente (REQ-059, INV-010);
* nenhuma autoridade única (REQ-060).

O modelo de ameaças aponta os riscos centrais: perpetuação da emergência (THR-DEF-001, CRÍTICA), abuso de credenciais, falso incidente e uso ofensivo (THR-DEF-002..004).

## Decisão

1. **Decisões por atestação de validadores.** Toda mudança de modo, avanço ou encerramento de incidente, concessão ou revogação de credencial e registro de contribuição exige assinaturas de validadores, ponderadas pelo poder de voto do consenso (ADR-0012).
   * Subir para vigilância exige **mais de 1/3**.
   * Incidente, guerra, encerramento, credenciais e contribuições exigem **mais de 2/3**.
   * **Descer de modo e revogar credencial exigem mais de 1/3.** Encerrar uma emergência é mais fácil que mantê-la.
   * Ninguém decide sozinho, nem o Agente Zero, que não é validador (THR-DEF-003).
2. **Evidência obrigatória** (`SPEC §52`): toda decisão carrega o hash de um pacote de evidências (fora da cadeia). Os hashes ficam no estado para sempre (AC-DEF-010).
3. **Contra repetição:** as atestações assinam um contador de decisões (`seq`). Uma atestação vale para uma única decisão.
4. **Modos com prazo** (`SPEC §53`, REQ-058).
   * NORMAL → VIGILÂNCIA → INCIDENTE → GUERRA_CIBERNÉTICA.
   * Guerra só a partir de um incidente aberto há pelo menos `war_persistence_blocks` ("ameaça persistente").
   * Cada modo tem duração máxima; sem renovação atestada, **desce um nível sozinho** (THR-DEF-001).
   * Um incidente que perde o modo sai de vigor (`Lapsed`), e suas credenciais são revogadas.
5. **Credenciais temporárias** (`SPEC §54`, INV-009).
   * Ligadas a um incidente, com escopos de uma lista fechada e só defensiva: diagnóstico, isolamento, recuperação, coordenação e preservação de evidências.
   * Validade máxima, revogação, e cada uso registrado como ação.
   * Só para quem tem histórico verificável: validador vigente ou pontos de contribuição mínimos. "Mais reputação, maior capacidade de ajudar, não maior soberania" (`ARCHITECTURE §38`).
6. **Sem operação ofensiva** (`SPEC §55`, AT-CYBER-006): não existe escopo nem transação para invadir, instalar código, coletar dados de terceiros ou atingir sistemas externos. O isolamento só faz Nodes participantes **deixarem de se conectar** a uma identidade.
7. **Encerramento verificável** (`SPEC §56`): Aberto → Contido → Recuperado → Encerrado, cada passo com evidência e mais de 2/3. Encerrar antes da recuperação é rejeitado. O encerramento registra o pacote final de evidências, revoga as credenciais e devolve a rede à VIGILÂNCIA (retorno gradual, REQ-063).
8. **Registro de contribuição ≠ participação** (`SPEC §57`, REQ-062): ter credencial ou registrar ações não gera registro. Só uma atestação de mais de 2/3, **após o encerramento**, grava a contribuição na identidade criptográfica do Node, nunca no hardware (THR-REP-003). Recompensa ou pontos por contribuição ficam A DEFINIR.

## Isolamento na rede

O Node conhece a identidade criptográfica só dos pares para os quais disca. Quem conecta a ele é anônimo por desenho (ADR-0011). O isolamento atestado, portanto:

* impede conexões de saída para a identidade isolada;
* derruba as existentes assim que o bloco com a ação é finalizado.

Pares de entrada anônimos continuam regidos pelo limite de taxa, pela pontuação e pela quarentena locais (`spec/P2P.md §6`).

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Conselho de defesa fixo | Autoridade permanente (REQ-059, REQ-060) |
| Modo de emergência sem prazo, encerrado por votação | Perpetuação (THR-DEF-001): quem controla a votação mantém a emergência |
| Ativação automática por métricas locais | Falso incidente (THR-DEF-003); métricas locais não são verificáveis por outros |
| Credenciais permanentes para Nodes de alta reputação | Reputação não é autorização eterna (REQ-048) |
| Mesma maioria para subir e descer | Uma minoria relevante deve poder encerrar a emergência |

## Consequências

* Com menos de 1/3 do poder bizantino, nenhum incidente falso é aberto. Com mais de 1/3 honesto, uma emergência indevida pode ser encerrada.
* Uma coalizão acima de 2/3 poderia abusar da defesa, mas também controlaria o consenso (fora do escopo, `THREAT_MODEL §12`). Mesmo assim, todo modo vence e toda credencial expira.
* A detecção de anomalias continua local (cada operador e o Agente Zero observam e propõem). O protocolo só registra decisões atestadas.

## Condições de revisão

* Pontos de contribuição para registros de defesa (câmara de contribuição, ADR-0008).
* Ações de alto impacto adicionais, sempre com escopo, prazo e atestação.
* Isolamento de pares de entrada sem quebrar o anonimato de clientes.
