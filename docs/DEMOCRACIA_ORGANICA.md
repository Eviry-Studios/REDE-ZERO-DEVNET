# DEMOCRACIA ORGÂNICA

## Modelo de participação política e comunitária da Rede Zero

**Versão:** 0.1.0
**Status:** Documento conceitual complementar ao Manifesto — incorporado com notas de conformidade (ADR-0013)
**Natureza:** Diretriz conceitual. Não altera regras protocolares vigentes; as regras concretas serão definidas em especificações próprias.
**Relacionamento:**

* `Manifesto Exonet` (§9 Navegador Zero, §10 Comunidades, §13 Governança)
* `REQUIREMENTS.md v0.2.0` (REQ-040..045, REQ-064..074)
* `THREAT_MODEL.md v0.3.0` (THR-GOV-*, THR-COM-*, THR-BRW-*)
* `docs/adr/0008-governanca-bicameral.md`, `spec/GOVERNANCE.md`
* `docs/adr/0013-democracia-organica.md`

> **Como ler este documento:** o texto das seções 1 a 14 é a proposta original. Onde ele precisa de interpretação para ser compatível com os documentos vigentes, há uma marca **[N-x]** que remete à seção **15. Notas de conformidade**, que prevalece em caso de dúvida.

---

## 1. Princípio

A Rede Zero não possui um administrador central responsável por decidir o futuro da rede.

As decisões são tomadas pela própria rede através de mecanismos de participação.

A participação pode ocorrer em diferentes níveis:

```text
USUÁRIO
   │
   ├── votação individual
   │
   └── participação em Comunidades
              │
              └── votação comunitária
```

Assim, uma proposta pode receber manifestações de indivíduos e também de Comunidades reconhecidas pela Exonet **[N-7]**.

A democracia da Rede Zero é, portanto, orgânica: a rede não observa somente indivíduos isolados, mas também as estruturas coletivas que surgem espontaneamente dentro dela.

## 2. Dois níveis de participação

Uma proposta da Rede Zero pode receber dois tipos de manifestação:

```text
                    PROPOSTA
                       │
             ┌─────────┴─────────┐
             │                   │
        USUÁRIOS             COMUNIDADES
             │                   │
       voto individual      voto comunitário
             │                   │
             └─────────┬─────────┘
                       │
                    APURAÇÃO
```

### Voto individual

Um usuário participa diretamente da votação.

Exemplo:

```text
Proposta #204

"Alterar determinado protocolo da Rede Zero"

Usuário A → SIM
Usuário B → SIM
Usuário C → NÃO
Usuário D → ABSTENÇÃO
```

A rede consegue registrar a participação individual conforme as regras de governança definidas pelo protocolo **[N-2]**.

### Voto de Comunidade

Uma Comunidade reconhecida pela Exonet também pode manifestar uma posição **[N-1]**.

Exemplo:

```text
Proposta #204

Comunidade Desenvolvedores → SIM
Comunidade Privacidade → NÃO
Comunidade Jogos → ABSTENÇÃO
Comunidade Ciência → SIM
```

Isso não significa necessariamente que cada membro daquela Comunidade tenha perdido seu direito de votar individualmente.

São manifestações diferentes.

## 3. A posição de uma Comunidade

Uma Comunidade possui sua própria governança.

Portanto, para que uma Comunidade vote em uma proposta da Rede Zero, ela precisa utilizar seu próprio mecanismo interno para determinar sua posição.

Exemplo:

```text
REDE ZERO
   │
   │ Proposta #204
   ▼
COMUNIDADE PRIVACIDADE
   │
   ├── membros discutem
   │
   ├── votação interna
   │
   └── resultado:
          NÃO
            │
            ▼
     posição da Comunidade
```

A Rede Zero não precisa determinar como uma Comunidade chegou à sua posição, desde que o método utilizado esteja dentro das regras que a própria Comunidade declarou **[N-6]**.

A Comunidade pode, por exemplo, possuir:

* votação direta;
* representantes;
* regras internas próprias;
* mecanismos de consenso;
* outros modelos permitidos pela Exonet.

A autonomia da Comunidade existe dentro dos limites do protocolo da Rede Zero.

## 4. A Rede passa a enxergar tendências coletivas

