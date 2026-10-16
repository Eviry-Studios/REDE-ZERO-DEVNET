# ACCEPTANCE_TESTS.md

# Rede Zero / Exonet

**Versão:** 0.1.0
**Status:** Suíte inicial de critérios de aceitação
**Natureza:** Testes executáveis e critérios de conformidade
**Relacionamento:**

* `REQUIREMENTS.md v0.2.0`
* `THREAT_MODEL.md v0.1.0`
* `ARCHITECTURE.md v0.1.0`
* `SPECIFICATIONS.md v0.1.0`

---

# 1. Objetivo

Este documento define os critérios de aceitação que uma implementação da Rede Zero deverá satisfazer.

A regra central é:

> **Uma característica somente será considerada implementada quando existir um teste automatizado capaz de demonstrar seu comportamento esperado.**

Os testes deverão verificar:

* comportamento correto;
* rejeição de operações inválidas;
* determinismo;
* integridade;
* isolamento;
* segurança;
* compatibilidade;
* recuperação;
* resistência a falhas.

---

# 2. Filosofia dos testes

A suíte deverá utilizar três categorias principais.

## 2.1 Teste positivo

Verifica que uma operação válida funciona.

```text
Entrada válida
      ↓
Operação
      ↓
Resultado esperado
```

---

## 2.2 Teste negativo

Verifica que uma operação inválida é rejeitada.

```text
Entrada inválida
      ↓
Operação
      ↓
REJEITAR
```

---

## 2.3 Teste de propriedade

Verifica uma propriedade que deve continuar verdadeira em diferentes entradas.

Exemplo:

```text
Para qualquer transação inválida:

resultado = REJEITADO
```

---

# 3. Convenções

Cada teste possuirá:

* ID;
* requisito relacionado;
* especificação relacionada;
* pré-condições;
* entrada;
* operação;
* resultado esperado;
* critério de aprovação.

Formato:

```text
TEST-XXX
Requirement: REQ-XXX
Specification: SPEC-XXX
Type: Positive / Negative / Property
```

---

# 4. Estados de execução

Cada teste deverá produzir um dos estados:

```text
PASS
FAIL
SKIP
BLOCKED
```

### PASS

Critério satisfeito.

### FAIL

Critério não satisfeito.

### SKIP

Teste deliberadamente não executado.

### BLOCKED

Teste não pode ser executado porque uma dependência ainda não foi implementada.

Um teste `BLOCKED` não deve ser confundido com `PASS`.

---

# 5. Critério global de aceitação

Uma versão da Rede Zero não deverá ser considerada conforme se:

* qualquer teste de segurança crítica falhar;
* qualquer invariante fundamental falhar;
* houver divergência determinística entre implementações;
* houver criação monetária inválida;
* uma assinatura inválida for aceita;
* uma transação inválida produzir estado válido;
* um Node não conseguir rejeitar dados protocolariamente inválidos.

---

# 6. Níveis de testes

A suíte deverá possuir:

```text
Unit Tests
    ↓
Integration Tests
    ↓
Protocol Tests
    ↓
Adversarial Tests
    ↓
Property Tests
    ↓
Fuzz Tests
    ↓
Interoperability Tests
    ↓
Recovery Tests
```

---

# 7. Identidade

## AT-ID-001 — Geração de identidade

**Specification:** `SPEC-ID-001` / `SPEC-ID-003`

### Entrada

Criar um novo Node.

### Esperado

O sistema deverá gerar:

* material privado;
* material público;
* Node ID.

### Aceitação

`PASS` se:

1. o material necessário for criado;
2. o Node ID puder ser derivado;
3. a chave privada não for transmitida;
4. a chave pública puder ser usada para verificação.

---

## AT-ID-002 — Determinismo do Node ID

**Specification:** `SPEC-ID-003`

### Entrada

A mesma chave pública.

### Operação

Derivar o Node ID duas ou mais vezes.

### Esperado

```text
NodeID(A) == NodeID(A)
```

### Aceitação

O resultado deverá ser idêntico em todas as execuções.

---

## AT-ID-003 — Chaves diferentes

### Entrada

Duas chaves públicas diferentes.

### Esperado

Os Node IDs deverão ser diferentes, salvo colisão criptográfica.

### Aceitação

O teste deverá falhar se IDs iguais forem produzidos para chaves distintas dentro das condições normais do teste.

---

## AT-ID-004 — Assinatura válida

**Specification:** `SPEC-ID-002`

### Entrada

```text
mensagem
chave privada
```

### Operação

Assinar e verificar.

### Esperado

```text
verify(public_key, message, signature) = TRUE
```

---

## AT-ID-005 — Assinatura adulterada

Alterar a mensagem depois da assinatura.

### Esperado

```text
verify(...) = FALSE
```

### Aceitação

Uma assinatura não poderá continuar válida depois que o conteúdo protegido for alterado.

---

## AT-ID-006 — Chave incorreta

Verificar uma assinatura com outra chave pública.

### Esperado

Rejeição.

---

# 8. Wallet

## AT-WAL-001 — Separação Wallet/Node

Criar:

```text
Wallet A
Node A
```

e não fornecer a chave privada da Wallet ao Node.

### Esperado

O Node deverá conseguir transmitir uma transação já assinada.

### Aceitação

O teste passa se:

```text
Wallet → assina
Node → transmite
Rede → valida
```

sem o Node possuir a chave privada.

---

## AT-WAL-002 — Node sem chave privada

Executar o Node sem acesso à chave privada da Wallet.

### Esperado

O Node deverá continuar capaz de:

* receber transações;
* transmitir transações;
* validar transações;
* participar das funções para as quais possui autorização.

---

## AT-WAL-003 — Assinatura local

