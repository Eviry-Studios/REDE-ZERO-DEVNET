//! Interface do Navegador (`navegador.localhost`): identificação, conexão,
//! navegação, Comunidades, Grande Mercado, Wallet, governança e defesa
//! (`SPEC §58`). Páginas sem scripts; toda ação passa por formulário com
//! token e verificação de origem.

use rz_core::community::{skeleton, CommunityStatus, NameKind};
use rz_core::{format_zero, market::AssetId};
use rz_crypto::Hash32;
use rz_p2p::Message;
use rz_wallet::format_units;

use crate::html::{esc, field, form, hidden, page};
use crate::http::{percent_encode, Request, Response};
use crate::pedido::State;
use crate::{lock, Navegador, Origin};

pub(crate) fn handle(nav: &Navegador, req: &Request) -> Response {
    if req.method == "POST" {
        let f = req.form();
        let origin_ok = req.header("origin") == Some(Origin::Ui.url(nav.port).as_str());
        if !origin_ok || f.get("token").map(String::as_str) != Some(nav.token.as_str()) {
            return Response::text(403, "formulário recusado: origem ou token inválido");
        }
        return post(nav, req, &f);
    }
    if req.method != "GET" && req.method != "HEAD" {
        return Response::text(405, "método não permitido");
    }
    let q = |k: &str| req.query.get(k).map(String::as_str).unwrap_or("");
    let body = match req.path.as_str() {
        "/" => home(nav),
        "/ir" => go(nav, q("endereco")),
        "/carteira" => wallet(nav),
        "/mercado" => market(nav),
        "/mercado/ativo" => asset(nav, q("id")),
        "/governanca" => governance(nav),
        "/comunidade" => community(nav, q("nome")),
        "/defesa" => defense(nav),
        "/pedidos" => pedidos(nav),
        p if p.starts_with("/pedidos/") => pedido(nav, &p["/pedidos/".len()..]),
        _ => {
            return Response::html(
                404,
                render(nav, "Não encontrado", "<p>Página inexistente.</p>"),
            )
        }
    };
    let (title, html) = body;
    Response::html(200, render(nav, &title, &html))
}

fn render(nav: &Navegador, title: &str, body: &str) -> String {
    page(title, lock(&nav.pedidos).pending(), body)
}

fn post(
    nav: &Navegador,
    req: &Request,
    f: &std::collections::BTreeMap<String, String>,
) -> Response {
    let path = req.path.as_str();
    if path == "/pedir" {
        return match nav.create_pedido(Origin::Ui, f) {
            Ok(id) => Response::redirect(&format!("/pedidos/{id}")),
            Err(e) => Response::html(400, render(nav, "Pedido inválido", &err_box(&e))),
        };
    }
    if path == "/fixar" {
        let addr = f.get("endereco").cloned().unwrap_or_default();
        let (Some(o), Some(v)) = (
            Origin::from_address(&addr),
            f.get("valor").and_then(|h| Hash32::from_hex(h)),
        ) else {
            return Response::text(400, "dados inválidos");
        };
        nav.pin(&o.address(), v);
        return Response::redirect(&format!("/ir?endereco={}", percent_encode(&o.address())));
    }
    if let Some(rest) = path.strip_prefix("/pedidos/") {
        if let Some(id) = rest.strip_suffix("/aprovar") {
            nav.execute(id);
            return Response::redirect(&format!("/pedidos/{id}"));
        }
        if let Some(id) = rest.strip_suffix("/recusar") {
            let pending = lock(&nav.pedidos)
                .items
                .get(id)
                .is_some_and(|p| p.state == State::Pending);
            if pending {
                lock(&nav.pedidos).resolve(id, State::Refused);
            }
            return Response::redirect(&format!("/pedidos/{id}"));
        }
    }
    Response::text(404, "ação desconhecida")
}

fn err_box(e: &str) -> String {
    format!("<div class=\"box bad\">{}</div>", esc(e))
}

fn kv(rows: &[(&str, String)]) -> String {
    let mut s = String::from("<table>");
    for (k, v) in rows {
        s.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>", esc(k), v));
    }
    s.push_str("</table>");
    s
}

