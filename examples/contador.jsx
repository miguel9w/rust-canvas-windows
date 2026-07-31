function Widget() {
  const [n, setN] = React.useState(0);
  return React.createElement('div', { style: { textAlign: 'center', padding: '32px', fontFamily: 'sans-serif' } },
    React.createElement('h2', { style: { color: '#f59e0b', marginBottom: '16px' } }, '🔥 Contador CLI'),
    React.createElement('button', {
      onClick: () => setN(c => c + 1),
      style: { background: '#f59e0b', color: '#111', border: 'none', padding: '10px 28px', borderRadius: 8, cursor: 'pointer', fontSize: 16 }
    }, 'Cliques: ' + n),
  );
}
