# spec/REPUTATION.md — Reputação

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0020
**Relacionamento:** REQ-046..050, `SPECIFICATIONS.md §48–§50`, `§57`, `spec/GOVERNANCE.md §4.2`, `spec/DEFENSE.md §3`, THR-REP-001..003, THR-DEF-005, THR-GOV-001/002
**Implementação de referência:** `crates/rz-core/src/reputation.rs`, `crates/rz-core/src/state.rs`

---

## 1. O que é

A reputação de uma identidade é o seu saldo de **pontos de contribuição**. São os mesmos pontos que pesam na câmara de contribuição da governança e que habilitam credenciais defensivas. O saldo:

* fica ligado à **chave pública** (identidade criptográfica), nunca ao hardware nem a qualquer impressão do computador (REQ-046, THR-REP-003);
* nasce só de **eventos verificáveis pelo protocolo** (REQ-047, `SPEC §48`);
* **decai** com meia-vida `contribution_half_life_blocks`, então um histórico positivo não dá imunidade permanente (REQ-048);
* é **zerado** só por punição com evidência verificável (REQ-049, `SPEC §49`).

Pontos ficam em escala `SCALE = 10⁶`, e o decaimento segue `spec/GOVERNANCE.md §4.2` em aritmética inteira.

## 2. Eventos

| Evento | Efeito | Quem verifica |
| --- | --- | --- |
| Bloco produzido e finalizado | `+1` ponto para o proponente | o próprio bloco e seu certificado |
| Contribuição defensiva atestada (`AttestContribution`, após o encerramento do incidente) | `+contribution_points` para a identidade atestada | atestações de mais de 2/3 do poder de voto (`spec/DEFENSE.md`) |
| Punição por evidência (`ReportEquivocation`, `ReportDoubleVote`, `ReportDoubleProposal`) | pontos zerados | evidência com assinaturas válidas (`spec/CONSENSUS.md §6`) |

O que **não** gera reputação:

* ter credencial defensiva ou registrar ações dentro dela (`SPEC §57`, THR-DEF-005);
* estar conectado, ter saldo, ter ZERO vinculado, hospedar conteúdo, publicar Comunidades ou módulos;
* nenhuma avaliação subjetiva, voto de popularidade ou denúncia sem evidência.

A pontuação local de pares no P2P (`spec/P2P.md §6`) **não** é reputação: é proteção de recursos de cada Node, não vai para o estado e não persiste.

**Pesquisa não é ataque** (REQ-050, `SPEC §50`). Ler, compilar, testar, fazer fuzzing ou divulgar falhas com responsabilidade nunca gera punição. Só as infrações definidas no protocolo, com evidência assinada, reduzem a reputação.

## 3. Registro verificável

Eventos que não aparecem só no próprio bloco ficam registrados no estado (os 4096 mais recentes):

```text
RepEvent { key: PublicKey, kind, points_after: u64, height: u64 }
kind = 0 DefenseContribution { incident: fixed[32], role: u8 }
     | 1 Slashed { evidence: fixed[32] }
reputation_root = H("rede-zero/reputation-root/v1", u64(n) ‖ eventos)
```

`reputation_root` entra na raiz do estado depois de `bridge_root`. A produção de blocos é verificável pela própria cadeia e não é repetida no registro.

## 4. Parâmetros

| Campo | Padrão | Governança (constitucional) |
| --- | --- | --- |
| `contribution_half_life_blocks` | ~6 meses | tag 9 (`spec/GOVERNANCE.md`) |
| `defense.contribution_points` | 50 pontos (no máximo 1 000) | `defense_contribution_points` (tag 32) |
| `defense.credential_min_contribution` | 100 pontos | — |

## 5. Consulta

`GET_REPUTATION key` (0x33) → `REPUTATION { height, key, points (com decaimento na altura atual), events (≤64, do mais recente) }` (0x34).

Na Wallet: `zero-wallet reputation --public-key HEX`.

## 6. Testes

* `contribution_record_distinct_from_participation`: credencial e ações não geram pontos; a atestação gera; o evento fica registrado; os pontos decaem.
* `equivocation_report_slashes_contribution`: a punição zera os pontos e fica registrada.
* `robustness.rs`.
