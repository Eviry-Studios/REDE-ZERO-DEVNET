# spec/DEFENSE.md — Defesa da Exonet

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0016
**Relacionamento:** `SPECIFICATIONS.md §51–§57`, REQ-051..063, AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003, THR-DEF-001..005, INV-009, INV-010
**Implementação de referência:** `crates/rz-core/src/defense.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node/src/node.rs`

---

## 1. Estado

```text
DefenseState {
  mode            : u8        // 0 NORMAL, 1 VIGILÂNCIA, 2 INCIDENTE, 3 GUERRA_CIBERNÉTICA
  mode_since      : u64
  mode_expires_at : u64       // 0 em NORMAL
  seq             : u64       // contador de decisões atestadas
  active_incident : option<fixed[32]>
  incidents       : mapa id → Incident       // nunca removidos
  credentials     : mapa id → Credential
  records         : mapa chave → list<DefenseRecord>
  mode_evidence   : list<(u64 altura, u8 modo, fixed[32] evidência)>
}

Incident   { id, opened_at, status (0 aberto, 1 contido, 2 recuperado, 3 encerrado, 4 sem vigor),
             evidence: list<fixed[32]> (≤256), actions: list<Action> (≤1024), closed_at? }
Credential { id, incident, holder: PublicKey, scopes: list<u8> (≤5), granted_at, expires_at, revoked }
Action     { credential, holder, scope, subject: fixed[32], height }
DefenseRecord { incident, role: u8, evidence, height }
```

Escopos (lista fechada, só defensiva): `0 diagnóstico`, `1 isolamento`, `2 recuperação`, `3 coordenação`, `4 preservação de evidências`.

## 2. Atestação

```text
Attestation { validator: PublicKey, signature: Signature }
assinatura = Ed25519(ctx "rede-zero/defense/v1" ‖ network_id ‖ payload)
```

O poder atestado é a soma do poder dos signatários no conjunto de validadores vigente (`state.validators()`). O conjunto inteiro é **inválido** se houver assinatura inválida, signatário repetido ou signatário fora do conjunto (AT-DEF-003). Limiares: **> 1/3** ou **> 2/3** do poder total.

Todo `payload` começa com a decisão e o `seq` vigente. Depois de aplicada uma decisão, `seq` aumenta, e atestações antigas não valem mais.

```text
transição   = u8(1) ‖ u64(seq) ‖ u8(modo) ‖ evidência
atualização = u8(2) ‖ u64(seq) ‖ incidente ‖ u8(estado) ‖ evidência
encerrar    = u8(3) ‖ u64(seq) ‖ incidente ‖ pacote_final
credencial  = u8(4) ‖ u64(seq) ‖ incidente ‖ portador ‖ list<u8>(escopos) ‖ u64(expires_at)
revogar     = u8(5) ‖ u64(seq) ‖ credencial
contribuir  = u8(6) ‖ u64(seq) ‖ incidente ‖ chave ‖ u8(papel) ‖ evidência
```

## 3. Operações

| Tag | Operação | Limiar | Regras |
| --- | --- | --- | --- |
| `0x50` | `DefenseTransition { to, evidence, seq, attestations }` | ver §4 | evidência ≠ 0 |
| `0x51` | `IncidentUpdate { incident, status, evidence, … }` | > 2/3 | incidente em vigor; aberto → contido → recuperado, nessa ordem |
| `0x52` | `CloseIncident { incident, archive, … }` | > 2/3 | incidente **recuperado** (AT-INC-002) |
| `0x53` | `GrantCredential { incident, holder, scopes, expires_at, … }` | > 2/3 | incidente em vigor; escopos não vazios e distintos; `h < expires_at ≤ h + credential_max_blocks`; portador validador vigente ou com pontos de contribuição `≥ credential_min_contribution` |
| `0x54` | `RevokeCredential { credential, … }` | > 1/3 | credencial existe e não está revogada |
| `0x55` | `DefenseAction { credential, scope, subject }` | — | remetente = portador; credencial não revogada, não vencida e com o escopo; incidente da credencial em vigor |
| `0x56` | `AttestContribution { incident, node, role, evidence, … }` | > 2/3 | incidente **encerrado** |

