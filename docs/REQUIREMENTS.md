# Rede Zero — Requisitos do Sistema

**Documento:** `REQUIREMENTS.md`
**Projeto:** Rede Zero / Exonet
**Versão:** `0.2.0`
**Status:** Documento de requisitos — fase de definição
**Natureza:** Especificação conceitual e funcional

---

# 1. Objetivo

Este documento define os requisitos fundamentais que a Rede Zero deverá atender para que sua implementação seja compatível com os princípios estabelecidos no Manifesto Exonet.

O documento define **o que a Rede Zero deve ser capaz de fazer ou garantir**.

As decisões sobre **como implementar** cada requisito serão definidas posteriormente em documentos de arquitetura, especificações técnicas, propostas de decisão e protótipos.

Este documento, portanto, não determina antecipadamente:

* linguagem de programação;
* algoritmo de consenso;
* biblioteca criptográfica;
* banco de dados;
* arquitetura definitiva de armazenamento;
* protocolo de transporte;
* modelo matemático do Grande Mercado;
* mecanismo definitivo de governança.

Essas decisões serão tomadas em etapas posteriores.

---

# 2. Princípios fundamentais

## REQ-001 — Descentralização

A Rede Zero deve funcionar de forma descentralizada, sem depender de uma autoridade central para sua operação cotidiana.

## REQ-002 — Continuidade

A continuidade da Rede Zero não deve depender de uma pessoa, grupo, empresa, servidor ou plataforma específica.

## REQ-003 — Código aberto

O código-fonte, os protocolos e as especificações fundamentais da Rede Zero devem ser públicos e auditáveis.

## REQ-004 — Reprodutibilidade

Um participante deve ser capaz de obter o código e as especificações necessárias para executar, estudar ou contribuir com a Rede Zero sem depender de conhecimento privado mantido por uma única pessoa.

## REQ-005 — Ausência de autoridade absoluta

Nenhuma chave, identidade, componente ou participante deve possuir capacidade unilateral permanente de alterar as regras fundamentais, modificar arbitrariamente o estado da rede, confiscar fundos ou assumir controle absoluto da Rede Zero.

---

# 3. Exonet

## REQ-006 — Rede sobre a Internet

A Exonet deve utilizar a infraestrutura da Internet como meio de transporte, mantendo seus próprios protocolos, identidades, regras e serviços.

## REQ-007 — Separação entre Internet e Exonet

A conexão à Internet não deve significar automaticamente participação na Exonet.

A participação deve depender da implementação dos protocolos da Rede Zero e dos mecanismos de autenticação correspondentes.

## REQ-008 — Participação baseada em protocolo

A Rede Zero deve determinar a participação por meio de regras de protocolo e mecanismos criptográficos, e não pela marca do navegador, sistema operacional ou equipamento utilizado.

---

# 4. Nodes

## REQ-009 — Identidade criptográfica

Cada Node deve possuir uma identidade criptográfica própria.

## REQ-010 — Separação entre identidade criptográfica e identidade civil

A identidade criptográfica de um Node não deve exigir o conhecimento da identidade civil de seu operador.

## REQ-011 — Separação entre Node e carteira

A arquitetura deve permitir que uma carteira seja utilizada sem entregar suas chaves privadas ao Node.

## REQ-012 — Proteção das chaves privadas

As chaves privadas utilizadas para controlar fundos ou identidades não devem ser transmitidas desnecessariamente pela rede.

## REQ-013 — Participação distribuída

A arquitetura deve permitir diferentes tipos de participantes e Nodes conforme as funções necessárias ao funcionamento da rede.

---

# 5. Blockchain e estado da rede

## REQ-014 — Estado compartilhado

A Rede Zero deve possuir um mecanismo distribuído para manter um estado compartilhado da rede.

## REQ-015 — Consenso

A Rede Zero deve possuir um mecanismo de consenso capaz de determinar, de maneira verificável, qual estado da rede é válido.

## REQ-016 — Verificabilidade

As regras utilizadas para validar transações, blocos e alterações do estado da rede devem poder ser verificadas independentemente pelos participantes.

## REQ-017 — Resistência a falhas

