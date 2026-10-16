//! Páginas da interface do Navegador: HTML gerado no servidor, sem scripts.

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "body{font-family:system-ui,sans-serif;max-width:60rem;margin:0 auto;\
padding:0 1rem 3rem;line-height:1.5;color:#1d1d1f;background:#fafafa}\
header{display:flex;flex-wrap:wrap;gap:.25rem 1rem;align-items:center;padding:.75rem 0;\
border-bottom:1px solid #ddd;margin-bottom:1rem}header strong{margin-right:1rem}\
a{color:#0b57d0}code,.mono{font-family:ui-monospace,monospace;word-break:break-all;font-size:.9em}\
table{border-collapse:collapse;width:100%}td,th{border-bottom:1px solid #e5e5e5;padding:.35rem;\
text-align:left;vertical-align:top}form.inline{display:inline}\
input,select,button{font:inherit;padding:.3rem .5rem;margin:.15rem 0}input[type=text]{width:100%;\
box-sizing:border-box}.box{background:#fff;border:1px solid #ddd;border-radius:.5rem;\
padding:.75rem 1rem;margin:1rem 0}.warn{background:#fff4e5;border-color:#f0a020}\
.ok{background:#eef8ee;border-color:#4a4}.bad{background:#fdecea;border-color:#d33}\
@media (prefers-color-scheme:dark){body{background:#151515;color:#e8e8e8}\
.box{background:#1f1f1f;border-color:#333}.warn{background:#3a2a10}.ok{background:#14301a}\
.bad{background:#3a1515}a{color:#8ab4f8}td,th{border-color:#333}header{border-color:#333}}";

pub fn page(title: &str, pending: usize, body: &str) -> String {
    let badge = if pending > 0 {
        format!(" ({pending})")
    } else {
        String::new()
    };
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{} — Navegador Zero</title><style>{STYLE}</style></head><body>\
<header><strong>Navegador Zero</strong><a href=\"/\">Início</a><a href=\"/carteira\">Wallet</a>\
<a href=\"/mercado\">Grande Mercado</a><a href=\"/governanca\">Governança</a>\
<a href=\"/comunidade\">Comunidades</a><a href=\"/defesa\">Defesa</a>\
<a href=\"/pedidos\">Pedidos{badge}</a></header>{body}</body></html>",
        esc(title)
    )
}

/// Formulário com o token anti-falsificação da interface.
pub fn form(action: &str, token: &str, fields: &str, button: &str) -> String {
    format!(
        "<form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"token\" value=\"{}\">\
{fields}<button type=\"submit\">{}</button></form>",
        esc(action),
        esc(token),
        esc(button)
    )
}

pub fn field(label: &str, name: &str, value: &str) -> String {
    format!(
        "<label>{}<input type=\"text\" name=\"{}\" value=\"{}\"></label>",
        esc(label),
        esc(name),
        esc(value)
    )
}

pub fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        esc(name),
        esc(value)
    )
}

pub fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(
            esc("<a href=\"x\">'&"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;"
        );
        assert_eq!(json_str("a\"<\n"), "\"a\\\"\\u003c\\u000a\"");
    }
}