Criar transação na Wallet.

### Esperado

A assinatura deverá ocorrer antes da transmissão.

---

# 9. Codificação canônica

## AT-CAN-001 — Mesmo objeto, mesma codificação

Construir o mesmo objeto protocolar em duas implementações.

### Esperado

```text
encode(A) == encode(B)
```

---

## AT-CAN-002 — Campos alterados

Alterar um campo relevante.

### Esperado

A codificação deverá mudar.

---

## AT-CAN-003 — Ordem não ambígua

Representar os mesmos dados através de diferentes ordens de entrada quando a estrutura lógica for equivalente.

### Esperado

A codificação canônica deverá produzir uma única representação definida pelo protocolo.

---

# 10. Transações

## AT-TX-001 — Transação válida

Criar uma transação válida.

### Esperado

```text
VALIDATE(tx) = ACCEPT
```

---

## AT-TX-002 — Transação malformada

Remover um campo obrigatório.

### Esperado

```text
VALIDATE(tx) = REJECT
```

---

## AT-TX-003 — Assinatura inválida

Modificar a assinatura.

### Esperado

Rejeição.

---

## AT-TX-004 — Conteúdo alterado

Assinar uma transação e depois alterar seu conteúdo.

### Esperado

Rejeição.

---

## AT-TX-005 — Taxa inválida

Criar uma transação com taxa incompatível com as regras.

### Esperado

Rejeição.

---

## AT-TX-006 — Estado incompatível

Criar uma transação que tente gastar recurso inexistente.

### Esperado

Rejeição.

---

## AT-TX-007 — Transação duplicada

Submeter a mesma transação válida duas vezes.

### Esperado

A segunda submissão não deverá gerar uma segunda alteração monetária.

---

# 11. Transaction ID

## AT-TID-001 — Determinismo

Calcular o ID da mesma transação várias vezes.

### Esperado

Todos os IDs deverão ser iguais.

---

## AT-TID-002 — Alteração relevante

Alterar um campo protegido.

### Esperado

O Transaction ID deverá mudar.

---

# 12. Double Spend

## AT-DS-001 — Duplo gasto simples

Estado:

```text
Wallet A = 10 ZERO
```

Criar:

```text
TX1 → 10 ZERO
TX2 → 10 ZERO
```

utilizando o mesmo recurso monetário.

### Esperado

As duas operações não poderão ser simultaneamente válidas no mesmo estado final.

---

## AT-DS-002 — Double spend concorrente

Enviar duas transações conflitantes para Nodes diferentes simultaneamente.

### Esperado

O consenso deverá produzir um estado único válido.

### Critério

Nunca:

```text
saldo gasto duas vezes
```

no estado final aceito.

---

# 13. Máquina de estado

## AT-STATE-001 — Transição válida

Aplicar um bloco válido ao estado.

### Esperado

```text
S1 = F(S0, B1)
```

produz estado válido.

---

## AT-STATE-002 — Transição inválida

Aplicar bloco contendo operação inválida.

### Esperado

O estado não deverá ser aceito.

---

## AT-STATE-003 — Determinismo

Executar:

```text
F(S0, B1)
```

em duas implementações.

### Esperado

```text
ResultA == ResultB
```

---

## AT-STATE-004 — Reexecução

Executar o mesmo bloco novamente contra o mesmo estado inicial.

### Esperado

O resultado deverá ser idêntico.

---

# 14. Integridade monetária

## AT-MONEY-001 — ZERO válido

Criar uma transação válida de transferência.

### Esperado

O total monetário deverá permanecer compatível com as regras de emissão e taxas.

---

## AT-MONEY-002 — Criação arbitrária

Tentar criar ZERO sem mecanismo autorizado.

### Esperado

Rejeição.

---

## AT-MONEY-003 — Saldo negativo

Tentar produzir:

```text
saldo < 0
```

### Esperado

Rejeição.

---

## AT-MONEY-004 — Alteração direta de estado

Modificar diretamente o saldo no armazenamento local sem uma transição protocolar válida.

### Esperado

O estado alterado deverá ser detectado como inconsistente ou rejeitado durante verificação.

---

# 15. Blocos

## AT-BLOCK-001 — Bloco válido

Criar bloco contendo somente operações válidas.

### Esperado

Aceitação.

---

## AT-BLOCK-002 — Bloco inválido

Inserir transação inválida.

### Esperado

Rejeição do bloco.

---

## AT-BLOCK-003 — Previous Block ID incorreto

Alterar a referência ao bloco anterior.

### Esperado

Rejeição.

---

## AT-BLOCK-004 — Bloco adulterado

Alterar uma transação de um bloco já referenciado.

### Esperado

A cadeia posterior deverá deixar de validar corretamente.

---

## AT-BLOCK-005 — Altura inconsistente

Criar bloco com altura incompatível.

### Esperado

Rejeição.

---

# 16. Genesis

## AT-GEN-001 — Genesis conhecido

Iniciar um Node novo.

### Esperado

O Node deverá reconhecer o Genesis correspondente à rede configurada.

---

## AT-GEN-002 — Genesis de rede diferente

Fornecer Genesis de outra rede.

### Esperado

O Node não deverá tratá-lo como Genesis válido da rede atual.

---

# 17. Consenso

Como o algoritmo de consenso ainda está **A DEFINIR**, os testes abaixo inicialmente serão testes de interface.

## AT-CON-001 — Determinação de estado

Fornecer candidatos válidos ao mecanismo de consenso.

### Esperado

O mecanismo deverá produzir resultado determinístico segundo as regras da implementação escolhida.

---

## AT-CON-002 — Candidato inválido

Fornecer candidato incompatível com as regras.