fn code(s: impl std::fmt::Display) -> String {
    format!("<code>{}</code>", esc(&s.to_string()))
}

// ------------------------------------------------------------------ início

fn home(nav: &Navegador) -> (String, String) {
    let mut b = String::new();
    b.push_str(
        "<form method=\"get\" action=\"/ir\"><label>Endereço da Exonet\
<input type=\"text\" name=\"endereco\" placeholder=\"zero://nome.tipo\" autofocus></label>\
<button>Ir</button></form>",
    );
    let status = nav.with_client(|c| {
        c.request(&Message::GetStatus, |m| match m {
            Message::Status { height, peers, .. } => Some((height, peers)),
            _ => None,
        })
        .map_err(|e| e.to_string())
    });
    let via = match (&nav.net.proxy, nav.node.is_loopback()) {
        (Some(p), _) => format!("via proxy {} (o Node não vê seu IP)", esc(&p.to_string())),
        (None, true) => "Node local".into(),
        (None, false) => "conexão direta: o operador do Node vê seu IP".into(),
    };
    b.push_str("<h2>Conexão</h2>");
    b.push_str(&match status {
        Ok((h, peers)) => kv(&[
            ("Rede", esc(&nav.genesis.network_id)),
            ("Node", format!("{} — {via}", esc(&nav.node.to_string()))),
            ("Altura", h.to_string()),
            ("Pares do Node", peers.to_string()),
        ]),
        Err(e) => err_box(&format!("sem conexão com o Node: {e}")),
    });
    b.push_str("<h2>Identificação</h2>");
    match &nav.keys {
        Some(k) => b.push_str(&format!(
            "{}<p>Cada publicação vê uma identidade diferente, derivada desta chave e do endereço do site; \
nenhuma revela seu endereço.</p>",
            kv(&[("Endereço transparente", code(k.address()))])
        )),
        None => b.push_str("<p>Sem chave: modo somente leitura. Inicie com <code>--key</code> para assinar.</p>"),
    }
    let pins = lock(&nav.pins).clone();
    if !pins.is_empty() {
        b.push_str("<h2>Fixados</h2><table><tr><th>Endereço</th><th>Identificador</th></tr>");
        for (a, h) in &pins {
            b.push_str(&format!(
                "<tr><td><a href=\"/ir?endereco={}\">{}</a></td><td>{}</td></tr>",
                percent_encode(a),
                esc(a),
                code(h)
            ));
        }
        b.push_str("</table>");
    }
    b.push_str(
        "<div class=\"box\">As publicações abrem em origens isoladas e não podem acessar servidores \
externos: seu IP não é revelado a terceiros pelo conteúdo. Links para a web comum, se clicados, \
saem da Exonet.</div>",
    );
    ("Início".into(), b)
}

// --------------------------------------------------------------- navegação

