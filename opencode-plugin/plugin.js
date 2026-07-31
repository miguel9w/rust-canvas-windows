// == OpenCode Plugin: Rust Canvas Windows ==
// Register a `create_window` tool that spawns native desktop windows
// rendering JSX widgets via WebSocket IPC.

const IPC_PORT = process.env.RUST_CANVAS_PORT || '8081';
const IPC_HOST = '127.0.0.1';

let ws = null;

function ensureConnection() {
  if (ws && ws.readyState === WebSocket.OPEN) return Promise.resolve(ws);
  return new Promise((resolve, reject) => {
    ws = new WebSocket(`ws://${IPC_HOST}:${IPC_PORT}`);
    ws.onopen = () => resolve(ws);
    ws.onerror = (err) => {
      ws = null;
      reject(new Error(`Cannot connect to Rust Canvas Windows at ws://${IPC_HOST}:${IPC_PORT}. Is the app running?`));
    };
    ws.onclose = () => { ws = null; };
  });
}

async function sendCommand(cmd) {
  const connection = await ensureConnection();
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Response timeout')), 5000);
    connection.onmessage = (event) => {
      clearTimeout(timeout);
      try {
        resolve(JSON.parse(event.data));
      } catch (e) {
        reject(e);
      }
    };
    connection.send(JSON.stringify(cmd));
  });
}

module.exports = {
  name: 'rust-canvas-windows',
  description: 'Create, update, and manage native desktop windows that render JSX widgets in real time.',

  tools: [
    {
      name: 'create_window',
      description: 'Open a native operating system window that renders a React JSX component. ' +
        'The JSX is compiled at runtime by Babel Standalone. Use React.createElement or JSX syntax. ' +
        'Each window is a real desktop window (not a browser tab).',

      parameters: {
        type: 'object',
        properties: {
          jsx: {
            type: 'string',
            description: 'React JSX component code as a string. ' +
              'Write a function like: function Widget(props) { return <div>...</div>; } ' +
              'Use React.createElement if JSX has issues. ' +
              'Available: React, React.createElement, React.useState, React.useEffect, React.useRef.',
          },
          title: {
            type: 'string',
            description: 'Window title (shown in the title bar).',
            default: 'Rust Canvas Widget',
          },
          width: {
            type: 'number',
            description: 'Window width in pixels.',
            default: 600,
          },
          height: {
            type: 'number',
            description: 'Window height in pixels.',
            default: 400,
          },
        },
        required: ['jsx'],
      },

      handler: async ({ jsx, title, width, height }) => {
        const cmd = {
          action: 'CREATE_WINDOW',
          title: title || 'Rust Canvas Widget',
          jsx,
          width: width || 600,
          height: height || 400,
        };
        const result = await sendCommand(cmd);
        if (result.success) {
          return `✅ Window created! ID: ${result.id}. The native window should now be visible on your desktop.`;
        }
        throw new Error(result.error || 'Failed to create window');
      },
    },

    {
      name: 'update_window',
      description: 'Update the content of an existing native desktop window with new JSX.',

      parameters: {
        type: 'object',
        properties: {
          id: {
            type: 'string',
            description: 'The ID of the window to update (returned by create_window).',
          },
          jsx: {
            type: 'string',
            description: 'New JSX component code to render.',
          },
        },
        required: ['id', 'jsx'],
      },

      handler: async ({ id, jsx }) => {
        const result = await sendCommand({ action: 'UPDATE_WINDOW', id, jsx });
        if (result.success) {
          return `✅ Window ${id} updated with new content.`;
        }
        throw new Error(result.error || 'Failed to update window');
      },
    },

    {
      name: 'close_window',
      description: 'Close a native desktop window by its ID.',

      parameters: {
        type: 'object',
        properties: {
          id: {
            type: 'string',
            description: 'The ID of the window to close.',
          },
        },
        required: ['id'],
      },

      handler: async ({ id }) => {
        const result = await sendCommand({ action: 'CLOSE_WINDOW', id });
        if (result.success) {
          return `✅ Window ${id} closed.`;
        }
        throw new Error(result.error || 'Failed to close window');
      },
    },
  ],
};