### Esperado

Rejeição.

---

## AT-CON-003 — Divergência

Executar a mesma entrada em múltiplos Nodes honestos.

### Esperado

Todos deverão chegar ao mesmo resultado quando as condições de consenso exigirem convergência.

---

# 18. Fork

## AT-FORK-001 — Fork concorrente

Criar duas versões concorrentes de um estado.

### Esperado

O mecanismo deverá aplicar suas regras determinísticas de resolução/finalização.

---

## AT-FORK-002 — Estado inválido em uma ramificação

Uma ramificação contém estado inválido.

### Esperado

Ela não poderá ser aceita como estado válido.

---

# 19. P2P

## AT-P2P-001 — Conexão válida

Dois Nodes compatíveis estabelecem comunicação.

### Esperado

A conexão deverá ser estabelecida.

---

## AT-P2P-002 — Versão incompatível

Conectar Nodes com versões incompatíveis.

### Esperado

A comunicação deverá seguir as regras de compatibilidade.

---

## AT-P2P-003 — Mensagem inválida

Enviar mensagem malformada.

### Esperado

Rejeição sem corrupção do estado.

---

## AT-P2P-004 — Mensagem excessivamente grande

Enviar mensagem acima do limite definido.

### Esperado

Rejeição antes de consumir recursos excessivos.

---

## AT-P2P-005 — Flooding

Enviar grande volume de mensagens inválidas.

### Esperado

O Node deverá aplicar mecanismo de limitação conforme os parâmetros definidos.

---

# 20. Sincronização

## AT-SYNC-001 — Node novo

Iniciar Node sem estado local.

### Esperado

O Node deverá:

1. encontrar pares;
2. identificar a rede;
3. obter estado;
4. verificar estado;
5. sincronizar;
6. participar.

---

## AT-SYNC-002 — Estado adulterado

Fornecer estado incorreto durante sincronização.

### Esperado

Rejeição.

---

## AT-SYNC-003 — Peer malicioso

Fornecer estado inválido a partir de um peer.

### Esperado

O Node não deverá aceitar o estado simplesmente porque veio de um peer conectado.

---

# 21. Determinismo

## AT-DET-001 — Mesmo estado

Executar a mesma transição em múltiplas instâncias.

### Esperado

Todos os resultados deverão ser iguais.

---

## AT-DET-002 — Mesma transação

Validar a mesma transação em múltiplos Nodes honestos.

### Esperado

Todos deverão produzir a mesma decisão de validade.

---

## AT-DET-003 — Mesmo bloco

Validar o mesmo bloco.

### Esperado

Todos os Nodes compatíveis deverão produzir a mesma decisão.

---

# 22. Privacidade

Os testes de privacidade deverão ser divididos em:

```text
Protocol Privacy Tests
+
Metadata Tests
+
Network Privacy Tests
+
Transaction Privacy Tests
```

A implementação criptográfica específica será definida posteriormente.

---

## AT-PRIV-001 — Ausência de identidade civil obrigatória

Criar Node usando somente identidade criptográfica.

### Esperado

O protocolo deverá permitir participação sem exigir:

* nome;
* CPF;
* endereço residencial;
* documento civil.

---

## AT-PRIV-002 — Minimização de metadados

Inspecionar mensagens protocolárias.

### Esperado

Campos não necessários não deverão ser transmitidos.

A lista exata de campos será definida em `PRIVACY.md`.

---

## AT-PRIV-003 — Chave privada

Monitorar mensagens do Node.

### Esperado

A chave privada não deverá aparecer em mensagens protocolárias.

---

# 23. ZERO

## AT-ZERO-001 — Transferência válida

Executar transferência válida.

### Esperado

Saldo final conforme as regras.

---

## AT-ZERO-002 — Soma monetária

Verificar o estado antes e depois de uma transferência.

### Esperado

A alteração monetária deverá corresponder exatamente às regras da transação e taxa.

---

## AT-ZERO-003 — Emissão não autorizada

Tentar produzir ZERO sem autorização.

### Esperado

Rejeição.

---

# 24. Taxas

## AT-FEE-001 — Taxa válida

Enviar transação com taxa válida.

### Esperado

Aceitação e aplicação correta da taxa.

---

## AT-FEE-002 — Taxa inválida

Enviar taxa incompatível com as regras.

### Esperado

Rejeição.

---

## AT-FEE-003 — Distribuição determinística

Executar a mesma operação em múltiplas implementações.

### Esperado

O resultado da taxa deverá ser idêntico.

---

# 25. Grande Mercado

Como a matemática definitiva ainda será especificada, os primeiros testes serão de segurança estrutural.

## AT-MKT-001 — Ordem válida

Criar uma ordem válida.

### Esperado

Aceitação.

---

## AT-MKT-002 — Ordem inválida

Criar ordem que viole as regras.

### Esperado

Rejeição.

---

## AT-MKT-003 — Cancelamento autorizado

Cancelar ordem utilizando autorização válida.

### Esperado

A ordem deverá deixar de estar disponível para execução.

---

## AT-MKT-004 — Cancelamento não autorizado

Tentar cancelar ordem pertencente a outra entidade sem autorização.

### Esperado

Rejeição.

---

# 26. Pool

## AT-POOL-001 — Depósito válido

Depositar ativo aceito pelo protocolo.

### Esperado

O ativo deverá ser contabilizado no Pool.

---

## AT-POOL-002 — Retirada administrativa

Tentar executar operação equivalente a:

```text
WITHDRAW_FROM_POOL_BY_ADMIN
```

### Esperado

A operação deverá ser inexistente ou rejeitada pela implementação compatível com a regra de permanência.

---

## AT-POOL-003 — Persistência