fn go(nav: &Navegador, input: &str) -> (String, String) {
    let Some(o) = Origin::from_address(input) else {
        return (
            "Endereço inválido".into(),
            err_box("use zero://nome.tipo (tipos: comunidade, blog, app, market, forum, video, service) ou zero://IDENTIFICADOR"),
        );
    };
    let addr = o.address();
    let mut b = format!("<h1>{}</h1>", esc(&addr));
    // Alerta de nomes parecidos com outros já fixados (THR-BRW-002).
    if let Origin::Named { name, kind } = &o {
        let sk = skeleton(name);
        let similar: Vec<String> = lock(&nav.pins)
            .keys()
            .filter(|a| *a != &addr)
            .filter_map(|a| Origin::from_address(a))
            .filter_map(|p| match &p {
                Origin::Named { name: n, kind: k }
                    if skeleton(n) == sk || (n == name && k != kind) =>
                {
                    Some(p.address())
                }
                _ => None,
            })
            .collect();
        if !similar.is_empty() {
            b.push_str(&format!(
                "<div class=\"box warn\">Atenção: parecido com {} que você já usa. Confira o identificador.</div>",
                esc(&similar.join(", "))
            ));
        }
    }
    let pinned = lock(&nav.pins).get(&addr).copied();
    match nav.resolve(&o) {
        Ok((value, content)) => {
            let mut rows = vec![("Conteúdo verificado por", code(content))];
            if let Origin::Named {
                kind: NameKind::Community,
                ..
            } = &o
            {
                rows.push(("Manifesto (versão fixável)", code(value)));
            }
            if let Origin::Named { name, kind } = &o {
                if *kind != NameKind::Community {
                    if let Ok((_, Some(owner))) =
                        nav.with_client(|c| rz_wallet::resolve(c, name, *kind))
                    {
                        rows.push(("Dono do nome", code(owner)));
                    }
                }
            }
            b.push_str(&kv(&rows));
            match pinned {
                Some(p) if p == value => {
                    b.push_str("<div class=\"box ok\">Identificador fixado e igual ao atual.</div>")
                }
                Some(p) => {
                    b.push_str(&format!(
                        "<div class=\"box warn\">O identificador mudou. Fixado: {}<br>Atual: {}</div>",
                        code(p),
                        code(value)
                    ));
                    b.push_str(&form(
                        "/fixar",
                        &nav.token,
                        &(hidden("endereco", &addr) + &hidden("valor", &value.to_hex())),
                        "Aceitar o novo identificador",
                    ));
                }
                None => b.push_str("<p>Ainda não fixado: será fixado no primeiro acesso.</p>"),
            }
            b.push_str(&format!(
                "<p><a href=\"{}/\"><strong>Abrir {}</strong></a></p>",
                o.url(nav.port),
                esc(&addr)
            ));
        }
        Err(e) => b.push_str(&err_box(&e)),
    }
    (addr, b)
}

// ------------------------------------------------------------------ Wallet

fn wallet(nav: &Navegador) -> (String, String) {
    let Some(keys) = &nav.keys else {
        return (
            "Wallet".into(),
            "<p>Sem chave (modo somente leitura).</p>".into(),
        );
    };
    let mut b = String::from("<h1>Wallet</h1>");
    match nav.with_client(|c| rz_wallet::account(c, keys.address())) {
        Ok((balance, nonce, height)) => b.push_str(&kv(&[
            ("Endereço transparente", code(keys.address())),
            (
                "Saldo transparente",
                format!("{} ZERO", format_zero(balance)),
            ),
            ("Nonce", nonce.to_string()),
            ("Altura", height.to_string()),
            ("Endereço privado", code(keys.shielded_address())),
        ])),
        Err(e) => b.push_str(&err_box(&e)),
    }
    b.push_str("<h2>Transferir (transparente)</h2>");
    b.push_str(&form(
        "/pedir",
        &nav.token,
        &(hidden("tipo", "transferencia")
            + &field("Para (endereço transparente)", "para", "")
            + &field("Valor em ZERO", "valor", "")),
        "Revisar",
    ));
    b.push_str(
        "<p>Envios privados (RingCT) continuam na <code>zero-wallet send</code>, que varre suas notas localmente.</p>",
    );
    ("Wallet".into(), b)
}

// ---------------------------------------------------------- Grande Mercado

fn price_str(price: u64, decimals: u8) -> String {
    let p = rz_wallet::zero_per_unit(price, decimals);
    format_zero(u64::try_from(p).unwrap_or(u64::MAX))
}

fn market(nav: &Navegador) -> (String, String) {
    let mut b = String::from("<h1>Grande Mercado</h1>");
    match nav.with_client(|c| rz_wallet::assets(c, None)) {
        Ok(v) => {
            b.push_str(&format!(
                "<p>Pool permanente: {} ZERO (sem operação de saída).</p>",
                format_zero(v.pool_zero)
            ));
            b.push_str("<table><tr><th>Ativo</th><th>Origem</th><th>Pool</th><th>Último preço (ZERO/unidade)</th></tr>");
            for a in &v.assets {
                b.push_str(&format!(
                    "<tr><td><a href=\"/mercado/ativo?id={}\">{}</a></td><td>{}:{}</td><td>{}</td><td>{}</td></tr>",
                    a.id.0,
                    code(a.id.0),
                    esc(&a.info.network),
                    esc(&a.info.asset_ref),
                    format_units(a.pool, a.info.decimals),
                    a.last_price.map(|p| price_str(p, a.info.decimals)).unwrap_or("—".into())
                ));
            }
            b.push_str("</table>");
        }
        Err(e) => b.push_str(&err_box(&e)),
    }
    ("Grande Mercado".into(), b)
}