Qualquer conta pode enviar as decisões atestadas (paga a taxa). A autoridade vem das atestações.

## 4. Transições de modo

| De → para | Limiar | Condição adicional |
| --- | --- | --- |
| subir para VIGILÂNCIA | > 1/3 | — |
| subir para INCIDENTE | > 2/3 | abre um incidente se não houver um em vigor |
| subir para GUERRA | > 2/3 | incidente em vigor aberto há `≥ war_persistence_blocks` (AT-CYBER-001/002) |
| renovar o mesmo modo (≠ NORMAL) | o mesmo de entrada | estende o prazo |
| descer para qualquer modo inferior | > 1/3 | ao sair do nível de INCIDENTE, o incidente perde vigor (`Lapsed`) e suas credenciais são revogadas |

A cada transição: `mode_evidence` recebe a evidência; com incidente em vigor e modo ≥ INCIDENTE, a evidência também entra no incidente; `mode_expires_at = h + duração(modo)`.

**Vencimento** (fim de bloco, antes do mercado e da governança): se `modo ≠ NORMAL` e `h ≥ mode_expires_at`, o modo desce **um** nível (GUERRA → INCIDENTE → VIGILÂNCIA → NORMAL), com as mesmas regras de saída. Nenhum modo dura além do prazo sem nova atestação (THR-DEF-001, INV-010).

**Encerramento** (`CloseIncident`): estado encerrado, `closed_at = h`, o pacote final entra nas evidências, as credenciais do incidente são revogadas, não há incidente em vigor e o modo passa a VIGILÂNCIA (retorno gradual).

## 5. Isolamento

```text
isolados(h) = { a.subject | a ∈ ações do incidente em vigor, a.scope = isolamento,
                credencial(a) autoriza isolamento em h }
```

Nodes participantes recusam conexões de saída para identidades em `isolados` e encerram as existentes ao finalizar o bloco com a ação. Pares de entrada são anônimos (ADR-0011) e seguem as defesas locais (`spec/P2P.md §6`). O isolamento termina quando a credencial vence ou é revogada, ou quando o incidente perde vigor ou é encerrado.

## 6. Parâmetros (`ProtocolParams.defense`)

| Campo | Padrão (blocos de 2 s) | Governança |
| --- | --- | --- |
| `vigilance_max_blocks` | 43 200 (~1 dia) | `defense_vigilance_max_blocks` (tag 24), constitucional |
| `incident_max_blocks` | 129 600 (~3 dias) | `defense_incident_max_blocks` (tag 25), constitucional |
| `war_max_blocks` | 43 200 (~1 dia), nunca acima de `incident_max_blocks` | `defense_war_max_blocks` (tag 26), constitucional |
| `war_persistence_blocks` | 10 800 (~6 horas) | — |
| `credential_max_blocks` | 43 200 (~1 dia) | — |
| `credential_min_contribution` | 100 pontos | — |

## 7. Raiz do estado

`defense_root = H("rede-zero/defense-root/v1", modo ‖ mode_since ‖ mode_expires_at ‖ seq ‖ option(incidente) ‖ incidentes ‖ credenciais ‖ registros ‖ mode_evidence)`, mapas em ordem de chave. Entra na raiz do estado depois de `community_root`.

## 8. Consulta P2P

`GET_DEFENSE` (0x20) → `DEFENSE` (0x21): modo, prazos, `seq`, incidente em vigor e suas credenciais. Ver `spec/P2P.md`.

## 9. Testes de aceitação cobertos

AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003, vencimento dos modos e registro de contribuição (`crates/rz-core/src/defense_tests.rs`, `defense::tests`); isolamento com Nodes reais (`crates/rz-wallet/tests/defense_devnet.rs`); robustez (`crates/rz-p2p/tests/robustness.rs`).