Depositar ativo e processar múltiplas transições.

### Esperado

O ativo deverá continuar contabilizado no Pool.

---

## AT-POOL-004 — Confusão ZERO/ativo externo

Tentar registrar ativo externo como ZERO.

### Esperado

Rejeição.

---

# 27. Governança

## AT-GOV-001 — Proposta válida

Criar proposta válida.

### Esperado

A proposta deverá receber identificador único e entrar no estado correto.

---

## AT-GOV-002 — Proposta malformada

Remover campo obrigatório.

### Esperado

Rejeição.

---

## AT-GOV-003 — Depósito insuficiente

Criar proposta sem cumprir eventual requisito de depósito.

### Esperado

Rejeição ou estado definido como inválido para votação.

---

## AT-GOV-004 — Voto fora do período

Enviar voto após o encerramento.

### Esperado

Rejeição.

---

## AT-GOV-005 — Voto duplicado

Enviar o mesmo voto mais de uma vez.

### Esperado

O resultado deverá obedecer exatamente às regras de votação definidas.

---

## AT-GOV-006 — Resultado determinístico

Executar a contagem em múltiplas implementações.

### Esperado

Resultado idêntico.

---

# 28. Sybil

O mecanismo definitivo ainda está **A DEFINIR**.

## AT-SYB-001 — Identidades múltiplas

Criar grande número de identidades criptográficas.

### Esperado

O teste deverá medir se o mecanismo anti-Sybil escolhido limita adequadamente o impacto.

O critério quantitativo será definido junto com `GOVERNANCE.md`/`CONSENSUS.md`.

---

## AT-SYB-002 — Sybil em governança

Simular criação de múltiplas identidades destinadas exclusivamente a manipular votação.

### Esperado

O sistema deverá impedir que o atacante obtenha influência ilimitada simplesmente criando identidades.

---

# 29. Reputação

## AT-REP-001 — Registro verificável

Produzir evento válido de reputação.

### Esperado

O evento deverá possuir origem verificável.

---

## AT-REP-002 — Falsificação

Tentar criar registro de reputação sem autorização válida.

### Esperado

Rejeição.

---

## AT-REP-003 — Reputação não permanente

Aplicar comportamento incompatível a Node anteriormente confiável.

### Esperado

O sistema deverá avaliar o comportamento atual conforme as regras definidas.

---

# 30. Penalização

## AT-PEN-001 — Evidência insuficiente

Tentar penalizar Node sem evidência protocolar suficiente.

### Esperado

A penalização deverá ser rejeitada ou não aplicada.

---

## AT-PEN-002 — Evidência válida

Fornecer evidência correspondente a uma violação definida.

### Esperado

A penalização deverá seguir exatamente a regra correspondente.

---

# 31. Defesa

## AT-DEF-001 — Anomalia

Simular comportamento anômalo sem evidência de ataque.

### Esperado

O sistema poderá entrar em Vigilância, mas não deverá automaticamente aplicar punição máxima.

---

## AT-DEF-002 — Incidente verificável

Simular comportamento comprovadamente incompatível.

### Esperado

O mecanismo de incidente deverá poder:

* limitar;
* isolar;
* registrar;
* preservar evidências.

---

## AT-DEF-003 — Falsa evidência

Fornecer evidência falsificada.

### Esperado

A evidência deverá ser rejeitada quando não puder ser validada.

---

# 32. Guerra Cibernética

## AT-CYBER-001 — Ativação

Simular condições que atendam aos critérios definidos para emergência.

### Esperado

A transição para o modo de defesa máximo deverá seguir regras verificáveis.

---

## AT-CYBER-002 — Ativação arbitrária

Tentar ativar emergência sem cumprir os critérios.

### Esperado

Rejeição.

---

## AT-CYBER-003 — Privilégio limitado

Conceder credencial defensiva.

### Esperado

A credencial deverá permitir somente operações dentro de seu escopo.

---

## AT-CYBER-004 — Expiração

Deixar uma credencial defensiva atingir o fim de sua validade.

### Esperado

A credencial não deverá continuar autorizando operações protegidas.

---

## AT-CYBER-005 — Revogação

Revogar credencial defensiva.

### Esperado

A autorização deverá deixar de funcionar.

---

## AT-CYBER-006 — Retaliação

Verificar a superfície de operações disponíveis ao mecanismo defensivo.

### Esperado

Não deverá existir uma operação protocolar destinada a:

* invadir sistema externo;
* instalar malware;
* roubar informações;
* destruir infraestrutura externa.

---

# 33. Encerramento de incidente

## AT-INC-001 — Encerramento válido

Simular incidente contido e sistemas recuperados.

### Esperado

O protocolo deverá permitir retorno ao estado normal conforme as regras.

---

## AT-INC-002 — Encerramento prematuro

Tentar encerrar incidente ainda ativo.

### Esperado

Rejeição.

---

## AT-INC-003 — Evidências preservadas

Encerrar incidente.

### Esperado

O encerramento não deverá apagar registros necessários à auditoria.

---

# 34. Navegador Zero

## AT-BRW-001 — Conexão válida

Navegador compatível conecta à Exonet.

### Esperado

Conexão protocolar válida.

---

## AT-BRW-002 — Interface não autorizada

Interface que não implementa o protocolo corretamente tenta conexão.

### Esperado

A sessão não deverá ser estabelecida como sessão válida da Exonet.

---

## AT-BRW-003 — Interface alternativa

Uma segunda implementação compatível do navegador acessa a mesma Comunidade.

### Esperado

Ambas deverão conseguir utilizar o protocolo conforme suas permissões.

---

# 35. Comunidades

## AT-COM-001 — Comunidade válida