fn asset(nav: &Navegador, id: &str) -> (String, String) {
    let Some(h) = Hash32::from_hex(id) else {
        return ("Ativo".into(), err_box("ativo inválido"));
    };
    let asset = AssetId(h);
    let owner = nav.keys.as_ref().map(|k| k.address());
    let r = nav.with_client(|c| {
        let a = rz_wallet::assets(c, owner)?
            .assets
            .into_iter()
            .find(|a| a.id == asset)
            .ok_or("ativo não registrado")?;
        Ok((a.clone(), rz_wallet::market(c, asset, owner)?))
    });
    let (a, m) = match r {
        Ok(x) => x,
        Err(e) => return ("Ativo".into(), err_box(&e)),
    };
    let d = a.info.decimals;
    let label = format!("{}:{}", a.info.network, a.info.asset_ref);
    let mut b = format!("<h1>{}</h1>", esc(&label));
    b.push_str(&kv(&[
        ("Identificador", code(h)),
        ("Verificação", esc(&format!("{:?}", a.info.verification))),
        (
            "Seu saldo",
            a.balance.map(|v| format_units(v, d)).unwrap_or("—".into()),
        ),
    ]));
    let side_table = |title: &str, levels: &[rz_core::market::BookLevel]| {
        let mut s = format!(
            "<h3>{}</h3><table><tr><th>Preço (ZERO/unidade)</th><th>Quantidade</th></tr>",
            esc(title)
        );
        for l in levels {
            s.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>",
                price_str(l.price, d),
                format_units(l.amount, d)
            ));
        }
        s + "</table>"
    };
    b.push_str(&side_table("Compras", &m.bids));
    b.push_str(&side_table("Vendas", &m.asks));
    if !m.own.is_empty() {
        b.push_str("<h3>Suas ordens</h3><table>");
        for o in &m.own {
            b.push_str(&format!(
                "<tr><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                code(o.id),
                o.side,
                format_units(o.remaining, d),
                price_str(o.price, d),
                form(
                    "/pedir",
                    &nav.token,
                    &(hidden("tipo", "cancelar") + &hidden("ordem", &o.id.to_hex())),
                    "Cancelar"
                )
            ));
        }
        b.push_str("</table>");
    }
    if nav.keys.is_some() {
        b.push_str("<h3>Nova ordem</h3>");
        b.push_str(&form(
            "/pedir",
            &nav.token,
            &(hidden("tipo", "ordem")
                + &hidden("ativo", &h.to_hex())
                + "<label>Lado <select name=\"lado\"><option value=\"compra\">compra</option>\
<option value=\"venda\">venda</option></select></label>"
                + &field("Quantidade", "quantidade", "")
                + &field("Preço limite (ZERO por unidade)", "preco", "")
                + &field("Validade em blocos", "blocos", "1000")),
            "Revisar",
        ));
        b.push_str("<p>Todas as ordens que cruzam num bloco executam ao mesmo preço (leilão por bloco).</p>");
    }
    (label, b)
}

// -------------------------------------------------------------- governança