Uma consequência interessante desse modelo é que a Rede Zero pode apresentar não apenas o resultado geral de uma votação, mas também sua distribuição social.

Por exemplo **[N-2]**:

```text
PROPOSTA #204

Resultado individual
────────────────────
SIM:        62%
NÃO:        31%
ABSTENÇÃO:   7%

Posição das Comunidades
───────────────────────
Comunidades favoráveis:   74
Comunidades contrárias:  21
Abstenções:               9
```

Isso não precisa substituir o resultado oficial da votação.

São informações diferentes.

A primeira mostra a participação individual.

A segunda mostra como as estruturas coletivas reconhecidas pela Exonet estão se posicionando.

## 5. Democracia não significa que Comunidades possuem mais poder automaticamente

Uma Comunidade não deve receber poder ilimitado simplesmente por existir.

A existência de:

```text
100 usuários
```

não deve significar automaticamente que a Comunidade possa representar:

```text
100.000 usuários
```

A governança precisa definir separadamente:

* quem pode votar;
* quem pode criar uma Comunidade;
* como uma Comunidade obtém reconhecimento;
* como uma Comunidade pode manifestar uma posição;
* como evitar duplicação artificial de Comunidades;
* como evitar captura do sistema por identidades falsas;
* como os votos individuais e comunitários participam das decisões.

Essas regras ainda precisam ser definidas **[N-1]**.

O princípio fundamental, porém, pode permanecer:

> A Rede Zero permite que a vontade política seja observada tanto através dos indivíduos quanto através das estruturas coletivas que esses indivíduos criam.

## 6. Publicações dentro da Exonet

A Exonet utiliza um único esquema de endereçamento:

```text
zero://
```

O tipo da publicação aparece no final do endereço **[N-3]**.

Exemplos:

```text
zero://jornalzero.blog
zero://editorzero.app
zero://desenvolvedores.comunidade
```

Isso permite que o Navegador Zero reconheça imediatamente a natureza do recurso.

### Blog

```text
zero://jornalzero.blog
```

Uma publicação de conteúdo.

### Aplicação

```text
zero://editorzero.app
```

Um programa executado dentro do ambiente da Exonet.

### Comunidade

```text
zero://desenvolvedores.comunidade
```

Uma entidade coletiva reconhecida pela Exonet, com identidade, membros **[N-5]**, regras e governança própria.

Novos tipos podem existir posteriormente:

```text
zero://xxx.video
zero://xxx.forum
zero://xxx.market      [N-4]
zero://xxx.service
```

A criação de novos tipos deve obedecer às regras do protocolo.

## 7. Comunidade não é simplesmente um site

Uma Comunidade é diferente de uma publicação comum.

Podemos pensar:

```text
BLOG
│
└── conteúdo


APP
│
└── software


COMUNIDADE
│
├── identidade
├── membros
├── regras
├── estado
├── conteúdo
├── aplicações
└── governança
```

Uma Comunidade pode conter blogs, aplicações e outros recursos.

Por exemplo:

```text
zero://cientistas.comunidade

        │
        ├── zero://cientistas.blog
        │
        ├── zero://laboratorio.app
        │
        └── votação interna
```

A Comunidade funciona como uma estrutura social e técnica dentro da Exonet.

## 8. Criação de uma Comunidade

A criação de uma Comunidade exige mais do que simplesmente publicar uma página.

Fluxo conceitual:

```text
CRIAR COMUNIDADE
       │
       ▼
Definir identidade
       │
       ▼
Definir regras
       │
       ▼
Definir governança
       │
       ▼
Criar código/conteúdo inicial
       │
       ▼
Depositar ZERO
       │
       ▼
Gerar manifesto da Comunidade
       │
       ▼
Validação técnica
       │
       ▼
Processo de aprovação
       │
       ▼
COMUNIDADE RECONHECIDA
       │
       ▼
Publicação na Exonet
```

O depósito de ZERO funciona como mecanismo econômico contra spam e criação indiscriminada.

Ele não representa compra de autoridade.

## 9. Aplicações e desenvolvimento

A Rede Zero deve permitir que aplicações sejam desenvolvidas dentro do próprio ambiente da Exonet.

O Navegador Zero funciona também como ambiente de desenvolvimento.

Exemplo conceitual:

```text
> create app

Application name: Editor Zero
Type: application

> editor

01  fn main() {
02      println!("Hello Exonet");
03  }

> build
> test
> publish
```

A aplicação é construída para o Exonet Runtime.

Ela não precisa depender de uma página convencional da Internet.

O objetivo é que a Exonet tenha:

```text
Navegador
Terminal
Editor
Build
Teste
Execução
Publicação
```

integrados no mesmo ambiente.

## 10. Navegador Zero

O Navegador Zero é a principal interface do usuário para a Exonet.

Ele combina:

```text
┌─────────────────────────────────────┐
│            NAVEGADOR ZERO            │
├─────────────────────────────────────┤
│                                     │
│  zero://desenvolvedores.comunidade  │
│                                     │
├─────────────────────────────────────┤
│                                     │
│          INTERFACE GRÁFICA          │
│                                     │
│   conteúdo / aplicações / dados     │
│                                     │
├─────────────────────────────────────┤
│ > comunidade                        │
│ > wallet                            │
│ > node status                       │
│ > rede peers                        │
│ > help                              │
│                                     │
└─────────────────────────────────────┘
```

O terminal não é apenas visual.

Ele utiliza as mesmas APIs e protocolos utilizados pela interface gráfica.

Portanto:

```text
GUI
 │
 ├──────────────┐
 │              │
 ▼              ▼
Navegação     Terminal
 │              │
 └──────┬───────┘
        ▼
   Exonet APIs
        │
        ▼
    Rede Zero
```

O usuário pode escolher entre clicar ou utilizar comandos.

## 11. Instalação

A entrada para o usuário comum deve ser simples.

O usuário não precisa instalar manualmente blockchain, P2P, runtime ou ferramentas de desenvolvimento **[N-8]**.

A experiência pode começar com:

```text
REDE ZERO

A Internet fornece o caminho.
A Exonet define o espaço.

[ INSTALAR ]
```

Depois:

```text
REDE ZERO

Vamos preparar seu Node.

Seu computador terá uma identidade
criptográfica própria dentro da Exonet.

Essa identidade não é seu nome,
seu e-mail ou sua conta da Microsoft.

[ CONTINUAR ]
```

Depois:

```text
SUA WALLET

Você ainda não possui uma Wallet.

[ CRIAR NOVA WALLET ]
[ IMPORTAR WALLET ]
```

E finalmente:

```text
REDE ZERO

Node: ONLINE
Exonet: CONECTADA
Blockchain: SINCRONIZADA

[ NAVEGADOR ZERO ]
[ WALLET ]
[ COMUNIDADES ]
[ GRANDE MERCADO ]
[ NODE ]
[ CONFIGURAÇÕES ]
```

A complexidade da infraestrutura permanece abaixo da experiência do usuário.

## 12. Execução distribuída

Aplicações e Comunidades não devem depender da honestidade de um único Node.

O princípio é:

```text
APLICAÇÃO
     │
     ├──── Node A
     ├──── Node B
     ├──── Node C
     └──── Node D
```

O Node fornece infraestrutura.

Ele não se torna automaticamente proprietário da aplicação.

O estado que precisa ser reconhecido pela Exonet deve possuir mecanismos de validação próprios.

Assim:

```text
Node A altera sua cópia
        │
        ▼
Estado inválido
        │
        ▼
Rede rejeita
```

O objetivo é separar:

```text
HOSTING
   ≠
AUTORIDADE
```

## 13. Confiança nos Nodes

A Rede Zero não deve partir da premissa:

> "Este Node é confiável."

A premissa deve ser:

> "Mesmo que este Node seja comprometido, o protocolo deve limitar o que ele consegue fazer."

O Node pode:

* hospedar dados;
* executar aplicações;
* participar da rede;
* fornecer recursos;
* armazenar cópias do estado;
* desaparecer e voltar posteriormente.

Mas não deve receber automaticamente capacidade de:

* falsificar estado;
* criar ZERO;
* assumir identidade de usuários;
* alterar regras da Comunidade;
* alterar o protocolo;
* obter chaves privadas de usuários.

Isso cria uma fronteira fundamental:

```text
COMPUTADOR DO OPERADOR
        │
        │ potencialmente não confiável
        ▼
┌─────────────────────────┐
│    EXONET RUNTIME       │
│                         │
│  aplicação isolada      │
│  permissões limitadas   │
│  APIs da Exonet         │
└─────────────────────────┘
        │
        ▼
   REDE ZERO
```