A rede deve continuar funcionando diante da indisponibilidade ou do comportamento incorreto de uma parcela dos Nodes, dentro dos limites definidos pelo seu modelo de segurança.

## REQ-018 — Rejeição de estado inválido

Os Nodes devem ser capazes de rejeitar dados que violem as regras do protocolo.

---

# 6. ZERO

## REQ-019 — Unidade monetária nativa

**ZERO** deve ser a unidade monetária nativa da Rede Zero.

## REQ-020 — Exclusividade monetária nativa

ZERO deve ser a única unidade monetária nativa reconhecida pelo protocolo da Rede Zero.

Ativos provenientes de redes externas, quando suportados, devem permanecer identificáveis como ativos externos e não poderão ser transformados em novas moedas nativas da Rede Zero.

## REQ-021 — Integridade monetária

A quantidade e a movimentação de ZERO devem obedecer exclusivamente às regras monetárias definidas pelo protocolo.

## REQ-022 — Verificação monetária

Os participantes devem ser capazes de verificar independentemente a validade da criação, emissão, transferência, destruição ou utilização de ZERO, conforme as regras monetárias do protocolo.

---

# 7. Privacidade

## REQ-023 — Privacidade por padrão

A arquitetura da Rede Zero deve buscar minimizar a exposição de informações dos participantes desde sua concepção.

## REQ-024 — Privacidade das transações

O protocolo deve buscar proteger, tanto quanto tecnicamente possível, informações sobre:

* remetentes;
* destinatários;
* valores;
* relacionamentos entre transações;
* histórico financeiro;
* outros metadados transacionais relevantes.

## REQ-025 — Minimização de metadados

A rede deve minimizar a quantidade de metadados necessários para seu funcionamento.

## REQ-026 — Privacidade de rede

A arquitetura deve buscar reduzir a possibilidade de associar diretamente uma atividade da Rede Zero ao endereço de rede ou localização de rede de um participante.

## REQ-027 — Ausência de registro civil central

A Rede Zero não deve manter um banco de dados central obrigatório que associe identidades criptográficas às identidades civis dos participantes.

## REQ-028 — Não promessa de anonimato absoluto

A documentação da Rede Zero deve distinguir claramente entre privacidade proporcionada pelo protocolo e anonimato absoluto.

A Rede Zero não deve afirmar que nenhuma forma de análise externa poderá identificar ou correlacionar um participante.

---

# 8. Carteiras

## REQ-029 — Controle pelo usuário

O usuário deve possuir controle direto sobre as chaves necessárias para controlar seus fundos.

## REQ-030 — Não custódia obrigatória

A arquitetura não deve exigir que o usuário entregue suas chaves privadas a uma entidade central para utilizar a Rede Zero.

## REQ-031 — Segurança e recuperação

A arquitetura da carteira deverá possuir mecanismos adequados para proteção, backup e recuperação das credenciais, definidos em especificação própria.

---

# 9. Grande Mercado

## REQ-032 — DEX nativa

A Rede Zero deve possuir um mecanismo nativo e descentralizado de negociação denominado **Grande Mercado**.

## REQ-033 — Grande Mercado como protocolo

O Grande Mercado deve ser uma função do protocolo/ecossistema da Rede Zero e não deve depender exclusivamente de um website ou servidor específico.

## REQ-034 — Múltiplas interfaces

As funcionalidades do Grande Mercado devem poder ser acessadas por diferentes interfaces compatíveis com o protocolo.

## REQ-035 — Ausência de autoridade central de negociação

Nenhuma entidade isolada deve possuir controle permanente sobre todas as operações do Grande Mercado.

---

# 10. Pool permanente

## REQ-036 — Pool único

A Rede Zero deve possuir um único **Pool permanente**, conforme definido pelo protocolo.

## REQ-037 — Permanência dos ativos

Os ativos depositados no Pool devem permanecer no Pool de acordo com as regras fundamentais estabelecidas para esse mecanismo.

## REQ-038 — Ausência de retirada administrativa

Nenhum administrador, Node ou entidade individual deve possuir mecanismo privilegiado capaz de retirar unilateralmente ativos do Pool.

## REQ-039 — Ativos externos

Quando houver suporte a ativos externos, o protocolo deverá definir mecanismos verificáveis para sua representação, validação e utilização.