Publicar Comunidade que atende aos requisitos.

### Esperado

Publicação válida.

---

## AT-COM-002 — Comunidade malformada

Publicar Comunidade que não atende ao protocolo.

### Esperado

Rejeição.

---

## AT-COM-003 — Isolamento

Simular Comunidade comprometida.

### Esperado

Ela não deverá conseguir alterar:

* consenso;
* saldo global;
* regras monetárias;
* outras Comunidades.

---

## AT-COM-004 — Replicação

Executar uma Comunidade em múltiplos Nodes compatíveis.

### Esperado

O estado deverá permanecer consistente conforme as regras da aplicação.

---

# 36. Atualizações

## AT-UPD-001 — Atualização válida

Fornecer atualização autenticada e compatível.

### Esperado

A atualização deverá ser reconhecida como válida.

---

## AT-UPD-002 — Atualização adulterada

Modificar o artefato depois de sua assinatura.

### Esperado

Rejeição.

---

## AT-UPD-003 — Versão incompatível

Fornecer versão incompatível.

### Esperado

O Node deverá aplicar as regras de compatibilidade, sem aceitar silenciosamente a versão.

---

## AT-UPD-004 — Atualização não autorizada

Fornecer atualização sem mecanismo de autenticação válido.

### Esperado

Rejeição.

---

# 37. Redes diferentes

## AT-NET-001 — Mainnet/Testnet

Tentar conectar dados de Testnet a um Node configurado para Mainnet.

### Esperado

Os dados não deverão ser tratados como pertencentes à Mainnet.

---

# 38. Isolamento

## AT-ISO-001 — Node comprometido

Simular Node malicioso.

### Esperado

Os demais Nodes honestos deverão continuar capazes de validar suas próprias operações conforme o protocolo.

---

## AT-ISO-002 — Estado local corrompido

Corromper estado local de um Node.

### Esperado

O Node deverá detectar inconsistência ou reconstruir estado verificável.

---

# 39. Recuperação

## AT-REC-001 — Reinício

Desligar e reiniciar um Node.

### Esperado

O Node deverá recuperar seu estado válido.

---

## AT-REC-002 — Perda de dados locais

Remover estado local recuperável.

### Esperado

O Node deverá conseguir reconstruir o estado a partir das fontes protocolarmente válidas disponíveis.

---

## AT-REC-003 — Peer malicioso durante recuperação

Fornecer dados inválidos.

### Esperado

O Node não deverá aceitar dados inválidos apenas para concluir a sincronização.

---

# 40. Disponibilidade

## AT-AVL-001 — Perda de Nodes

Desligar uma parte dos Nodes.

### Esperado

A rede deverá continuar operando se as condições mínimas do consenso forem mantidas.

---

## AT-AVL-002 — Node individual

Remover um Node importante.

### Esperado

A rede não deverá depender exclusivamente dele.

---

# 41. Ausência de autoridade absoluta

## AT-AUTH-001 — Tentativa de administração global

Tentar executar uma operação capaz de:

* alterar qualquer saldo;
* modificar qualquer regra;
* confiscar qualquer ativo;
* substituir o consenso;

utilizando uma única identidade administrativa.

### Esperado

Não deverá existir tal mecanismo na implementação compatível.

---

## AT-AUTH-002 — Chave-mestra

Auditar interfaces privilegiadas.

### Esperado

Não deverá existir uma chave única com poder permanente e irrestrito sobre a rede.

---

# 42. Independência de implementação

## AT-INT-001 — Duas implementações

Executar duas implementações independentes com os mesmos vetores de teste.

### Esperado

Resultados protocolários equivalentes.

---

## AT-INT-002 — Estado

Fornecer o mesmo estado e bloco às duas implementações.

### Esperado

Estados finais idênticos.

---

## AT-INT-003 — Transação

Validar o mesmo conjunto de transações.

### Esperado

Mesmas decisões de validade.

---

# 43. Testes de propriedades

Além dos testes fixos, a implementação deverá possuir testes baseados em propriedades.

Exemplo:

```text
Para qualquer transação tx:

se tx é inválida
→ tx nunca produz estado válido
```

---

## AT-PROP-001 — Invariância monetária

Para qualquer sequência válida de operações:

```text
Estado monetário final
```

deverá respeitar a política monetária.

---

## AT-PROP-002 — Assinaturas

Para qualquer mensagem:

```text
alterar mensagem
→ assinatura original não deve validar
```

---

## AT-PROP-003 — IDs

Para qualquer objeto:

```text
mesmo objeto
→ mesmo ID
```

---

## AT-PROP-004 — Estado

Para qualquer bloco válido:

```text
mesmo estado + mesmo bloco
→ mesmo resultado
```

---

# 44. Fuzzing

Componentes que processam entrada externa deverão possuir testes de fuzzing.

Alvos prioritários:

* parser de transações;
* parser de blocos;
* mensagens P2P;
* serialização;
* desserialização;
* scripts de governança;
* dados de Comunidades;
* mensagens de sincronização.

---

## AT-FUZZ-001 — Parser

Gerar entradas arbitrárias.

### Esperado

O programa deverá:

* rejeitar entradas inválidas;
* não corromper estado;
* não aceitar dados inválidos;
* não apresentar comportamento indefinido.

---

## AT-FUZZ-002 — Mensagens P2P

Enviar dados aleatórios.

### Esperado

O Node deverá permanecer operacional.

---

# 45. Testes de carga

A implementação deverá ser submetida a cargas progressivamente maiores.

Medir:

* CPU;
* memória;
* armazenamento;
* largura de banda;
* latência;
* taxa de processamento.

Nenhum limite definitivo deverá ser inventado antes dos benchmarks.

---

# 46. Testes de falha