fn governance(nav: &Navegador) -> (String, String) {
    let mut b = String::from("<h1>Governança</h1>");
    match nav.with_client(|c| rz_wallet::governance(c, None)) {
        Ok(v) => {
            b.push_str(&format!("<p>Altura {}.</p>", v.height));
            b.push_str("<table><tr><th>Proposta</th><th>Categoria</th><th>Estado</th><th>Votação</th><th></th></tr>");
            for p in &v.proposals {
                let vote = if p.status == rz_core::ProposalStatus::Pending && nav.keys.is_some() {
                    form(
                        "/pedir",
                        &nav.token,
                        &(hidden("tipo", "voto")
                            + &hidden("proposta", &p.id.to_hex())
                            + "<select name=\"escolha\"><option value=\"sim\">sim</option>\
<option value=\"nao\">não</option><option value=\"abstencao\">abstenção</option></select>"),
                        "Votar",
                    )
                } else {
                    String::new()
                };
                b.push_str(&format!(
                    "<tr><td>{}<br><small>conteúdo {}</small></td><td>{}</td><td>{}</td><td>{}–{}</td><td>{}</td></tr>",
                    code(p.id),
                    code(p.content_hash),
                    esc(p.category.name()),
                    esc(p.status.name()),
                    p.voting_start,
                    p.voting_end,
                    vote
                ));
            }
            b.push_str("</table><p>Votar exige ZERO bloqueado (<code>zero-wallet lock</code>). Votos são públicos nesta versão.</p>");
        }
        Err(e) => b.push_str(&err_box(&e)),
    }
    ("Governança".into(), b)
}

// ------------------------------------------------------------- Comunidades

fn community(nav: &Navegador, name: &str) -> (String, String) {
    let mut b = String::from(
        "<h1>Comunidades</h1><form method=\"get\" action=\"/comunidade\"><label>Nome\
<input type=\"text\" name=\"nome\"></label><button>Buscar</button></form>",
    );
    if name.is_empty() {
        return ("Comunidades".into(), b);
    }
    match nav.with_client(|c| rz_wallet::community(c, name)) {
        Ok((_, Some(c))) => {
            let origin = Origin::Named {
                name: c.name.clone(),
                kind: NameKind::Community,
            };
            b.push_str(&format!("<h2>{}</h2>", esc(&origin.address())));
            b.push_str(&kv(&[
                ("Identificador", code(c.id)),
                (
                    "Estado",
                    match c.status {
                        CommunityStatus::Recognized => "reconhecida".into(),
                        CommunityStatus::Declared => {
                            format!("declarada (até a altura {})", c.expires_at)
                        }
                    },
                ),
                ("Versão", c.version.to_string()),
                ("Manifesto", code(c.manifest_hash)),
                (
                    "Regra de decisão",
                    format!("{} de {} chaves", c.rule.threshold, c.rule.keys.len()),
                ),
            ]));
            match nav.manifest(&c.manifest_hash) {
                Ok(Some(m)) => b.push_str(&kv(&[
                    ("Descrição", esc(&m.description)),
                    ("Interface", m.frontend.map(code).unwrap_or("—".into())),
                    (
                        "Módulo (Exonet Runtime)",
                        m.module.map(code).unwrap_or("—".into()),
                    ),
                ])),
                Ok(None) => {
                    b.push_str("<p>Manifesto indisponível nos Nodes ou em formato livre.</p>")
                }
                Err(e) => b.push_str(&err_box(&e)),
            }
            b.push_str("<h3>Histórico de versões</h3><table>");
            for (v, h, at) in &c.history {
                b.push_str(&format!(
                    "<tr><td>{v}</td><td>{}</td><td>altura {at}</td></tr>",
                    code(h)
                ));
            }
            b.push_str("</table>");
            if c.status == CommunityStatus::Recognized {
                b.push_str(&format!(
                    "<p><a href=\"/ir?endereco={}\">Abrir a Comunidade</a></p>",
                    percent_encode(&origin.address())
                ));
            }
            b.push_str("<p>A Rede Zero não registra membros: só verifica decisões pela regra declarada.</p>");
        }
        Ok((_, None)) => b.push_str(&err_box("Comunidade não encontrada")),
        Err(e) => b.push_str(&err_box(&e)),
    }
    ("Comunidades".into(), b)
}

// ------------------------------------------------------------------ Defesa

