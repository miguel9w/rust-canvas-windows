/// Generates the HTML that wraps a JSX widget with React + Babel Standalone
/// for runtime compilation, injected into WebKit.
/// `config_json` é a configuração atual do app, exposta como
/// `window.__canvasConfig` para o widget de Configurações.
pub fn build_widget_html(jsx: &str, props: &str, config_json: &str) -> String {
    // Escape for embedding inside a JS template literal (backticks, ${).
    // NOTE: do NOT escape < > here — the JSX is compiled by Babel as
    // JavaScript, where < > are valid tokens (arrow functions, comparisons).
    // Escaping them breaks every widget using `=>` (see Unexpected token).
    let safe_jsx = jsx
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace("</script", "<\\/script");
    let safe_props = if props.is_empty() || props == "null" {
        "{}".to_string()
    } else {
        props.to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  html, body {{ 
    width: 100%; height: 100%; overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0a0a0a; color: #ededed;
  }}
  #root {{ width: 100%; height: 100%; overflow: auto; padding: 16px; }}
  /* Scrollbar styling */
  #root::-webkit-scrollbar {{ width: 6px; }}
  #root::-webkit-scrollbar-track {{ background: transparent; }}
  #root::-webkit-scrollbar-thumb {{ background: #2a2a2a; border-radius: 3px; }}
  /* Loading state */
  .loading {{ display: flex; align-items: center; justify-content: center; height: 100%; color: #6366f1; font-size: 14px; }}
  .error {{ color: #f87171; padding: 20px; font-size: 13px; white-space: pre-wrap; }}
</style>
</head>
<body>
  <div id="root"><div class="loading">⬡ Loading widget...</div></div>
  <script src="/vendor/react17.production.min.js"></script>
  <script src="/vendor/react-dom17.production.min.js"></script>
  <script src="/vendor/babel.min.js"></script>
  <script>
    // Bus de eventos do padrão IAS-CANVAS-TOOL. Cada janela é uma webview
    // com window próprio, então o emit é roteado pelo app (bridge nativo):
    // window.webkit.messageHandlers.canvasBus.postMessage(payload) → Rust
    // → run_javascript(__localEmit) em TODAS as janelas.
    window.__canvasBus = window.__canvasBus || (function () {{
      var handlers = {{}};
      function dispatch(payload) {{
        var m;
        try {{ m = JSON.parse(payload); }} catch (e) {{ return; }}
        (handlers[m.evt] || []).slice().forEach(function (fn) {{
          try {{ fn(m.data); }} catch (e) {{ /* listener quebrado não derruba os outros */ }}
        }});
      }}
      return {{
        on: function (evt, fn) {{
          (handlers[evt] = handlers[evt] || []).push(fn);
          return function () {{
            handlers[evt] = (handlers[evt] || []).filter(function (h) {{ return h !== fn; }});
          }};
        }},
        emit: function (evt, data) {{
          var payload = JSON.stringify({{ evt: evt, data: data }});
          if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.canvasBus) {{
            window.webkit.messageHandlers.canvasBus.postMessage(payload);
          }} else {{
            dispatch(payload); // fallback: sem bridge, só a própria janela
          }}
        }},
        __localEmit: dispatch
      }};
    }})();

    // Props injetados pelo window manager (appBus sempre disponível)
    const WIDGET_PROPS = Object.assign({{ appBus: window.__canvasBus }}, {safe_props});
    // Configuração atual do app (preenchida pelo window manager)
    window.__canvasConfig = {config_json};
    
    // Compile and render the JSX widget
    try {{
      const code = Babel.transform(`{safe_jsx}`, {{
        presets: ['react'],
        filename: 'widget.jsx',
      }}).code;
      
      // Babel outputs the widget as a function declaration; wrap it in a
      // factory and invoke it immediately so Component IS the widget function
      // (React calls it with props + hooks context).
      const Component = new Function('props', code + '\nreturn Widget;')();
      const root = React.createElement(Component, WIDGET_PROPS);
      // React 17 required: React 18's concurrent scheduler (even the legacy
      // ReactDOM.render path) never completes on WebKit software rendering
      // (WEBKIT_DISABLE_DMABUF_RENDERER=1) — the script hangs and the page
      // stays on the loading state. React 17 renders synchronously.
      ReactDOM.render(root, document.getElementById('root'));
    }} catch (err) {{
      document.getElementById('root').innerHTML = 
        `<div class="error">❌ ${{err.message}}\n\n${{err.stack?.split('\\n').slice(0,5).join('\\n') || ''}}</div>`;
    }}
  </script>
</body>
</html>"#
    )
}

/// Generate a blank canvas widget (default)
pub fn blank_widget() -> String {
    r#"function Widget(props) {
  const [count, setCount] = React.useState(0);
  return React.createElement('div', { style: { textAlign: 'center', padding: '40px 20px' } },
    React.createElement('h2', { style: { color: '#6366f1', marginBottom: '16px', fontSize: '18px' } }, 'WindowLoom'),
    React.createElement('p', { style: { color: '#a1a1aa', marginBottom: '24px', fontSize: '13px' } }, 'Native desktop widget — edit the JSX to get started'),
    React.createElement('button', {
      onClick: () => setCount(c => c + 1),
      style: { background: '#6366f1', color: 'white', border: 'none', padding: '8px 24px', borderRadius: '8px', cursor: 'pointer', fontSize: '13px' }
    }, 'Clicked ' + count + ' times'),
  );
}"#.to_string()
}

/// A slightly richer demo widget (menu example / quick showcase).
pub fn exemplo_cardapio() -> String {
    r#"function Widget() {
  const itens = ['Pizza', 'Burger', 'Sushi', 'Taco'];
  return React.createElement('div', { style: { padding: 24, fontFamily: 'sans-serif' } },
    React.createElement('h2', { style: { color: '#8b5cf6', marginBottom: 12 } }, 'Cardapio'),
    React.createElement('ul', { style: { listStyle: 'none', padding: 0 } },
      itens.map(i => React.createElement('li', { key: i, style: { background: '#1e1e2e', margin: '8px 0', padding: '12px 16px', borderRadius: 8, color: '#cdd6f4' } }, i))
    ),
  );
}"#.to_string()
}

/// Widget de Configurações (aberto pelo tray). Lê `window.__canvasConfig`
/// (injetado pelo window manager) e salva via `configBus` (bridge JS→Rust).
pub fn config_widget() -> String {
    r#"function Widget({ appBus }) {
  var c = React.useState(window.__canvasConfig || { width: 600, height: 400, autostart: false });
  var saved = React.useState(false);
  function save() {
    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.configBus) {
      window.webkit.messageHandlers.configBus.postMessage(JSON.stringify(c[0]));
      saved[1](true);
      setTimeout(function () { saved[1](false); }, 1500);
    }
  }
  function upd(key) {
    return function (e) {
      var v = e.target.type === 'checkbox' ? e.target.checked : parseInt(e.target.value) || 0;
      var o = {}; o[key] = v;
      c[1](Object.assign({}, c[0], o));
    };
  }
  var s = {
    wrap: { padding: 24, fontFamily: 'sans-serif' },
    h2: { color: '#818cf8', margin: '0 0 20px', fontSize: 17 },
    row: { display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12, color: '#cbd5e1', fontSize: 13 },
    input: { background: '#0f172a', color: '#e2e8f0', border: '1px solid #334155', borderRadius: 6, padding: '4px 8px', width: 70, fontSize: 13 },
    btn: { background: '#6366f1', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 22px', fontSize: 13, fontWeight: 600, cursor: 'pointer', marginTop: 8 },
    saved: { color: '#4ade80', fontSize: 13, marginLeft: 10 }
  };
  return React.createElement('div', { style: s.wrap },
    React.createElement('h2', { style: s.h2 }, 'Configurações'),
    React.createElement('div', { style: s.row },
      React.createElement('input', { type: 'checkbox', checked: c[0].autostart, onChange: upd('autostart') }),
      React.createElement('span', null, 'Iniciar com o sistema')
    ),
    React.createElement('div', { style: s.row },
      React.createElement('span', null, 'Largura padrão'),
      React.createElement('input', { type: 'number', value: c[0].width, onChange: upd('width'), style: s.input })
    ),
    React.createElement('div', { style: s.row },
      React.createElement('span', null, 'Altura padrão'),
      React.createElement('input', { type: 'number', value: c[0].height, onChange: upd('height'), style: s.input })
    ),
    React.createElement('button', { onClick: save, style: s.btn }, 'Salvar'),
    saved[0] ? React.createElement('span', { style: s.saved }, 'Salvo!') : null
  );
}"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_js_tokens_that_use_angle_brackets() {
        // Regression test: escaping < > used to turn `=>` into `=&gt;`,
        // breaking Babel with "Unexpected token" on every arrow function.
        let html = build_widget_html("function W(){ return x => x + 1; }", "{}", "{}");
        assert!(html.contains("x => x + 1"), "arrow function must survive verbatim");
        assert!(!html.contains("&gt;"), "no HTML-entity mangling of JS");
        assert!(!html.contains("&lt;"), "no HTML-entity mangling of JS");
    }

    #[test]
    fn escapes_template_literal_and_script_close() {
        let html = build_widget_html("const s = `tick ${1}`;", "{}", "{}");
        assert!(html.contains("\\`tick \\${1}\\`"), "backtick and dollar-brace escaped for template literal");
        let html2 = build_widget_html("const s = '</script>';", "{}", "{}");
        assert!(
            html2.contains("<\\/script>"),
            "script close inside JSX escaped (the page's own </script> tags are unrelated)"
        );
    }

    #[test]
    fn injects_props_json() {
        let html = build_widget_html("function W(){}", r#"{"name":"Miguel"}"#, "{}");
        let expected = "Object.assign({ appBus: window.__canvasBus }, {\"name\":\"Miguel\"});";
        assert!(html.contains(expected));
    }

    #[test]
    fn provides_app_bus() {
        // IAS-CANVAS-TOOL widgets receive { appBus } and use emit/on across
        // windows; the bus must be shared per-process (window.__canvasBus).
        let html = build_widget_html("function W(){}", "{}", "{}");
        assert!(html.contains("window.__canvasBus = window.__canvasBus ||"));
        assert!(html.contains("appBus: window.__canvasBus"));
        assert!(html.contains("on: function (evt, fn)"));
        assert!(html.contains("emit: function (evt, data)"));
        // on() must return an unsubscribe fn (React useEffect cleanup)
        assert!(html.contains("return function ()"));
    }

    #[test]
    fn uses_react17_sync_render() {
        // Regression: React 18's concurrent scheduler (createRoot AND legacy
        // ReactDOM.render) never completes on WebKit software rendering
        // (WEBKIT_DISABLE_DMABUF_RENDERER=1) — the script hangs, page stays
        // on loading. React 17 renders synchronously and works.
        let html = build_widget_html("function W(){}", "{}", "{}");
        assert!(html.contains("react17.production.min.js"));
        assert!(html.contains("react-dom17.production.min.js"));
        assert!(html.contains("ReactDOM.render(root, document.getElementById('root'))"));
        assert!(html.contains("return Widget;"), "component factory must return Widget");
        assert!(!html.contains("createRoot"), "createRoot must not be used");
        assert!(!html.contains("react.production.min.js\""), "React 18 must not be referenced");
        assert!(!html.contains("react-dom.production.min.js\""), "ReactDOM 18 must not be referenced");
    }
}