---

# 11. Governança

## REQ-040 — Governança comunitária

Alterações fundamentais da Rede Zero devem depender dos mecanismos de governança definidos pelo protocolo e da participação da comunidade.

## REQ-041 — Propostas

Deve existir um mecanismo para criação, discussão e votação de propostas.

## REQ-042 — Prevenção de spam

O sistema de propostas deve possuir mecanismos para reduzir a criação abusiva, automatizada ou maliciosa de propostas.

## REQ-043 — Votação verificável

Os resultados das votações devem poder ser verificados independentemente.

## REQ-044 — Resistência a Sybil

O modelo de governança deve considerar o risco de um participante criar múltiplas identidades para obter influência desproporcional.

## REQ-045 — Limitação de concentração

O modelo de governança deve considerar os riscos de concentração de poder decorrentes da posse de grandes quantidades de ZERO.

---

# 12. Reputação

## REQ-046 — Identidade de Node

A reputação deve estar associada à identidade criptográfica do Node, e não a uma impressão digital permanente do computador físico.

## REQ-047 — Histórico verificável

O sistema deve permitir o registro verificável de determinados eventos relevantes para a reputação.

## REQ-048 — Comportamento atual

Uma reputação histórica positiva não deve conceder imunidade permanente contra comportamentos futuros.

## REQ-049 — Penalização objetiva

Penalizações devem depender de comportamentos tecnicamente verificáveis e definidos pelo protocolo.

## REQ-050 — Pesquisa não é ataque

Estudar, auditar, compilar, testar, fazer fuzzing ou pesquisar o protocolo não deve ser considerado automaticamente comportamento malicioso.

---

# 13. Segurança

## REQ-051 — Segurança em camadas

A Rede Zero deve utilizar múltiplas camadas independentes de segurança.

## REQ-052 — Detecção de anomalias

A rede deve possuir mecanismos para identificar comportamentos anormais.

## REQ-053 — Isolamento

A arquitetura deve permitir isolar participantes ou componentes que apresentem comportamento comprovadamente incompatível com o protocolo.

## REQ-054 — Preservação de evidências

Durante incidentes de segurança, informações relevantes para análise posterior devem poder ser preservadas de maneira verificável.

## REQ-055 — Recuperação

A Rede Zero deve possuir mecanismos para recuperação após incidentes de segurança.

---

# 14. Defesa da Exonet

## REQ-056 — Defesa, não retaliação

Os mecanismos de defesa da Rede Zero devem ter como objetivo proteger a rede e seus participantes, e não atacar sistemas externos.

## REQ-057 — Modos de defesa

A arquitetura deve permitir diferentes níveis de resposta defensiva conforme a gravidade e as evidências de um incidente.

## REQ-058 — Emergência limitada

Mecanismos de emergência devem possuir:

* critérios de ativação;
* limites de atuação;
* mecanismos de supervisão;
* critérios de encerramento.

## REQ-059 — Ausência de poder permanente de emergência

Nenhum mecanismo criado para responder a uma emergência deve conceder controle permanente sobre a rede.

## REQ-060 — Defesa distribuída

A defesa da rede deve poder contar com múltiplos participantes independentes.

## REQ-061 — Credenciais defensivas temporárias

Participantes que atendam aos critérios definidos pelo protocolo poderão receber capacidades adicionais e temporárias durante incidentes.

Essas capacidades devem ser limitadas ao necessário para a função defensiva autorizada.

## REQ-062 — Contribuição defensiva verificável

A participação em um incidente não deve ser suficiente, por si só, para gerar recompensas ou privilégios.

Qualquer recompensa ou registro de contribuição defensiva deve depender de contribuição verificável.

## REQ-063 — Encerramento de incidentes

A rede deve possuir mecanismo para determinar quando um incidente terminou e permitir o retorno gradual ao funcionamento normal.

---

# 15. Navegador Zero

## REQ-064 — Interface da Exonet

A Rede Zero deve possuir uma interface dedicada para acesso às aplicações da Exonet, denominada **Navegador Zero**.

## REQ-065 — Navegador como interface

O Navegador Zero deve funcionar como interface para os protocolos da Exonet e não como autoridade central da rede.