A rede deverá ser submetida a:

* perda de conexão;
* perda de Nodes;
* mensagens atrasadas;
* mensagens duplicadas;
* mensagens fora de ordem;
* partições;
* reinicialização;
* corrupção de dados.

---

# 47. Testes de segurança

Deverão existir testes contra pelo menos:

```text
Double Spend
Sybil
Eclipse
DoS
Flooding
Invalid State
Invalid Signature
Forged Identity
Malformed Data
Governance Manipulation
Pool Manipulation
Update Tampering
```

---

# 48. Testes de recuperação

Todo componente crítico deverá responder à pergunta:

> **O que acontece depois que ele falha?**

Para cada componente:

```text
Falha
 ↓
Detecção
 ↓
Contenção
 ↓
Recuperação
 ↓
Verificação
```

deverá ser testado.

---

# 49. Testes de regressão

Toda correção de bug deverá gerar um teste de regressão.

Regra:

```text
Bug encontrado
     ↓
Correção
     ↓
Teste reproduz o bug
     ↓
Teste passa após correção
     ↓
Teste permanece na suíte
```

---

# 50. Testes de compatibilidade

Cada nova versão deverá executar a suíte de compatibilidade contra:

* versão anterior;
* vetores históricos;
* estados conhecidos;
* transações conhecidas;
* blocos conhecidos.

---

# 51. Vetores de teste

A Rede Zero deverá manter vetores determinísticos.

Exemplo:

```text
tests/
├── vectors/
│   ├── identity/
│   ├── transactions/
│   ├── blocks/
│   ├── state/
│   ├── p2p/
│   ├── governance/
│   └── security/
```

Um vetor deverá possuir:

```text
input
expected_output
expected_status
protocol_version
```

---

# 52. Teste de conformidade

Uma implementação poderá declarar conformidade somente se:

```text
Core Tests       = PASS
Security Tests   = PASS
Determinism      = PASS
Compatibility    = PASS
Critical Invariants = PASS
```

Testes não aplicáveis deverão possuir justificativa.

---

# 53. Testes bloqueados

Quando uma especificação ainda estiver indefinida:

```text
STATUS = BLOCKED
```

deverá ser utilizado.

Exemplo:

```text
AT-SYB-001
BLOCKED
Reason:
algoritmo anti-Sybil ainda não definido.
```

Isso evita transformar uma decisão futura em uma falsa aprovação.

---

# 54. Matriz de rastreabilidade

Cada requisito deverá possuir ligação com:

```text
Requirement
    ↓
Threat
    ↓
Specification
    ↓
Acceptance Test
    ↓
Implementation
```

Exemplo:

```text
REQ-021
   ↓
T-002
   ↓
SPEC-034
   ↓
AT-MONEY-002
   ↓
implementation/test_money_creation
```

---

# 55. Critérios críticos

Os seguintes testes serão classificados como **CRITICAL**:

* AT-ID-005;
* AT-TX-003;
* AT-TX-006;
* AT-DS-001;
* AT-STATE-002;
* AT-MONEY-002;
* AT-MONEY-003;
* AT-BLOCK-003;
* AT-BLOCK-004;
* AT-SYNC-002;
* AT-AUTH-001;
* AT-AUTH-002;
* AT-UPD-002;
* AT-UPD-004;
* AT-ISO-001.

Uma falha crítica impede uma release considerada estável.

---

# 56. Critério de release

Uma versão candidata à release deverá satisfazer:

```text
              ┌────────────────────┐
              │   Release Candidate│
              └─────────┬──────────┘
                        │
          ┌─────────────┼─────────────┐
          ▼             ▼             ▼
       Unit Tests   Integration    Security
          │             │             │
          └─────────────┼─────────────┘
                        ▼
                 Protocol Tests
                        │
                        ▼
                  Determinism
                        │
                        ▼
                   Fuzz Tests
                        │
                        ▼
                Compatibility
                        │
                        ▼
                    RELEASE
```

---

# 57. Proibição de aprovação manual de segurança

Um desenvolvedor não deverá poder marcar um teste crítico como aprovado simplesmente alterando seu resultado.

Resultados deverão ser produzidos automaticamente sempre que tecnicamente possível.

Exceções deverão possuir justificativa registrada.

---

# 58. Evidência de execução

Cada execução da suíte deverá registrar:

* versão do código;
* versão do protocolo;
* sistema de execução;
* configuração relevante;
* timestamp;
* testes executados;
* resultados;
* falhas;
* logs necessários.

---

# 59. Reprodutibilidade

Outro desenvolvedor deverá conseguir reproduzir um teste utilizando:

* código;
* vetor de entrada;
* configuração;
* versão;
* instruções documentadas.

---

# 60. Testes antes da Mainnet

Antes de uma eventual Mainnet:

```text
Unit
 ↓
Integration
 ↓
Devnet
 ↓
Testnet
 ↓
Security testing
 ↓
Adversarial testing
 ↓
Independent review
 ↓
Release Candidate
 ↓
Mainnet
```

Nenhuma passagem de fase deverá ocorrer apenas por decisão pessoal.

---

# 61. Critério de falha

Uma implementação deverá ser considerada não conforme quando:

* aceitar uma operação que deveria rejeitar;
* rejeitar uma operação obrigatória válida;
* produzir estado divergente;
* criar ZERO fora das regras;
* aceitar assinatura inválida;
* aceitar bloco inválido;
* permitir autoridade proibida;
* comprometer uma propriedade crítica de privacidade;
* falhar em uma propriedade crítica de segurança.

---

# 62. Critério de correção

Quando um teste falhar:

```text
FAIL
 ↓
Investigar
 ↓
Classificar
 ↓
Corrigir
 ↓
Criar/regenerar teste
 ↓
Executar regressão
 ↓
PASS
```

Uma correção não deverá remover o teste simplesmente para obter `PASS`.

---

# 63. Cobertura mínima inicial

A suíte deverá buscar cobertura dos seguintes domínios:

| Domínio        | Cobertura         |
| -------------- | ----------------- |
| Identidade     | Obrigatória       |
| Wallet         | Obrigatória       |
| Transações     | Obrigatória       |
| Estado         | Obrigatória       |
| Blocos         | Obrigatória       |
| Consenso       | Interface inicial |
| P2P            | Obrigatória       |
| ZERO           | Obrigatória       |
| Privacidade    | Inicial           |
| Governança     | Inicial           |
| Reputação      | Inicial           |
| Defesa         | Inicial           |
| Exonet         | Inicial           |
| Comunidades    | Inicial           |
| Grande Mercado | Inicial           |
| Pool           | Inicial           |
| Atualizações   | Obrigatória       |

“Obrigatória” significa que não deve existir componente implementado sem testes correspondentes.

---

# 64. Automação

Os testes deverão ser integrados ao sistema de desenvolvimento.

Fluxo:

```text
Pull Request
     ↓
Build
     ↓
Unit Tests
     ↓
Integration Tests
     ↓
Protocol Tests
     ↓
Security Tests
     ↓
PASS / FAIL
```

Uma alteração que quebre testes críticos não deverá ser considerada pronta para integração.

---

# 65. CI

O ambiente de integração contínua deverá executar automaticamente:

* compilação;
* testes;
* lint;
* análise estática;
* testes de protocolo;
* testes de segurança disponíveis;
* testes de determinismo.

---

# 66. Testes de múltiplas implementações

Quando existir mais de uma implementação:

```text
Implementation A
       │
       ├── Test vectors
       │
       ▼
Expected results
       ▲
       │
       ├── Test vectors
       │
Implementation B
```

Diferenças deverão ser investigadas.

---

# 67. Testes de interoperabilidade

Dois componentes compatíveis deverão conseguir:

* trocar mensagens;
* validar transações;
* validar blocos;
* sincronizar;
* produzir estados compatíveis.

---

# 68. Testes de desastre

Antes da Mainnet deverão ser simulados cenários como:

### Cenário A

Grande parte dos Nodes fica offline.

### Cenário B

Uma parcela dos Nodes passa a agir maliciosamente.

### Cenário C

Uma implementação apresenta bug.

### Cenário D

Uma dependência externa desaparece.

### Cenário E

O repositório principal deixa de existir.

### Cenário F

O fundador deixa de participar.

### Critério

A rede deverá continuar existindo ou possuir caminho documentado de recuperação sem depender de uma única pessoa ou infraestrutura.

---

# 69. Teste de independência do GitHub

Simular indisponibilidade do repositório principal.

### Esperado

O código e a especificação deverão continuar recuperáveis através de cópias, forks ou outras formas documentadas de distribuição.

---

# 70. Teste de continuidade

Simular:

```text
Fundador = indisponível
```

### Esperado

A documentação deverá permitir que outros contribuidores:

* compilem;
* testem;
* entendam;
* modifiquem;
* continuem o projeto.

---

# 71. Teste de ausência de administrador

Auditar o código e o protocolo procurando uma identidade privilegiada permanente.

### Critério

Não deverá existir uma entidade capaz de:

```text
alterar saldos arbitrariamente
+
alterar consenso unilateralmente
+
confiscar fundos
+
modificar regras sem processo protocolar
```

---

# 72. Teste de menor privilégio

Para cada componente:

```text
Componente
    ↓
listar permissões
    ↓
verificar necessidade
    ↓
remover permissões desnecessárias
```

### Critério

Nenhum privilégio deverá existir apenas por conveniência arquitetural sem justificativa documentada.

---

# 73. Teste de isolamento de Comunidade

Uma Comunidade maliciosa deverá tentar:

* alterar saldo;
* modificar consenso;
* alterar outra Comunidade;
* criar ZERO;
* alterar governança.

### Esperado

Todas as operações fora de seu escopo deverão ser rejeitadas.

---

# 74. Teste de isolamento de Node

Um Node malicioso deverá tentar:

* falsificar blocos;
* criar ZERO;
* falsificar votos;
* falsificar reputação;
* modificar estado remoto.

### Esperado

Outros Nodes não deverão aceitar essas operações sem provas válidas.

---

# 75. Teste de atualização maliciosa

Simular comprometimento do mecanismo de distribuição.

### Esperado

Uma atualização sem autenticação/identificação válida deverá ser rejeitada.

---

# 76. Teste de downgrade

Tentar substituir uma versão atual por uma versão incompatível ou não autorizada.

### Esperado

O comportamento deverá seguir as regras de compatibilidade e segurança do protocolo.

---

# 77. Teste de corrupção

Modificar dados persistidos localmente.

### Esperado

A implementação deverá:

* detectar inconsistência;
* rejeitar dados inválidos;
* reconstruir estado quando possível.

---

# 78. Testes de tempo

Componentes que dependam de tempo deverão ser testados contra:

* timestamps inválidos;
* relógio incorreto;
* mensagens atrasadas;
* mensagens futuras;
* reordenação.

O protocolo não deverá depender cegamente do relógio local de um único Node.

---

# 79. Testes de armazenamento

Testar:

* falta de espaço;
* corrupção;
* reinicialização;
* leitura parcial;
* escrita interrompida;
* recuperação.

---

# 80. Testes de recursos

Testar limites de:

* memória;
* CPU;
* armazenamento;
* conexões;
* tamanho de mensagens.