## 14. A visão completa

Com essas ideias, a experiência da Rede Zero começa a formar um ciclo completo:

```text
                    REDE ZERO
                        │
                        ▼
                     EXONET
                        │
              ┌─────────┴─────────┐
              │                   │
         NAVEGADOR ZERO        TERMINAL
              │                   │
              └─────────┬─────────┘
                        │
                  zero://...
                        │
          ┌─────────────┼─────────────┐
          │             │             │
        .blog          .app     .comunidade
          │             │             │
      conteúdo      software       entidade
                                      │
                              ┌───────┴───────┐
                              │               │
                           membros        governança
                              │               │
                              └───────┬───────┘
                                      │
                              DEMOCRACIA ORGÂNICA
                                      │
                        ┌─────────────┴─────────────┐
                        │                           │
                  voto individual            voto comunitário
                        │                           │
                        └─────────────┬─────────────┘
                                      │
                                  PROPOSTA
                                      │
                                      ▼
                               REDE ZERO
```

### Princípio final

A Rede Zero não precisa tratar todos os participantes como uma massa homogênea.

Ela permite que existam:

```text
INDIVÍDUOS
      +
COMUNIDADES
      +
APLICAÇÕES
      +
PUBLICAÇÕES
```

e todos participam de uma mesma infraestrutura.

O indivíduo pode falar diretamente.

Uma Comunidade pode falar como organização.

E a própria rede consegue registrar essas duas dimensões sem necessariamente confundi-las.

Essa é a ideia central da Democracia Orgânica da Exonet.

---

## 15. Notas de conformidade

Estas notas encaixam a proposta nos documentos vigentes **sem alterá-los**. Onde a proposta e um documento vigente parecem divergir, a nota indica a interpretação adotada. Mudanças de regra protocolar continuam exigindo o processo formal (REQ-092, REQ-095).

### N-1 — Voto comunitário é sinalização, não peso decisório (por enquanto)

**Vigente:** a decisão de uma proposta é dada pela apuração bicameral da ADR-0008 (câmara econômica e câmara de contribuição), com quórum e limiar nas duas.

**Interpretação:** a posição de Comunidades é uma **terceira dimensão registrada e verificável, não vinculante**. Ela aparece junto do resultado oficial (seção 4), mas não o altera. A própria proposta já prevê isso ("não precisa substituir o resultado oficial") e deixa em aberto "como os votos individuais e comunitários participam das decisões".

Dar peso decisório a Comunidades exige uma nova ADR que demonstre resistência a Sybil (REQ-044): criar muitas Comunidades não pode multiplicar influência, que é exatamente o risco que a seção 5 aponta.

### N-2 — "Voto individual" segue a ponderação vigente

**Vigente:** `ARCHITECTURE.md §32` impede adotar automaticamente `1 carteira = 1 voto` ou `1 ZERO = 1 voto`.

**Interpretação:** "voto individual" é o voto de uma conta nas regras da ADR-0008. Os percentuais do exemplo da seção 4 ("Resultado individual") correspondem ao **resultado ponderado de cada câmara**, não a uma contagem por pessoa. Contagens por cabeça podem ser exibidas como informação, mas nunca como resultado oficial.

A contagem de Comunidades ("74 favoráveis") considera apenas **Comunidades reconhecidas** (N-7) e é informativa.

### N-3 — Nomes legíveis são uma camada sobre identificadores criptográficos

**Vigente:** `SPECIFICATIONS.md §5` e `§60` exigem identificadores verificáveis e resistentes a colisão. O THR-BRW-002 prevê nomes legíveis apenas como camada adicional, contra phishing.

**Interpretação:** `zero://nome.tipo` é um **nome registrado** que aponta para um identificador criptográfico da publicação ou Comunidade.

* O sufixo indica o tipo e é verificado contra o tipo registrado. Um `.app` não pode se passar por `.comunidade`.
* `.comunidade` só existe para Comunidades reconhecidas (N-7).
* Publicações e aplicações privadas podem existir sem aprovação (Manifesto §10), mas usam o identificador criptográfico ou um nome registrado segundo regras objetivas a definir.
* O Navegador Zero deve alertar sobre nomes visualmente semelhantes (THR-BRW-002).