fn defense(nav: &Navegador) -> (String, String) {
    let mut b = String::from("<h1>Defesa da Exonet</h1>");
    match nav.with_client(rz_wallet::defense) {
        Ok(v) => {
            let mut rows = vec![
                ("Modo", esc(v.mode.name())),
                ("Desde a altura", v.mode_since.to_string()),
            ];
            if v.mode != rz_core::defense::DefenseMode::Normal {
                rows.push((
                    "Vence sem renovação na altura",
                    v.mode_expires_at.to_string(),
                ));
            }
            if let Some(i) = &v.incident {
                rows.push(("Incidente", format!("{} ({:?})", code(i.id), i.status)));
                rows.push(("Ações registradas", i.actions.len().to_string()));
                rows.push(("Credenciais", v.credentials.len().to_string()));
            }
            b.push_str(&kv(&rows));
            b.push_str("<p>Decisões de defesa exigem atestações de validadores; todo modo vence sozinho. Não existe operação ofensiva.</p>");
        }
        Err(e) => b.push_str(&err_box(&e)),
    }
    ("Defesa".into(), b)
}

// ----------------------------------------------------------------- pedidos

fn origin_label(o: &Origin) -> String {
    match o {
        Origin::Ui => "Navegador Zero (você)".into(),
        o => o.address(),
    }
}

fn pedidos(nav: &Navegador) -> (String, String) {
    let list = lock(&nav.pedidos).items.clone();
    let mut b = String::from("<h1>Pedidos de assinatura</h1>");
    if list.is_empty() {
        b.push_str("<p>Nenhum pedido.</p>");
    }
    b.push_str("<table>");
    for p in list.values() {
        let what =
            p.op.describe()
                .first()
                .map(|d| d.1.clone())
                .unwrap_or_default();
        b.push_str(&format!(
            "<tr><td><a href=\"/pedidos/{0}\">{0}</a></td><td>{1}</td><td>{2}</td><td>{3}</td></tr>",
            esc(&p.id),
            esc(&origin_label(&p.origin)),
            esc(&what),
            state_label(&p.state)
        ));
    }
    b.push_str("</table>");
    ("Pedidos".into(), b)
}

fn state_label(s: &State) -> String {
    match s {
        State::Pending => "pendente".into(),
        State::Approved { .. } => "aprovado".into(),
        State::Refused => "recusado".into(),
        State::Failed(e) => format!("erro: {}", esc(e)),
    }
}

fn pedido(nav: &Navegador, id: &str) -> (String, String) {
    let Some(p) = lock(&nav.pedidos).items.get(id).cloned() else {
        return ("Pedido".into(), err_box("pedido não encontrado"));
    };
    let mut b = format!("<h1>Pedido {}</h1>", esc(&p.id));
    b.push_str(&format!(
        "<div class=\"box warn\">Origem: <strong>{}</strong></div>",
        esc(&origin_label(&p.origin))
    ));
    let rows: Vec<(String, String)> = p.op.describe();
    let rows: Vec<(&str, String)> = rows.iter().map(|(k, v)| (k.as_str(), esc(v))).collect();
    b.push_str(&kv(&rows));
    match &p.state {
        State::Pending => {
            if nav.keys.is_none() {
                b.push_str(&err_box("Navegador sem chave: não é possível aprovar."));
            }
            b.push_str(&form(
                &format!("/pedidos/{}/aprovar", p.id),
                &nav.token,
                "",
                "Aprovar e assinar",
            ));
            b.push_str(&form(
                &format!("/pedidos/{}/recusar", p.id),
                &nav.token,
                "",
                "Recusar",
            ));
        }
        State::Approved {
            txid,
            key,
            signature,
        } => {
            let mut r = Vec::new();
            if let Some(t) = txid {
                r.push(("Transação", code(t)));
            }
            if let Some(k) = key {
                r.push(("Chave deste site", code(k)));
            }
            if let Some(s) = signature {
                r.push(("Assinatura", code(s)));
            }
            b.push_str("<div class=\"box ok\">Aprovado.</div>");
            b.push_str(&kv(&r));
        }
        s => b.push_str(&format!("<div class=\"box bad\">{}</div>", state_label(s))),
    }
    ("Pedido".into(), b)
}
