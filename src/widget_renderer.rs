/// Generates the HTML that wraps a JSX widget with React + Babel Standalone
/// for runtime compilation, injected into WebKit.
pub fn build_widget_html(jsx: &str, props: &str) -> String {
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
    // Props injected by the window manager
    const WIDGET_PROPS = {safe_props};
    
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
    React.createElement('h2', { style: { color: '#6366f1', marginBottom: '16px', fontSize: '18px' } }, '🧊 Rust Canvas Window'),
    React.createElement('p', { style: { color: '#a1a1aa', marginBottom: '24px', fontSize: '13px' } }, 'Native desktop widget — edit the JSX to get started'),
    React.createElement('button', {
      onClick: () => setCount(c => c + 1),
      style: { background: '#6366f1', color: 'white', border: 'none', padding: '8px 24px', borderRadius: '8px', cursor: 'pointer', fontSize: '13px' }
    }, 'Clicked ' + count + ' times'),
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
        let html = build_widget_html("function W(){ return x => x + 1; }", "{}");
        assert!(html.contains("x => x + 1"), "arrow function must survive verbatim");
        assert!(!html.contains("&gt;"), "no HTML-entity mangling of JS");
        assert!(!html.contains("&lt;"), "no HTML-entity mangling of JS");
    }

    #[test]
    fn escapes_template_literal_and_script_close() {
        let html = build_widget_html("const s = `tick ${1}`;", "{}");
        assert!(html.contains("\\`tick \\${1}\\`"), "backtick and dollar-brace escaped for template literal");
        let html2 = build_widget_html("const s = '</script>';", "{}");
        assert!(
            html2.contains("<\\/script>"),
            "script close inside JSX escaped (the page's own </script> tags are unrelated)"
        );
    }

    #[test]
    fn injects_props_json() {
        let html = build_widget_html("function W(){}", r#"{"name":"Miguel"}"#);
        assert!(html.contains(r#"const WIDGET_PROPS = {"name":"Miguel"};"#));
    }

    #[test]
    fn uses_react17_sync_render() {
        // Regression: React 18's concurrent scheduler (createRoot AND legacy
        // ReactDOM.render) never completes on WebKit software rendering
        // (WEBKIT_DISABLE_DMABUF_RENDERER=1) — the script hangs, page stays
        // on loading. React 17 renders synchronously and works.
        let html = build_widget_html("function W(){}", "{}");
        assert!(html.contains("react17.production.min.js"));
        assert!(html.contains("react-dom17.production.min.js"));
        assert!(html.contains("ReactDOM.render(root, document.getElementById('root'))"));
        assert!(html.contains("return Widget;"), "component factory must return Widget");
        assert!(!html.contains("createRoot"), "createRoot must not be used");
        assert!(!html.contains("react.production.min.js\""), "React 18 must not be referenced");
        assert!(!html.contains("react-dom.production.min.js\""), "ReactDOM 18 must not be referenced");
    }
}