O `zero://comunidade` do Manifesto §9 é compatível: é a forma conceitual sem sufixo.

### N-4 — `.market` não pode ser uma segunda DEX

**Vigente:** REQ-032, REQ-033 e Manifesto §11: o Grande Mercado é o **único** mecanismo oficial de negociação descentralizada, e Comunidades não devem criar uma DEX concorrente.

**Interpretação:** o tipo `.market` fica reservado a interfaces do Grande Mercado e a comércio de bens e serviços que liquida pelo Grande Mercado ou por transferências comuns. Não pode implementar livro de ofertas nem liquidação própria de ativos.

### N-5 — Filiação a Comunidades com privacidade

**Vigente:** REQ-023 (privacidade por padrão), REQ-025 (minimização de metadados), REQ-027 (sem registro civil central).

**Interpretação:** "membros" não implica lista pública nem identidade civil. A filiação deve ser pseudônima e mínima. Cada Comunidade pode escolher mecanismos que não exponham quem são seus membros (por exemplo, votação interna com provas de pertinência sem revelar a identidade, a especificar). O protocolo não exige que uma Comunidade publique sua lista de membros.

### N-6 — A posição de uma Comunidade precisa ser verificável

**Vigente:** REQ-043 (votação verificável) e REQ-073 (regras objetivas).

**Interpretação:** ao ser reconhecida, a Comunidade **declara no protocolo** o seu mecanismo de decisão (por exemplo, um conjunto de chaves com limiar, ou uma regra de votação interna auditável). A posição enviada a uma proposta é aceita apenas se assinada ou comprovada segundo esse mecanismo. A Rede Zero não julga o *mérito* do método, só a sua *conformidade* com o que foi declarado. Uma posição sem essa prova é apenas uma opinião, não uma manifestação da Comunidade.

### N-7 — "Reconhecida pela Exonet" significa reconhecida pela governança

**Vigente:** a Exonet é o ambiente de rede, não uma autoridade (Manifesto §3). O reconhecimento de Comunidades ocorre pela categoria **Comunidade** da ADR-0008, que usa a mesma apuração bicameral, com quórum de 5% e maioria simples, e registra o hash do manifesto da Comunidade.

**Interpretação:** "Comunidade reconhecida pela Exonet" = Comunidade aprovada por proposta de categoria Comunidade. O fluxo da seção 8 corresponde a:

1. depósito;
2. manifesto (o hash vai na proposta);
3. validação técnica (regras objetivas, REQ-073);
4. votação;
5. registro.

Conteúdo controverso não é motivo técnico de rejeição (REQ-074).

### N-8 — Instalação simples preserva a privacidade de rede

**Vigente:** ADR-0011 (proteção do IP do usuário).

**Interpretação:** o instalador simples deve criar, por padrão, um **Node privado** (sem aceitar conexões e sem anunciar endereço), com a Wallet conectada a ele localmente, e oferecer Tor como opção. A identidade criptográfica do Node ("não é seu nome, e-mail ou conta") segue o REQ-010 e o REQ-046.

### Pontos sem divergência

As seções 1, 3 (autonomia dentro do protocolo), 5, 7, 9, 10, 12 e 13 estão em conformidade direta com os seguintes pontos dos documentos vigentes:

* REQ-005 e REQ-040: sem autoridade central;
* REQ-064..067, SPEC §58–59 e INV-008: o Navegador e o terminal usam as mesmas APIs, sem autoridade especial;
* REQ-068..074 e SPEC §60–62: Comunidades com governança própria, *sandbox* e sem autoridade sobre o consenso;
* INV-006 e INV-007, TM-P-001: "hosting ≠ autoridade" e Nodes não confiáveis;
* Manifesto §10: backend executado por múltiplos Nodes.

### Próximas especificações derivadas

* `spec/COMMUNITIES.md`: registro, declaração do mecanismo de decisão, manifestação de posição (N-1, N-5, N-6, N-7). **Publicada** (ADR-0015).
* `spec/NAMING.md`: nomes `zero://nome.tipo` e tipos registrados (N-3, N-4). **Publicada** (ADR-0015).
* `spec/RUNTIME.md`: Exonet Runtime e *sandbox* (seções 9, 12, 13).