## REQ-066 — Independência da interface

A Exonet não deve depender de uma única implementação do Navegador Zero.

## REQ-067 — Identidade criptográfica

O acesso aos recursos da Exonet deve utilizar mecanismos de identidade e autenticação compatíveis com o protocolo.

---

# 16. Comunidades

## REQ-068 — Comunidades descentralizadas

A Exonet deve permitir a criação de ambientes denominados **Comunidades**.

## REQ-069 — Protocolo de comunidades

As Comunidades devem utilizar protocolos compatíveis com a Rede Zero.

## REQ-070 — Distribuição

A arquitetura deve permitir que uma Comunidade continue funcionando mesmo quando participantes individuais ficam indisponíveis, dentro dos limites definidos para sua arquitetura.

## REQ-071 — Criação comunitária

Participantes devem poder propor novas Comunidades.

## REQ-072 — Validação

Comunidades oficiais devem passar pelos mecanismos de validação definidos pelo projeto.

## REQ-073 — Regras objetivas

As regras aplicadas às Comunidades devem ser públicas, claras e verificáveis.

## REQ-074 — Ausência de poder arbitrário

Nenhuma entidade individual deve possuir autoridade arbitrária e permanente para remover Comunidades ou participantes sem seguir as regras estabelecidas pelo protocolo.

---

# 17. Desenvolvimento aberto

## REQ-075 — Repositório público

O desenvolvimento da Rede Zero deve ser realizado de maneira pública e auditável.

## REQ-076 — Contribuições externas

Qualquer pessoa deve poder estudar o projeto e propor contribuições de acordo com as regras do repositório.

## REQ-077 — Revisão de código

Alterações importantes no código devem possuir mecanismos de revisão e validação.

## REQ-078 — Testes automatizados

Componentes críticos devem possuir testes automatizados.

## REQ-079 — Histórico das decisões

Decisões técnicas importantes devem possuir documentação sobre:

* contexto;
* problema;
* alternativas consideradas;
* decisão;
* justificativa;
* consequências.

## REQ-080 — Forks

O projeto deve permanecer tecnicamente capaz de continuar existindo por meio de forks caso a infraestrutura de desenvolvimento original deixe de existir.

---

# 18. Segurança do desenvolvimento

## REQ-081 — Modelo de ameaças

O projeto deve possuir um modelo de ameaças documentado antes da implementação dos componentes críticos.

## REQ-082 — Divulgação responsável

O projeto deve possuir um processo público para comunicação e tratamento de vulnerabilidades.

## REQ-083 — Auditoria

Componentes críticos devem poder ser submetidos a auditorias independentes.

## REQ-084 — Dependências

Dependências externas utilizadas pelo projeto devem ser identificadas, documentadas e avaliadas quanto aos riscos introduzidos.

---

# 19. Transparência

## REQ-085 — Protocolos públicos

As regras fundamentais do protocolo devem ser públicas.

## REQ-086 — Alterações documentadas

Alterações relevantes no protocolo devem possuir histórico público.

## REQ-087 — Versionamento

Protocolos, especificações, documentos e componentes críticos devem possuir versões identificáveis.

## REQ-088 — Compatibilidade

Alterações incompatíveis devem possuir mecanismos claros para identificação e migração.

---

# 20. Independência da infraestrutura

## REQ-089 — Ausência de dependência crítica única

A Rede Zero não deve depender de um único provedor de infraestrutura para funcionar.

## REQ-090 — Independência do GitHub

A operação da Rede Zero não deve depender da existência do GitHub.

O GitHub será utilizado como infraestrutura de desenvolvimento, colaboração e distribuição do código, mas não será um componente necessário para a operação da Exonet.

## REQ-091 — Distribuição do conhecimento

Código, especificações e documentação fundamentais devem poder ser replicados em diferentes infraestruturas.

---

# 21. Evolução do protocolo

## REQ-092 — Evolução controlada

A Rede Zero deve possuir mecanismos para evolução do protocolo sem depender da intervenção manual de uma autoridade central.

## REQ-093 — Compatibilidade entre versões

O protocolo deve definir como diferentes versões dos Nodes interagem durante processos de atualização.

## REQ-094 — Registro de mudanças