A implementação deverá falhar de maneira controlada quando recursos forem insuficientes.

---

# 81. Segurança contra entradas maliciosas

Toda entrada externa deverá ser considerada não confiável até validação.

Modelo:

```text
Entrada externa
      ↓
Parsing seguro
      ↓
Validação
      ↓
Autorização
      ↓
Processamento
```

Nunca:

```text
Entrada externa
      ↓
Execução direta
```

---

# 82. Critério para novas funcionalidades

Toda nova funcionalidade deverá adicionar:

```text
Nova funcionalidade
       ↓
Nova especificação
       ↓
Novo teste
       ↓
Novo teste negativo
       ↓
Teste de regressão
```

Uma funcionalidade sem teste correspondente não deverá ser considerada concluída.

---

# 83. Critério para correções de segurança

Toda vulnerabilidade confirmada deverá produzir:

1. correção;
2. teste reproduzindo a vulnerabilidade;
3. teste confirmando a correção;
4. análise de componentes afetados;
5. atualização do `THREAT_MODEL.md`, quando necessário.

---

# 84. Relação com o Threat Model

Quando um novo ataque for descoberto:

```text
Novo ataque
     ↓
THREAT_MODEL.md
     ↓
Especificação afetada
     ↓
Novo Acceptance Test
     ↓
Implementação
```

O teste deverá impedir que a mesma vulnerabilidade volte a existir silenciosamente.

---

# 85. Relação com Requirements

Cada requisito implementado deverá possuir pelo menos um caminho:

```text
REQ
 ↓
SPEC
 ↓
TEST
```

Requisitos críticos poderão possuir múltiplos testes.

---

# 86. Matriz inicial

| Requisito | Especificação | Teste        |
| --------- | ------------- | ------------ |
| REQ-009   | SPEC-ID       | AT-ID-001    |
| REQ-010   | SPEC-ID       | AT-PRIV-001  |
| REQ-011   | SPEC-WAL      | AT-WAL-001   |
| REQ-012   | SPEC-ID/WAL   | AT-PRIV-003  |
| REQ-015   | SPEC-CON      | AT-CON-001   |
| REQ-016   | SPEC-STATE    | AT-STATE-003 |
| REQ-018   | SPEC-STATE    | AT-STATE-002 |
| REQ-019   | SPEC-ZERO     | AT-ZERO-001  |
| REQ-021   | SPEC-MONEY    | AT-MONEY-002 |
| REQ-023   | SPEC-PRIV     | AT-PRIV-002  |
| REQ-029   | SPEC-WAL      | AT-WAL-001   |
| REQ-032   | SPEC-MKT      | AT-MKT-001   |
| REQ-036   | SPEC-POOL     | AT-POOL-001  |
| REQ-037   | SPEC-POOL     | AT-POOL-003  |
| REQ-040   | SPEC-GOV      | AT-GOV-001   |
| REQ-044   | SPEC-SYB      | AT-SYB-001   |
| REQ-046   | SPEC-REP      | AT-REP-001   |
| REQ-049   | SPEC-PEN      | AT-PEN-001   |
| REQ-056   | SPEC-DEF      | AT-CYBER-006 |
| REQ-058   | SPEC-DEF      | AT-CYBER-004 |
| REQ-059   | SPEC-DEF      | AT-CYBER-005 |
| REQ-063   | SPEC-DEF      | AT-INC-001   |
| REQ-064   | SPEC-BRW      | AT-BRW-001   |
| REQ-068   | SPEC-COM      | AT-COM-001   |
| REQ-077   | Development   | CI           |
| REQ-078   | Acceptance    | AT-*         |
| REQ-089   | Architecture  | AT-AVL-002   |
| REQ-096   | Continuity    | AT-REC-003   |

A matriz completa deverá crescer junto com as especificações especializadas.

---

# 87. Definição de pronto

Uma tarefa técnica somente deverá ser marcada como **DONE** quando:

```text
Código
  +
Teste positivo
  +
Teste negativo
  +
Regressão
  +
Documentação
```

estiverem concluídos.

---

# 88. Definição de pronto para protocolo

Uma especificação protocolar somente deverá ser considerada estável quando:

```text
Especificação
      ↓
Implementação
      ↓
Testes
      ↓
Fuzzing
      ↓
Ataques simulados
      ↓
Interoperabilidade
      ↓
Auditoria
      ↓
Aprovação
```

---

# 89. Estado atual

**ACCEPTANCE_TESTS.md v0.1.0**

Status:

> **Suíte inicial de critérios de aceitação — não congelada.**

Diversos testes permanecem `BLOCKED` porque as especificações correspondentes ainda não foram definidas.

Isso é intencional.

Não devemos transformar decisões ainda desconhecidas em critérios artificiais.

---

# 90. Próxima etapa

A próxima etapa recomendada é iniciar as especificações especializadas.

Primeiro:

```text
spec/CRYPTOGRAPHY.md
```

Depois:

```text
spec/IDENTITY.md
spec/WALLET.md
spec/TRANSACTIONS.md
spec/STATE.md
spec/BLOCKS.md
spec/P2P.md
spec/CONSENSUS.md
```

À medida que essas especificações forem congeladas, os testes `BLOCKED` correspondentes deverão ser transformados em testes executáveis.

---

# 91. Regra final

A Rede Zero não deverá considerar uma propriedade verdadeira simplesmente porque foi escrita em um documento.

A propriedade deverá ser:

```text
definida
   ↓
implementada
   ↓
testada
   ↓
reproduzível
   ↓
verificada
```

> **Se não pode ser testado, ainda não está suficientemente especificado.**

---

**Status final:** `ACCEPTANCE_TESTS.md v0.1.0`
