/// Generates the HTML that wraps a JSX widget with React + Babel Standalone
/// for runtime compilation, injected into WebKit.
pub fn build_widget_html(jsx: &str, props: &str) -> String {
    // Sanitize JSX input — basic escaping to prevent injection
    let safe_jsx = jsx.replace('<', "&lt;").replace('>', "&gt;");
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
  <script crossorigin src="https://unpkg.com/react@18/umd/react.production.min.js"></script>
  <script crossorigin src="https://unpkg.com/react-dom@18/umd/react-dom.production.min.js"></script>
  <script src="https://unpkg.com/@babel/standalone/babel.min.js"></script>
  <script>
    // Props injected by the window manager
    const WIDGET_PROPS = {safe_props};
    
    // Compile and render the JSX widget
    try {{
      const code = Babel.transform(`{safe_jsx}`, {{
        presets: ['react'],
        filename: 'widget.jsx',
      }}).code;
      
      const Component = new Function('React', 'props', code);
      const root = React.createElement(Component, WIDGET_PROPS);
      ReactDOM.createRoot(document.getElementById('root')).render(root);
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