Mudanças relevantes devem possuir identificação, versão e histórico.

## REQ-095 — Processo de decisão

Alterações fundamentais devem passar por um processo previamente definido de:

1. proposta;
2. análise;
3. discussão;
4. implementação experimental;
5. testes;
6. aprovação;
7. adoção.

---

# 22. Princípio da Continuidade

## REQ-096 — Nenhum indivíduo indispensável

Nenhuma pessoa deve ser estruturalmente indispensável para o funcionamento ou desenvolvimento da Rede Zero.

## REQ-097 — Conhecimento compartilhado

Conhecimentos essenciais para operação e desenvolvimento devem ser documentados e acessíveis à comunidade.

## REQ-098 — Sucessão técnica

Novos participantes devem ser capazes de compreender progressivamente o sistema e assumir responsabilidades técnicas.

## REQ-099 — Continuidade sem fundador

A Rede Zero deve continuar tecnicamente operável e desenvolvível mesmo que todos os seus criadores originais deixem de participar do projeto.

---

# 23. Requisitos ainda não definidos

Os seguintes pontos ainda necessitam de decisões técnicas específicas:

* algoritmo de consenso;
* modelo de emissão do ZERO;
* quantidade máxima ou política monetária do ZERO;
* estrutura dos blocos;
* formato das transações;
* modelo criptográfico de privacidade;
* mecanismo de anonimização de rede;
* mecanismo de descoberta de Nodes;
* protocolo P2P;
* resistência a Sybil;
* modelo definitivo de governança;
* mecanismo de votação;
* arquitetura do Grande Mercado;
* funcionamento matemático do Pool;
* integração com outras blockchains;
* modelo definitivo de identidade dos Nodes;
* sistema de reputação;
* protocolo de defesa;
* arquitetura do Navegador Zero;
* protocolo das Comunidades;
* armazenamento distribuído;
* mecanismo de atualização do protocolo;
* mecanismos de recuperação;
* modelo de segurança contra ataques;
* requisitos mínimos de hardware;
* linguagens de programação;
* bibliotecas e dependências;
* formatos de mensagens;
* APIs;
* testes de conformidade.

Esses pontos **não devem ser considerados decisões tomadas** até que sejam formalmente definidos em documentos, especificações ou propostas de decisão.

---

# 24. Regra de interpretação

Este documento define requisitos do sistema e não uma implementação específica.

Quando uma implementação não atender a um requisito, a implementação deverá ser revisada ou o requisito deverá passar por um processo formal de alteração.

Nenhum requisito deve ser considerado tecnicamente imutável.

Alterações neste documento devem possuir histórico e justificativa.

---

# 25. Status do documento

**Versão:** `0.2.0`

**Status:** Aprovado conceitualmente para prosseguimento da fase de arquitetura.

Esta versão incorpora as decisões realizadas durante a revisão do documento.

### Alterações principais desde a versão 0.1.0

* Remoção completa do **Agente Zero** do núcleo da Rede Zero.
* Remoção dos requisitos relacionados à computação distribuída para o Agente Zero.
* Reformulação do requisito de exclusividade monetária para diferenciar **ZERO** de ativos externos.
* Reforço do princípio de ausência de autoridade absoluta.
* Separação mais clara entre Rede Zero, Exonet, GitHub e interfaces.
* Reorganização e renumeração dos requisitos.
* Consolidação dos princípios de continuidade e independência.
* Definição de limites para mecanismos emergenciais de defesa.
* Reforço da distinção entre reputação de Node e identidade física do equipamento.

---

# 26. Próxima etapa

O próximo documento técnico deverá ser:

`THREAT_MODEL.md`

O Modelo de Ameaças deverá identificar:

* quem pode atacar a Rede Zero;
* quais recursos podem ser atacados;
* quais são os objetivos possíveis de um atacante;
* quais informações um atacante pode possuir;
* quais ataques devem ser considerados;
* quais ataques não fazem parte do modelo;
* quais propriedades precisam ser protegidas;
* quais mecanismos serão necessários para reduzir cada ameaça.

A arquitetura da Rede Zero deverá ser posteriormente confrontada com esse modelo de ameaças antes da implementação dos componentes críticos.
